//! The floating windows' Win32 side: frameless, always on top, hidden and shown
//! on demand, dragged and resized from their own edges.
//!
//! At the pinned GPUI revision a window is created with `WindowKind::PopUp`
//! (which is `WS_EX_TOOLWINDOW`: no caption, no taskbar entry, and no
//! `WS_THICKFRAME`, so Windows draws no resize border), and there is no API for
//! topmost, hide, show, move or resize-from-the-outside. The React app draws its
//! own handles and drives them through Tauri commands; this does the same through
//! the window's own `HWND`, which GPUI hands out via `HasWindowHandle`.
//!
//! Everything here is the shell, not the design: the sizes and the clamps live in
//! [`crate::companion::layout`] and [`crate::companion::positions`].

#![allow(dead_code)]  // A ported surface: the helpers are complete and
// slice 6's settings page and slice 7 wire the rest.

use std::ptr;

use gpui::{AnyWindowHandle, App};
use raw_window_handle::HasWindowHandle;
use raw_window_handle::RawWindowHandle;
use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, MonitorFromWindow, HDC, HMONITOR, MONITORINFOEXW,
    MONITOR_DEFAULTTONEAREST,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetForegroundWindow, GetWindowRect, IsWindowVisible, SetForegroundWindow,
    SetWindowPos, ShowWindow, HWND_TOPMOST, SW_HIDE, SW_SHOW, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, SWP_NOZORDER,
};

use crate::companion::positions::{Rect, WorkArea};

/// The window's `HWND` as a plain number, so a view can keep it without holding
/// a GPUI borrow.
pub fn hwnd_of(window: &gpui::Window) -> Option<isize> {
    // Window::window_handle answers with GPUI's own handle; the raw one is the
    // trait's, called explicitly.
    let handle = <gpui::Window as HasWindowHandle>::window_handle(window).ok()?;
    match handle.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
        _ => None,
    }
}

/// The `HWND` of a window GPUI already opened.
pub fn hwnd_of_handle(cx: &mut App, window: AnyWindowHandle) -> Option<isize> {
    let mut found = None;
    let _ = window.update(cx, |_, window, _| {
        found = hwnd_of(window);
    });
    found
}

fn hwnd(value: isize) -> HWND {
    value as HWND
}

/// Run one window operation on the background executor. `ShowWindow` and
/// `SetWindowPos` send window messages that re-enter GPUI, which panics while the
/// app's own state is borrowed; GPUI's `Window::resize` defers them the same way.
pub fn defer(executor: &gpui::BackgroundExecutor, op: impl FnOnce() + Send + 'static) {
    executor.spawn(async move { op() }).detach();
}

