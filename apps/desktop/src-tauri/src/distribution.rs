//! How this copy of Retinue was installed: from the Microsoft Store (an MSIX package), from
//! the NSIS installer, or, on Linux, as an AppImage. A Store copy updates through the
//! Store, so the built-in updater stays off, and it starts at sign-in through the package's
//! startup task (`packaging/msix/AppxManifest.xml`), because a packaged app's writes to the
//! Run key do not reach the real registry. On Linux nothing is ever packaged: a `.deb` or
//! `.rpm` copy belongs to the package manager, and only an AppImage is updated from inside
//! the app (`tauri-plugin-updater` replaces the file `$APPIMAGE` names).

use serde::Serialize;

/// The startup task's id, as declared in the MSIX manifest.
#[cfg(windows)]
const STARTUP_TASK_ID: &str = "RetinueStartup";

/// True when the process runs with package identity (installed from the Microsoft Store).
pub fn is_packaged() -> bool {
    use std::sync::OnceLock;
    static PACKAGED: OnceLock<bool> = OnceLock::new();
    *PACKAGED.get_or_init(detect_packaged)
}

#[cfg(windows)]
fn detect_packaged() -> bool {
    use windows_sys::Win32::Foundation::{APPMODEL_ERROR_NO_PACKAGE, ERROR_INSUFFICIENT_BUFFER};
    use windows_sys::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;
    let mut length = 0u32;
    // SAFETY: a zero length with a null buffer only asks for the length.
    let status = unsafe { GetCurrentPackageFullName(&mut length, std::ptr::null_mut()) };
    match status {
        ERROR_INSUFFICIENT_BUFFER => true,
        APPMODEL_ERROR_NO_PACKAGE => false,
        _ => false,
    }
}

#[cfg(not(windows))]
fn detect_packaged() -> bool {
    false
}

/// True when this process runs from an AppImage. `tauri-plugin-updater` replaces
/// the file `$APPIMAGE` names, so that variable is the whole test; a `.deb`,
/// `.rpm` or development build never has it.
#[cfg(target_os = "linux")]
fn detect_app_image() -> bool {
    std::env::var_os("APPIMAGE").is_some_and(|path| !path.is_empty())
}

