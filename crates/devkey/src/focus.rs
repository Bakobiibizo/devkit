//! Window focus management for saving/restoring the active window

#[cfg(windows)]
use windows::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{GetForegroundWindow, SetForegroundWindow},
};

use std::sync::atomic::{AtomicIsize, Ordering};

/// Global storage for the previous foreground window handle
static PREVIOUS_HWND: AtomicIsize = AtomicIsize::new(0);

/// Save the currently focused window so we can restore it later
#[cfg(windows)]
pub fn save_foreground_window() {
    unsafe {
        let hwnd = GetForegroundWindow();
        PREVIOUS_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
    }
}

/// Restore focus to the previously saved window
#[cfg(windows)]
pub fn restore_foreground_window() {
    let hwnd_val = PREVIOUS_HWND.load(Ordering::SeqCst);
    if hwnd_val != 0 {
        unsafe {
            let hwnd = HWND(hwnd_val as *mut _);
            // SetForegroundWindow may fail if the window is minimized or the calling
            // process doesn't have focus, but we try our best
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

#[cfg(not(windows))]
pub fn save_foreground_window() {}

#[cfg(not(windows))]
pub fn restore_foreground_window() {}