/// `SetWindowPos` with `HWND_TOPMOST`: the companion sits over other windows
/// without taking the keyboard.
pub fn set_topmost(executor: &gpui::BackgroundExecutor, value: isize) {
    defer(executor, move || unsafe {
        SetWindowPos(hwnd(value), HWND_TOPMOST as HWND, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    });
}

pub fn show(executor: &gpui::BackgroundExecutor, value: isize) {
    defer(executor, move || unsafe {
        ShowWindow(hwnd(value), SW_SHOW);
        // Shown without being focused: the owner's caret stays where it was.
        SetWindowPos(hwnd(value), HWND_TOPMOST as HWND, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    });
}

/// Bring a window to the front and give it the keyboard, the way the owner's
/// click on "Open in Bitbots" does.
pub fn activate(executor: &gpui::BackgroundExecutor, value: Option<isize>) {
    let Some(value) = value else { return };
    defer(executor, move || unsafe {
        ShowWindow(hwnd(value), SW_SHOW);
        SetForegroundWindow(hwnd(value));
    });
}

pub fn hide(executor: &gpui::BackgroundExecutor, value: isize) {
    defer(executor, move || unsafe {
        ShowWindow(hwnd(value), SW_HIDE);
    });
}

pub fn is_visible(value: isize) -> bool {
    unsafe { IsWindowVisible(hwnd(value)) != 0 }
}

/// The window's rectangle in physical pixels.
pub fn window_rect(value: isize) -> Option<Rect> {
    let mut rect: RECT = unsafe { std::mem::zeroed() };
    let ok = unsafe { GetWindowRect(hwnd(value), &mut rect) };
    if ok == 0 {
        return None;
    }
    Some((rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top))
}

/// Move and/or resize the window, without activating it or changing its place in
/// the z-order.
pub fn place(executor: &gpui::BackgroundExecutor, value: isize, x: i32, y: i32, width: i32, height: i32) {
    defer(executor, move || unsafe {
        SetWindowPos(hwnd(value), ptr::null_mut(), x, y, width, height, SWP_NOZORDER | SWP_NOACTIVATE);
    });
}

/// The cursor in physical screen pixels (`GetCursorPos`).
pub fn cursor() -> Option<(i32, i32)> {
    let mut point = POINT { x: 0, y: 0 };
    let ok = unsafe { GetCursorPos(&mut point) };
    (ok != 0).then_some((point.x, point.y))
}

/// The window that has the keyboard (`GetForegroundWindow`), as a number.
pub fn foreground_hwnd() -> isize {
    unsafe { GetForegroundWindow() as isize }
}

/// Every display's work area, taskbar excluded, in physical pixels.
pub fn work_areas() -> Vec<WorkArea> {
    let mut areas: Vec<WorkArea> = Vec::new();
    unsafe {
        EnumDisplayMonitors(
            0 as HDC,
            ptr::null(),
            Some(collect_monitor),
            &mut areas as *mut Vec<WorkArea> as isize,
        );
    }
    areas
}

unsafe extern "system" fn collect_monitor(monitor: HMONITOR, _hdc: HDC, _rect: *mut RECT, data: isize) -> windows_sys::Win32::Foundation::BOOL {
    let areas = &mut *(data as *mut Vec<WorkArea>);
    let mut info: MONITORINFOEXW = std::mem::zeroed();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    if GetMonitorInfoW(monitor, &mut info.monitorInfo) != 0 {
        let name = String::from_utf16_lossy(&info.szDevice)
            .trim_end_matches('\0')
            .to_owned();
        let work = info.monitorInfo.rcWork;
        areas.push(WorkArea {
            x: work.left,
            y: work.top,
            width: work.right - work.left,
            height: work.bottom - work.top,
            name,
        });
    }
    1
}

/// The work area of the display a window mostly sits on, or the nearest one.
pub fn area_for_window(value: isize) -> Option<WorkArea> {
    let monitor = unsafe { MonitorFromWindow(hwnd(value), MONITOR_DEFAULTTONEAREST) };
    if monitor.is_null() {
        return None;
    }
    let mut info: MONITORINFOEXW = unsafe { std::mem::zeroed() };
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    let ok = unsafe { GetMonitorInfoW(monitor, &mut info.monitorInfo) };
    if ok == 0 {
        return None;
    }
    let work = info.monitorInfo.rcWork;
    Some(WorkArea {
        x: work.left,
        y: work.top,
        width: work.right - work.left,
        height: work.bottom - work.top,
        name: String::from_utf16_lossy(&info.szDevice).trim_end_matches('\0').to_owned(),
    })
}

/// Which display's work area holds a point.
pub fn area_at_point(areas: &[WorkArea], x: i32, y: i32) -> Option<&WorkArea> {
    crate::companion::positions::area_at(areas, x, y)
}

/// Change a floating window's size around its bottom centre, the way the React
/// app grows the island and the dictation bubble upward.
pub fn resize_around_bottom_centre(executor: &gpui::BackgroundExecutor, value: isize, logical: (f32, f32)) {
    let Some(rect) = window_rect(value) else { return };
    let scale = dpi_scale(value);
    let width = (logical.0 * scale).round() as i32;
    let height = (logical.1 * scale).round() as i32;
    let centre = rect.0 + rect.2 / 2;
    let bottom = rect.1 + rect.3;
    let areas = work_areas();
    let clamped = crate::companion::positions::clamp_into(&areas, (centre - width / 2, bottom - height, width, height), (centre, bottom));
    place(executor, value, clamped.0, clamped.1, width, height);
}

/// The system's scale factor, for a window that does not exist yet (the
/// companion's launch position, before its first paint).
pub fn system_scale() -> f32 {
    let dpi = unsafe { windows_sys::Win32::UI::HiDpi::GetDpiForSystem() };
    if dpi == 0 {
        1.0
    } else {
        dpi as f32 / 96.0
    }
}

/// The window's scale factor: 1.0 at 96 DPI, 1.25 at 120, and so on.
pub fn dpi_scale(value: isize) -> f32 {
    let dpi = unsafe { windows_sys::Win32::UI::HiDpi::GetDpiForWindow(hwnd(value)) };
    if dpi == 0 {
        1.0
    } else {
        dpi as f32 / 96.0
    }
}

/// The scale factor for a window, from the sizes GPUI and Windows report.
pub fn scale_for(logical_width: f32, physical_width: i32) -> f32 {
    if logical_width > 0.0 && physical_width > 0 {
        physical_width as f32 / logical_width
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scale_comes_from_the_two_widths_gpui_and_windows_report() {
        assert!((scale_for(344.0, 344) - 1.0).abs() < 1e-6);
        assert!((scale_for(344.0, 430) - 1.25).abs() < 1e-6);
        assert_eq!(scale_for(0.0, 430), 1.0);
        assert_eq!(scale_for(344.0, 0), 1.0);
    }

    #[test]
    fn a_point_finds_the_display_it_is_on() {
        let areas = vec![
            WorkArea { x: 0, y: 0, width: 1920, height: 1040, name: "one".into() },
            WorkArea { x: 1920, y: 0, width: 1280, height: 680, name: "two".into() },
        ];
        assert_eq!(area_at_point(&areas, 100, 100).map(|area| area.name.as_str()), Some("one"));
        assert_eq!(area_at_point(&areas, 2000, 100).map(|area| area.name.as_str()), Some("two"));
        assert!(area_at_point(&areas, 5000, 5000).is_none());
    }
}
