//! Diagnostic: log every raw mouse message a `WH_MOUSE_LL` hook receives.
//!
//! Run it, then press the buttons in question:
//!
//! ```text
//! cargo run --example raw_hook_log -p openlogi-hook
//! ```
//!
//! It prints the message id for each button / wheel event (mouse-move is
//! filtered out). If a side button shows `WM_XBUTTONDOWN` it is a standard mouse
//! button the hook can see; if pressing it prints nothing, it is delivered by
//! some other path (e.g. `WM_APPCOMMAND` or a HID++ control) that a low-level
//! mouse hook cannot observe.
#![allow(unsafe_code)]

#[cfg(target_os = "windows")]
fn main() {
    use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage,
        MSG, MSLLHOOKSTRUCT, WH_MOUSE_LL, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN,
        WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_XBUTTONDOWN,
        WM_XBUTTONUP,
    };

    unsafe extern "system" fn proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 {
            let msg = wparam as u32;
            let name = match msg {
                WM_LBUTTONDOWN => Some("WM_LBUTTONDOWN"),
                WM_LBUTTONUP => Some("WM_LBUTTONUP"),
                WM_RBUTTONDOWN => Some("WM_RBUTTONDOWN"),
                WM_RBUTTONUP => Some("WM_RBUTTONUP"),
                WM_MBUTTONDOWN => Some("WM_MBUTTONDOWN"),
                WM_MBUTTONUP => Some("WM_MBUTTONUP"),
                WM_XBUTTONDOWN => Some("WM_XBUTTONDOWN"),
                WM_XBUTTONUP => Some("WM_XBUTTONUP"),
                WM_MOUSEWHEEL => Some("WM_MOUSEWHEEL"),
                WM_MOUSEHWHEEL => Some("WM_MOUSEHWHEEL"),
                _ => None, // WM_MOUSEMOVE etc. — filtered out
            };
            if let Some(name) = name {
                // SAFETY: for HC_ACTION, lparam points to a valid MSLLHOOKSTRUCT.
                let info = unsafe { &*(lparam as *const MSLLHOOKSTRUCT) };
                let hi = (info.mouseData >> 16) as u16;
                println!("{name:<16} msg=0x{msg:04X} mouseData_hi=0x{hi:04X} (XBUTTON1=1 back, XBUTTON2=2 fwd)");
            }
        }
        unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
    }

    println!("Raw WH_MOUSE_LL logger. Press your mouse buttons (incl. the side buttons).");
    println!("Mouse-move is filtered. Ctrl+C to quit.\n");

    // SAFETY: straight-line Win32 hook install + message pump.
    unsafe {
        let hmod = GetModuleHandleW(std::ptr::null());
        let hook = SetWindowsHookExW(WH_MOUSE_LL, Some(proc), hmod, 0);
        if hook.is_null() {
            eprintln!("SetWindowsHookExW failed");
            return;
        }
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("This diagnostic is Windows-only.");
}
