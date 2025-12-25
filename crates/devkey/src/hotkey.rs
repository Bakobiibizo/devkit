//! Windows global hotkey listener for CTRL+;

use crate::AppMessage;
use anyhow::Result;
use std::sync::mpsc::Sender;

#[cfg(windows)]
use windows::{
    Win32::Foundation::*,
    Win32::UI::Input::KeyboardAndMouse::*,
    Win32::UI::WindowsAndMessaging::*,
};

const HOTKEY_ID: i32 = 1;

#[cfg(windows)]
pub fn run_hotkey_listener(tx: Sender<AppMessage>) -> Result<()> {
    unsafe {
        // VK_OEM_1 is the semicolon key (;)
        RegisterHotKey(
            HWND::default(),
            HOTKEY_ID,
            MOD_CONTROL | MOD_NOREPEAT,
            VK_OEM_1.0 as u32,
        )
        .map_err(|_| {
            anyhow::anyhow!(
                "Failed to register hotkey CTRL+; - it may be in use by another application"
            )
        })?;

        // Message loop
        let mut msg = MSG::default();
        loop {
            let ret = GetMessageW(&mut msg, HWND::default(), 0, 0);
            if ret.0 <= 0 {
                break;
            }

            if msg.message == WM_HOTKEY && msg.wParam.0 as i32 == HOTKEY_ID {
                let _ = tx.send(AppMessage::ShowWindow);
            }

            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        let _ = UnregisterHotKey(HWND::default(), HOTKEY_ID);
    }

    Ok(())
}

#[cfg(not(windows))]
pub fn run_hotkey_listener(_tx: Sender<AppMessage>) -> Result<()> {
    Err(anyhow::anyhow!("Global hotkeys only supported on Windows"))
}
