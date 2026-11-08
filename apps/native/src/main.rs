#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Bitbots in Rust (GPUI), slice 1: an installable core.
//!
//! The window is a native preview of the desktop app: it starts its own
//! `bitbotsd.exe`, reads the daemon's snapshot, follows its event stream, and
//! shows the projects tree, the conversations column and one conversation. Later
//! slices add the remaining screens.

mod art;
mod daemon;
mod files;
mod notify;
mod paths;
mod settings_file;
mod state;
mod theme;
mod trace;
mod ui;

use gpui::{
    px, size, App, AppContext, Application, Bounds, TitlebarOptions, WindowAppearance,
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

        // Haze is frosted glass: the blur is drawn by Windows behind the window
        // and the app paints a translucent tint over it. The tint is the
        // documented 80% fallback on its own, so the window stays readable when
        // Windows draws no backdrop.
        let bounds = Bounds::centered(None, size(px(1360.0), px(860.0)), cx);
        cx.open_window(
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
            move |window, cx| {
                // Windows' theme at start, and the observer keeps it in step.
                let appearance = window.appearance();
                let system_is_light = appearance != WindowAppearance::Dark
                    && appearance != WindowAppearance::VibrantDark;
                let root = cx.new(|cx| ui::Root::new(db_path, system_is_light, cx));
                let handle = root.read(cx).composer_handle(cx);
                window.focus(&handle, cx);
                root.update(cx, |root, cx| root.watch_appearance(window, cx));
                root
            },
        )
        .expect("the main window should open");

        // A smoke run closes itself, so the close path (and the daemon shutdown
        // it triggers) can be checked without a person at the keyboard.
        if let Some(seconds) = std::env::var("BITBOTS_NATIVE_SMOKE_SECS").ok().and_then(|value| value.parse::<u64>().ok()) {
            cx.spawn(async move |cx| {
                cx.background_executor().timer(std::time::Duration::from_secs(seconds)).await;
                trace::log("smoke run: closing the window");
                cx.update(|cx| cx.quit());
            })
            .detach();
        }

        cx.on_window_closed(|cx| {
            trace::log("window closed: quitting");
            cx.quit();
        })
        .detach();

        cx.activate(true);
    });
    trace::log("exited");
}
