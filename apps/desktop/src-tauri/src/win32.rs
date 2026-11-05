//! Thin Win32 helpers: cursor, monitor work areas / DPI, focus handling.

use std::ffi::c_void;

use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, HMONITOR, MONITORINFO, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY,
};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, GetCursorPos, GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow, SetWindowPos,
    ShowWindow, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_SHOWNOACTIVATE,
};

/// A monitor's work area (taskbar excluded) in physical pixels plus its scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorkArea {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub scale: f64,
}

impl WorkArea {
    pub fn center(&self) -> (i32, i32) {
        ((self.left + self.right) / 2, (self.top + self.bottom) / 2)
    }
}

pub fn cursor_position() -> Option<(i32, i32)> {
    let mut point = POINT::default();
    // SAFETY: out-parameter call.
    unsafe { GetCursorPos(&mut point) }.ok()?;
    Some((point.x, point.y))
}

fn work_area_of(monitor: HMONITOR) -> Option<WorkArea> {
    let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    // SAFETY: cbSize initialised; monitor handle from MonitorFromPoint.
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return None;
    }
    let (mut dpi_x, mut dpi_y) = (96u32, 96u32);
    // SAFETY: out-parameters.
    let _ = unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) };
    let rc = info.rcWork;
    Some(WorkArea {
        left: rc.left,
        top: rc.top,
        right: rc.right,
        bottom: rc.bottom,
        scale: dpi_x.max(48) as f64 / 96.0,
    })
}

const FALLBACK_AREA: WorkArea = WorkArea { left: 0, top: 0, right: 1920, bottom: 1040, scale: 1.0 };

/// Work area of the monitor nearest to a point.
pub fn work_area_at(x: i32, y: i32) -> WorkArea {
    // SAFETY: plain call.
    let monitor = unsafe { MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST) };
    work_area_of(monitor).unwrap_or(FALLBACK_AREA)
}

pub fn primary_work_area() -> WorkArea {
    // SAFETY: plain call.
    let monitor = unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) };
    work_area_of(monitor).unwrap_or(FALLBACK_AREA)
}

/// Top-left for a `size`×`size` square centred on a point, kept inside `area`.
pub fn centered_square(center: (i32, i32), size: i32, area: &WorkArea) -> (i32, i32) {
    let clamp = |v: i32, lo: i32, hi: i32| if hi < lo { lo } else { v.clamp(lo, hi) };
    (clamp(center.0 - size / 2, area.left, area.right - size), clamp(center.1 - size / 2, area.top, area.bottom - size))
}

/// Top-left for a `width`×`height` panel in the bottom-right corner of `area`.
pub fn bottom_right(area: &WorkArea, width: i32, height: i32, margin: i32) -> (i32, i32) {
    ((area.right - width - margin).max(area.left), (area.bottom - height - margin).max(area.top))
}

pub fn hwnd_from_raw(raw: *mut c_void) -> HWND {
    HWND(raw)
}

/// Bring a window to the foreground even when we are a background process.
/// Works best when the caller (Explorer) granted us `AllowSetForegroundWindow`.
pub fn force_foreground(hwnd: HWND) {
    // SAFETY: all calls take a valid window handle; failures are tolerated.
    unsafe {
        if SetForegroundWindow(hwnd).as_bool() {
            return;
        }
        let foreground = GetForegroundWindow();
        let foreground_thread = GetWindowThreadProcessId(foreground, None);
        let current_thread = GetCurrentThreadId();
        let attached = foreground_thread != 0
            && foreground_thread != current_thread
            && AttachThreadInput(current_thread, foreground_thread, true).as_bool();
        let _ = BringWindowToTop(hwnd);
        let _ = SetForegroundWindow(hwnd);
        if attached {
            let _ = AttachThreadInput(current_thread, foreground_thread, false);
        }
    }
}

/// Show a topmost window without stealing focus from the user's current app.
pub fn show_without_activating(hwnd: HWND) {
    // SAFETY: valid handle; errors are non-fatal.
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: WorkArea = WorkArea { left: 0, top: 0, right: 1920, bottom: 1040, scale: 1.0 };

    #[test]
    fn square_is_centred_when_it_fits() {
        assert_eq!(centered_square((960, 520), 420, &AREA), (750, 310));
    }

    #[test]
    fn square_is_clamped_to_the_work_area() {
        assert_eq!(centered_square((5, 5), 420, &AREA), (0, 0));
        assert_eq!(centered_square((1919, 1039), 420, &AREA), (1500, 620));
        let secondary = WorkArea { left: -2560, top: 0, right: 0, bottom: 1400, scale: 1.5 };
        assert_eq!(centered_square((-10, 700), 630, &secondary), (-630, 385));
    }

    #[test]
    fn bottom_right_respects_margin() {
        assert_eq!(bottom_right(&AREA, 380, 200, 16), (1524, 824));
        assert_eq!(bottom_right(&AREA, 5000, 5000, 16), (0, 0));
    }

    #[test]
    fn work_area_queries_do_not_fail() {
        let area = primary_work_area();
        assert!(area.right > area.left && area.bottom > area.top);
        assert!(area.scale >= 0.5);
    }
}