#[cfg(not(target_os = "linux"))]
fn detect_app_image() -> bool {
    false
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Distribution {
    /// Installed from the Microsoft Store.
    Store,
    /// Installed with the NSIS installer, or a development build.
    Direct,
    /// A Linux AppImage, which updates itself (added 27 November 2026, Linux port).
    AppImage,
}

pub fn distribution() -> Distribution {
    distribution_from(is_packaged(), detect_app_image())
}

/// The rule, separated from the environment it reads so both shapes can be tested
/// on whichever platform runs the tests. A Store package is never an AppImage:
/// package identity wins.
fn distribution_from(packaged: bool, app_image: bool) -> Distribution {
    if packaged {
        Distribution::Store
    } else if app_image {
        Distribution::AppImage
    } else {
        Distribution::Direct
    }
}

/// Whether the built-in updater can replace this copy in place.
///
/// Windows and macOS builds update themselves (the NSIS copy; a Store copy does
/// not). On Linux only an AppImage does: a `.deb` or `.rpm` copy is the package
/// manager's, so the two values have to agree.
pub fn updates_itself() -> bool {
    !is_packaged() && (cfg!(target_os = "linux") == detect_app_image())
}

#[tauri::command]
pub fn app_distribution() -> Distribution {
    distribution()
}

/// Whether Retinue starts at sign-in, for a Store copy.
#[tauri::command]
pub async fn startup_task_enabled() -> Result<bool, String> {
    if !is_packaged() {
        return Err("not_packaged".into());
    }
    tauri::async_runtime::spawn_blocking(startup::enabled)
        .await
        .map_err(|_| "unavailable".to_string())?
}

/// Turns starting at sign-in on or off, for a Store copy. Windows keeps the last word: when
/// the owner turned Retinue off in Windows Settings, only Settings can turn it back on.
#[tauri::command]
pub async fn set_startup_task_enabled(enabled: bool) -> Result<bool, String> {
    if !is_packaged() {
        return Err("not_packaged".into());
    }
    tauri::async_runtime::spawn_blocking(move || startup::set_enabled(enabled))
        .await
        .map_err(|_| "unavailable".to_string())?
}

#[cfg(windows)]
mod startup {
    use super::STARTUP_TASK_ID;
    use windows::core::HSTRING;
    use windows::ApplicationModel::{StartupTask, StartupTaskState};

    pub(super) const DISABLED_IN_SETTINGS: &str =
        "Windows Settings has Retinue turned off at startup. Turn it on in Settings, Apps, Startup.";
    pub(super) const DISABLED_BY_POLICY: &str = "Your organisation's policy controls whether Retinue starts with Windows.";

    fn task() -> Result<StartupTask, String> {
        StartupTask::GetAsync(&HSTRING::from(STARTUP_TASK_ID))
            .and_then(|operation| operation.join())
            .map_err(|_| "The startup task is unavailable.".to_string())
    }

    fn is_on(state: StartupTaskState) -> bool {
        state == StartupTaskState::Enabled || state == StartupTaskState::EnabledByPolicy
    }

    pub(super) fn enabled() -> Result<bool, String> {
        let state = task()?.State().map_err(|_| "The startup task is unavailable.".to_string())?;
        Ok(is_on(state))
    }

    pub(super) fn set_enabled(enabled: bool) -> Result<bool, String> {
        let task = task()?;
        let state = task.State().map_err(|_| "The startup task is unavailable.".to_string())?;
        if enabled {
            if is_on(state) {
                return Ok(true);
            }
            if state == StartupTaskState::DisabledByUser {
                return Err(DISABLED_IN_SETTINGS.into());
            }
            if state == StartupTaskState::DisabledByPolicy {
                return Err(DISABLED_BY_POLICY.into());
            }
            let next = task
                .RequestEnableAsync()
                .and_then(|operation| operation.join())
                .map_err(|_| "The startup task could not be turned on.".to_string())?;
            if next == StartupTaskState::DisabledByUser {
                return Err(DISABLED_IN_SETTINGS.into());
            }
            Ok(is_on(next))
        } else {
            if state == StartupTaskState::EnabledByPolicy {
                return Err(DISABLED_BY_POLICY.into());
            }
            if is_on(state) {
                task.Disable().map_err(|_| "The startup task could not be turned off.".to_string())?;
            }
            Ok(false)
        }
    }
}

#[cfg(not(windows))]
mod startup {
    pub(super) fn enabled() -> Result<bool, String> {
        Err("not_packaged".into())
    }
    pub(super) fn set_enabled(_enabled: bool) -> Result<bool, String> {
        Err("not_packaged".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_test_process_is_not_packaged() {
        assert!(!is_packaged());
        assert_eq!(distribution(), Distribution::Direct);
    }

    #[test]
    fn distribution_serialises_in_camel_case() {
        assert_eq!(serde_json::to_string(&Distribution::Store).unwrap(), "\"store\"");
        assert_eq!(serde_json::to_string(&Distribution::Direct).unwrap(), "\"direct\"");
        assert_eq!(serde_json::to_string(&Distribution::AppImage).unwrap(), "\"appImage\"");
    }

    #[test]
    fn a_store_package_wins_over_an_app_image_and_a_loose_copy_is_direct() {
        assert_eq!(distribution_from(true, false), Distribution::Store);
        assert_eq!(distribution_from(true, true), Distribution::Store, "package identity wins");
        assert_eq!(distribution_from(false, true), Distribution::AppImage);
        assert_eq!(distribution_from(false, false), Distribution::Direct);
    }
}
