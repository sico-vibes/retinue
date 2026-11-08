#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Bitbots in Rust (GPUI): the main window, the companion and dictation.
//!
//! The main window is slice 1 and 2's (an installable core plus the look, the
//! team and new projects). Slice 5 adds the floating companion window, its sounds
//! and on-device dictation, and shares the one daemon connection between the two
//! windows.

mod art;
mod companion;
mod daemon;
mod dictation;
mod files;
mod hotkeys;
mod notify;
mod paths;
mod settings_file;
mod sound;
mod state;
mod theme;
mod trace;
mod ui;

use std::sync::Arc;

use gpui::{
    px, size, AnyWindowHandle, App, AppContext, Application, Bounds, TitlebarOptions, WindowAppearance,
    WindowBackgroundAppearance, WindowBounds, WindowOptions,
};

fn main() {
    // First, before any window or daemon exists: a panic must reach the log, or
    // a crash looks like a silent quit.
    trace::install_panic_hook();
    Application::new().run(|cx: &mut App| {
        cx.bind_keys(ui::text_input::key_bindings());
        cx.bind_keys(ui::search::key_bindings());

        let db_path = std::env::var_os("BLOBLEX_DB_PATH")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(paths::native_db_path);
        trace::log(format!("starting {} (database: {})", paths::PRODUCT_NAME, db_path.display()));
        let settings = settings_file::read(&paths::settings_file_path());

        // The state both windows read. The companion never opens a daemon
        // connection of its own: this entity mirrors the main window's snapshot
        // and events, and sends commands back through the same `Root`.
        let shared = cx.new(|_| companion::Shared::new());

        // Sounds: off unless the owner switched them on, and no audio thread is
        // started until a cue actually plays.
        let sound = Arc::new(sound::SoundEngine::new());
        sound.set_enabled(settings.companion.sounds);
        sound.set_volume(settings.companion.sound_volume);

        let dictation = cx.new(|cx| dictation::Dictation::new(settings.dictation.clone(), sound.clone(), cx));

        // Haze is frosted glass: the blur is drawn by Windows behind the window
        // and the app paints a translucent tint over it. The tint is the
        // documented 80% fallback on its own, so the window stays readable when
        // Windows draws no backdrop.
        let bounds = Bounds::centered(None, size(px(1360.0), px(860.0)), cx);
        let main_window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some(paths::PRODUCT_NAME.into()),
                        appears_transparent: false,
                        traffic_light_position: None,
                    }),
                    window_background: WindowBackgroundAppearance::MicaAltBackdrop,
                    window_min_size: Some(size(px(1000.0), px(680.0))),
                    ..Default::default()
                },
                {
                    let shared = shared.clone();
                    move |window, cx| {
                        // Windows' theme at start, and the observer keeps it in step.
                        let appearance = window.appearance();
                        let system_is_light = appearance != WindowAppearance::Dark
                            && appearance != WindowAppearance::VibrantDark;
                        let root = cx.new(|cx| ui::Root::new(db_path, system_is_light, shared, cx));
                        let handle = root.read(cx).composer_handle(cx);
                        window.focus(&handle, cx);
                        root.update(cx, |root, cx| root.watch_appearance(window, cx));
                        root
                    }
                },
            )
            .expect("the main window should open");

        // The main window's side of the shared state: commands go through it,
        // and dictation knows where its words belong.
        let main_handle: AnyWindowHandle = main_window.into();
        let main_hwnd = companion::window::hwnd_of_handle(cx, main_handle).unwrap_or(0);
        {
            let root = main_window.entity(cx).expect("the main window has its root").downgrade();
            shared.update(cx, |shared, cx| {
                shared.attach_root(root.clone());
                shared.main_window = Some(main_handle);
                shared.set_selection(None, None, cx);
            });
            dictation.update(cx, |dictation, _| dictation.attach_main(root, main_handle, main_hwnd));
        }

        // The companion: its own always-on-top window, the same snapshot and
        // events, Haze's smoke palette.
        let companion_window = companion::view::open(cx, shared.clone(), dictation.clone(), sound.clone(), settings.companion.clone())
            .expect("the companion window should open");
        let companion_handle: AnyWindowHandle = companion_window.into();
        let companion_hwnd = companion::window::hwnd_of_handle(cx, companion_handle).unwrap_or(0);
        companion::window::set_topmost(cx.background_executor(), companion_hwnd);
        shared.update(cx, |shared, _| shared.companion_window = Some(companion_handle));

        // The dictation bubble: hidden until a session starts, above the island.
        let bubble_window = dictation::bubble::open(dictation.clone(), cx).expect("the dictation bubble should open");
        let bubble_handle: AnyWindowHandle = bubble_window.into();
        let bubble_hwnd = companion::window::hwnd_of_handle(cx, bubble_handle).unwrap_or(0);
        companion::window::hide(cx.background_executor(), bubble_hwnd);
        dictation.update(cx, |dictation, _| dictation.attach_bubble(bubble_handle, bubble_hwnd));

        // Ctrl+Alt+B always; Ctrl+E while dictation is switched on.
        let (hotkeys, presses) = hotkeys::start(&[hotkeys::Hotkey::Companion]);
        let enabled = settings.dictation.enabled;
        dictation.update(cx, |dictation, _| dictation.configure(enabled, Some(&hotkeys)));
        // The manager lives for the whole run: it owns the hotkey thread, and
        // letting it drop would release both shortcuts.
        let hotkeys: &'static hotkeys::HotkeyManager = Box::leak(Box::new(hotkeys));
        dictation.update(cx, |dictation, _| dictation.warm());
        let companion_rect = companion::window::window_rect(companion_hwnd);
        trace::log(format!(
            "companion: window {} at {:?} visible {} (shortcuts: {}, {}), dictation {} (shortcut {})",
            if settings.companion.visible { "shown" } else { "hidden" },
            companion_rect,
            companion::window::is_visible(companion_hwnd),
            hotkeys::Hotkey::Companion.label(),
            if hotkeys.refusals.is_empty() { "registered" } else { "unavailable" },
            if enabled { "on" } else { "off" },
            hotkeys::Hotkey::Dictation.label(),
        ));

        // The shortcut presses arrive on their own thread's channel.
        let companion_for_keys = companion_window.entity(cx).expect("the companion view").clone();
        let dictation_for_keys = dictation.clone();
        let main_for_keys = main_handle;
        cx.spawn(async move |cx| {
            while let Ok(pressed) = presses.recv().await {
                cx.update(|cx| match pressed {
                    hotkeys::Hotkey::Companion => {
                        companion_for_keys.update(cx, |companion, cx| {
                            if companion.mode() == companion::Mode::Hidden {
                                companion.reveal(cx);
                                companion::window::activate(cx.background_executor(), companion.hwnd());
                            } else {
                                companion.toggle(cx);
                            }
                            trace::log(format!("companion: shortcut, now {}", companion.mode().key()));
                        });
                    }
                    hotkeys::Hotkey::Dictation => {
                        dictation_for_keys.update(cx, |dictation, cx| {
                            dictation.toggle(cx);
                            trace::log(format!("dictation: shortcut, now {}", dictation.phase.key()));
                        });
                        let _ = main_for_keys;
                    }
                });
            }
        })
        .detach();

        // A smoke run closes itself, so the close path (and the daemon shutdown
        // it triggers) can be checked without a person at the keyboard.
        if let Some(seconds) = std::env::var("BITBOTS_NATIVE_SMOKE_SECS").ok().and_then(|value| value.parse::<u64>().ok()) {
            // The companion's hide and show, without a keyboard: the live check
            // cannot press Ctrl+Alt+B when another app already owns it.
            let toggle_at = std::env::var("BITBOTS_NATIVE_TOGGLE_COMPANION_AT").ok().and_then(|value| value.parse::<u64>().ok());
            if let Some(at) = toggle_at {
                let companion_for_smoke = companion_window.entity(cx).expect("the companion view").clone();
                let hwnd = companion_hwnd;
                cx.spawn(async move |cx| {
                    cx.background_executor().timer(std::time::Duration::from_secs(at)).await;
                    cx.update(|cx| {
                        companion_for_smoke.update(cx, |companion, cx| companion.toggle(cx));
                    });
                    // The Win32 call is deferred, so read visibility after it.
                    cx.background_executor().timer(std::time::Duration::from_millis(400)).await;
                    cx.update(|cx| {
                        companion_for_smoke.update(cx, |companion, _| {
                            trace::log(format!(
                                "live check: companion toggled to {} (window visible: {}, rect {:?})",
                                companion.mode().key(),
                                companion::window::is_visible(companion.hwnd().unwrap_or(hwnd)),
                                companion::window::window_rect(companion.hwnd().unwrap_or(hwnd))
                            ));
                        });
                    });
                    cx.background_executor().timer(std::time::Duration::from_millis(1_100)).await;
                    cx.update(|cx| {
                        companion_for_smoke.update(cx, |companion, cx| companion.toggle(cx));
                    });
                    cx.background_executor().timer(std::time::Duration::from_millis(400)).await;
                    cx.update(|cx| {
                        companion_for_smoke.update(cx, |companion, _| {
                            trace::log(format!(
                                "live check: companion toggled back to {} (window visible: {}, rect {:?})",
                                companion.mode().key(),
                                companion::window::is_visible(companion.hwnd().unwrap_or(hwnd)),
                                companion::window::window_rect(companion.hwnd().unwrap_or(hwnd))
                            ));
                        });
                    });
                })
                .detach();
            }
            let dictation_for_smoke = dictation.clone();
            let dictation_probe = std::env::var("BITBOTS_NATIVE_START_DICTATION").is_ok();
            cx.spawn(async move |cx| {
                if dictation_probe {
                    cx.background_executor().timer(std::time::Duration::from_millis(1_500)).await;
                    cx.update(|cx| {
                        dictation_for_smoke.update(cx, |dictation, cx| {
                            dictation.toggle(cx);
                            trace::log(format!(
                                "live check: dictation started by itself, now {} ({})",
                                dictation.phase.key(),
                                if dictation.error.is_empty() { "no error".to_owned() } else { format!("it says: {}", dictation.error) }
                            ));
                        });
                    });
                    cx.background_executor().timer(std::time::Duration::from_millis(2_000)).await;
                }
                cx.background_executor().timer(std::time::Duration::from_secs(seconds)).await;
                trace::log("smoke run: closing the window");
                cx.update(|cx| cx.quit());
            })
            .detach();
        }

        // Closing the main window quits, and the daemon child stops with it.
        // The companion and the bubble are hidden rather than closed, so the
        // window list is never empty while the main window is alive.
        cx.on_window_closed(move |cx| {
            if !cx.windows().contains(&main_handle) {
                trace::log("the main window closed: quitting");
                cx.quit();
            }
        })
        .detach();

        cx.activate(true);
    });
    trace::log("exited");
}
