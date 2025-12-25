//! Text injection via clipboard and simulated paste

use anyhow::Result;

#[cfg(windows)]
use windows::{
    Win32::Foundation::*,
    Win32::System::DataExchange::*,
    Win32::System::Memory::*,
    Win32::UI::Input::KeyboardAndMouse::*,
};

// CF_UNICODETEXT = 13
#[cfg(windows)]
const CF_UNICODETEXT: u32 = 13;

/// Copy text to clipboard only (no paste simulation)
/// Used for commands where we want the user to have the value
/// but don't want to inject it into the current context
#[cfg(windows)]
pub fn copy_to_clipboard(text: &str) -> Result<()> {
    unsafe { set_clipboard_text(text) }
}

/// Inject text at the current cursor position by:
/// 1. Setting clipboard to our text (user keeps this as fallback)
/// 2. Restoring focus to the original window
/// 3. Simulating Ctrl+V
///
/// Note: We intentionally don't restore the original clipboard anymore.
/// This way if the paste fails (e.g., wrong focus), the user still has
/// the value in their clipboard and can manually paste.
#[cfg(windows)]
pub fn inject_text(text: &str) -> Result<()> {
    unsafe {
        // Set clipboard to our text (user keeps this as fallback)
        set_clipboard_text(text)?;

        // Restore focus to the original window before pasting
        crate::focus::restore_foreground_window();

        // Small delay to ensure focus switch and clipboard is ready
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Simulate Ctrl+V
        send_paste()?;

        Ok(())
    }
}

#[cfg(windows)]
unsafe fn set_clipboard_text(text: &str) -> Result<()> {
    unsafe {
        // Convert to UTF-16
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let size = wide.len() * 2;

        // Allocate global memory
        let hmem = GlobalAlloc(GMEM_MOVEABLE, size)?;
        let ptr = GlobalLock(hmem) as *mut u16;
        if ptr.is_null() {
            let _ = GlobalFree(hmem);
            return Err(anyhow::anyhow!("Failed to lock global memory"));
        }

        std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
        let _ = GlobalUnlock(hmem);

        // Open and set clipboard
        if OpenClipboard(HWND::default()).is_err() {
            let _ = GlobalFree(hmem);
            return Err(anyhow::anyhow!("Failed to open clipboard"));
        }

        let _ = EmptyClipboard();

        let result = SetClipboardData(CF_UNICODETEXT, HANDLE(hmem.0));
        let _ = CloseClipboard();

        if result.is_err() {
            let _ = GlobalFree(hmem);
            return Err(anyhow::anyhow!("Failed to set clipboard data"));
        }

        Ok(())
    }
}

#[cfg(windows)]
unsafe fn send_paste() -> Result<()> {
    unsafe {
        // Create input events for Ctrl+V
        let mut inputs: [INPUT; 4] = std::mem::zeroed();

        // Ctrl down
        inputs[0].r#type = INPUT_KEYBOARD;
        inputs[0].Anonymous.ki.wVk = VK_CONTROL;

        // V down
        inputs[1].r#type = INPUT_KEYBOARD;
        inputs[1].Anonymous.ki.wVk = VK_V;

        // V up
        inputs[2].r#type = INPUT_KEYBOARD;
        inputs[2].Anonymous.ki.wVk = VK_V;
        inputs[2].Anonymous.ki.dwFlags = KEYEVENTF_KEYUP;

        // Ctrl up
        inputs[3].r#type = INPUT_KEYBOARD;
        inputs[3].Anonymous.ki.wVk = VK_CONTROL;
        inputs[3].Anonymous.ki.dwFlags = KEYEVENTF_KEYUP;

        let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        if sent != 4 {
            return Err(anyhow::anyhow!("Failed to send input events"));
        }

        Ok(())
    }
}

#[cfg(not(windows))]
pub fn copy_to_clipboard(_text: &str) -> Result<()> {
    Err(anyhow::anyhow!("Clipboard only supported on Windows"))
}

#[cfg(not(windows))]
pub fn inject_text(_text: &str) -> Result<()> {
    Err(anyhow::anyhow!("Text injection only supported on Windows"))
}
