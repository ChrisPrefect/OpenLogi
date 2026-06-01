//! Windows `WH_MOUSE_LL` implementation of the OS-level mouse hook.
//!
//! Mirrors the macOS `CGEventTap` design ([`super::macos`]): a dedicated
//! background thread installs a low-level mouse hook and pumps a message loop;
//! [`stop`] posts `WM_QUIT` to that thread to tear it down.
//!
//! A low-level mouse hook procedure is a bare `extern "system"` function — it
//! can't capture the user's Rust closure directly. Because Windows invokes the
//! procedure on the *same* thread that installed the hook (during its
//! `GetMessage` pump), we stash the closure in a `thread_local` on the hook
//! thread and read it back from the procedure. This keeps the whole thing on
//! one thread with no cross-thread sharing of the callback.

use std::cell::RefCell;
use std::sync::mpsc;
use std::thread;

use tracing::{error, warn};
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::{
    GetCurrentThreadId, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId,
    SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, HHOOK, LLMHF_INJECTED, MSG,
    MSLLHOOKSTRUCT, PostThreadMessageW, WH_MOUSE_LL, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN,
    WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEWHEEL, WM_QUIT, WM_RBUTTONDOWN, WM_RBUTTONUP,
    WM_XBUTTONDOWN, WM_XBUTTONUP, XBUTTON1, XBUTTON2,
};

use crate::{ButtonId, EventDisposition, HookError, MouseEvent};

// Per-thread slot holding the user's callback. Set by `thread_main` before the
// hook is installed and cleared when the message loop exits; read by
// `ll_mouse_proc` on the same thread.
thread_local! {
    static CALLBACK: RefCell<Option<Box<dyn Fn(MouseEvent) -> EventDisposition>>> =
        const { RefCell::new(None) };
}

/// Everything [`crate::Hook`] needs to control the background thread.
pub(crate) struct HookInner {
    thread: thread::JoinHandle<()>,
    /// OS thread id of the hook thread, so [`stop`] can post `WM_QUIT` to it.
    thread_id: u32,
}

/// Read the foreground window's executable file name (e.g. `"chrome.exe"`),
/// lower-cased for stable per-application profile matching. The Windows analogue
/// of the macOS bundle identifier. `None` when there is no foreground window or
/// the process can't be queried.
pub(crate) fn frontmost_process_name() -> Option<String> {
    // SAFETY: each Win32 call below is invoked with valid arguments; the process
    // handle from `OpenProcess` is closed before returning, and the UTF-16 path
    // buffer is sized via the `size` out-param that `QueryFullProcessImageNameW`
    // updates.
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return None;
        }
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == 0 {
            return None;
        }
        let handle: HANDLE = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut buf = [0u16; 260];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut size);
        CloseHandle(handle);
        if ok == 0 || size == 0 {
            return None;
        }
        let path = String::from_utf16_lossy(&buf[..size as usize]);
        let name = path.rsplit(['\\', '/']).next().unwrap_or(path.as_str());
        Some(name.to_ascii_lowercase())
    }
}

/// Translate a Windows mouse message + hook payload to our [`MouseEvent`]
/// vocabulary. Returns `None` for messages we don't map.
fn translate(msg: u32, info: &MSLLHOOKSTRUCT) -> Option<MouseEvent> {
    /// One wheel notch in `mouseData` units (`WHEEL_DELTA`).
    const WHEEL_DELTA: f32 = 120.0;

    let button = |id: ButtonId, pressed: bool| Some(MouseEvent::Button { id, pressed });
    match msg {
        WM_LBUTTONDOWN => button(ButtonId::LeftClick, true),
        WM_LBUTTONUP => button(ButtonId::LeftClick, false),
        WM_RBUTTONDOWN => button(ButtonId::RightClick, true),
        WM_RBUTTONUP => button(ButtonId::RightClick, false),
        WM_MBUTTONDOWN => button(ButtonId::MiddleClick, true),
        WM_MBUTTONUP => button(ButtonId::MiddleClick, false),
        WM_XBUTTONDOWN | WM_XBUTTONUP => {
            // The pressed X button is in the high word of `mouseData`.
            let pressed = msg == WM_XBUTTONDOWN;
            #[allow(clippy::cast_possible_truncation, reason = "high word is 16-bit")]
            let xbtn = (info.mouseData >> 16) as u16;
            let id = match xbtn {
                XBUTTON1 => ButtonId::Back,
                XBUTTON2 => ButtonId::Forward,
                _ => return None,
            };
            button(id, pressed)
        }
        WM_MOUSEWHEEL => {
            // High word of `mouseData` is a signed notch count. Windows: positive
            // = away from user (up); our `delta_y` convention: positive = down.
            #[allow(clippy::cast_possible_truncation, reason = "high word is i16")]
            let raw = (info.mouseData >> 16) as i16;
            Some(MouseEvent::Scroll {
                delta_x: 0.0,
                delta_y: -(f32::from(raw) / WHEEL_DELTA),
            })
        }
        WM_MOUSEHWHEEL => {
            // Windows horizontal wheel: positive = right; our `delta_x`: positive
            // = right. Same sign.
            #[allow(clippy::cast_possible_truncation, reason = "high word is i16")]
            let raw = (info.mouseData >> 16) as i16;
            Some(MouseEvent::Scroll {
                delta_x: f32::from(raw) / WHEEL_DELTA,
                delta_y: 0.0,
            })
        }
        _ => None,
    }
}

/// The low-level mouse hook procedure. Invoked by Windows on the hook thread for
/// every mouse event system-wide. Reads the callback from the thread-local slot,
/// and returns a non-zero value to suppress an event the callback wants dropped.
unsafe extern "system" fn ll_mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // `code < 0` (i.e. not `HC_ACTION`) means we must pass the event on untouched.
    if code < 0 {
        return unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) };
    }

    // SAFETY: for a `WH_MOUSE_LL` hook with `code == HC_ACTION`, `lparam` points
    // to a valid `MSLLHOOKSTRUCT` for the duration of this call.
    let info = unsafe { &*(lparam as *const MSLLHOOKSTRUCT) };

    // Skip events we synthesised ourselves (e.g. a remapped button that posts a
    // click via `SendInput`) so remapping can't feed back into itself.
    if info.flags & LLMHF_INJECTED != 0 {
        return unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) };
    }

    #[allow(clippy::cast_possible_truncation, reason = "mouse message ids fit in u32")]
    if let Some(event) = translate(wparam as u32, info) {
        let disposition = CALLBACK.with(|slot| {
            slot.borrow()
                .as_ref()
                .map_or(EventDisposition::PassThrough, |cb| cb(event))
        });
        if disposition == EventDisposition::Suppress {
            return 1;
        }
    }

    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

/// Install the hook on a dedicated thread and start delivering events to `cb`.
pub(crate) fn start(
    cb: impl Fn(MouseEvent) -> EventDisposition + Send + Sync + 'static,
) -> Result<HookInner, HookError> {
    let cb: Box<dyn Fn(MouseEvent) -> EventDisposition + Send + Sync> = Box::new(cb);
    let (tx, rx) = mpsc::channel::<Result<u32, HookError>>();

    let thread = thread::Builder::new()
        .name("openlogi-hook".into())
        .spawn(move || thread_main(cb, &tx))
        .map_err(|e| HookError::WindowsHook(format!("failed to spawn hook thread: {e}")))?;

    // Block until the hook thread reports the hook is live (with its thread id)
    // or reports the failure that prevented it.
    match rx.recv() {
        Ok(Ok(thread_id)) => Ok(HookInner { thread, thread_id }),
        Ok(Err(e)) => {
            let _ = thread.join();
            Err(e)
        }
        Err(_) => {
            let _ = thread.join();
            Err(HookError::WindowsHook(
                "hook thread exited before reporting status".into(),
            ))
        }
    }
}

/// Body of the background hook thread: install the hook, signal readiness, pump
/// messages until `WM_QUIT`, then unhook.
fn thread_main(
    cb: Box<dyn Fn(MouseEvent) -> EventDisposition + Send + Sync>,
    tx: &mpsc::Sender<Result<u32, HookError>>,
) {
    // Coerce away the `Send + Sync` bounds into the thread-local's plain
    // `dyn Fn` type — the closure never leaves this thread now.
    CALLBACK.with(|slot| *slot.borrow_mut() = Some(cb));

    // SAFETY: `GetModuleHandleW(null)` returns this process's module handle, the
    // documented value to pass for an in-process low-level hook procedure.
    let hmod = unsafe { GetModuleHandleW(std::ptr::null()) };
    // SAFETY: `ll_mouse_proc` is a valid hook procedure; `WH_MOUSE_LL` with
    // thread id 0 installs a system-wide low-level mouse hook.
    let hook: HHOOK = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(ll_mouse_proc), hmod, 0) };
    if hook.is_null() {
        // SAFETY: `GetLastError` is always safe to call.
        let err = unsafe { GetLastError() };
        let _ = tx.send(Err(HookError::WindowsHook(format!(
            "SetWindowsHookExW(WH_MOUSE_LL) failed (GetLastError={err})"
        ))));
        CALLBACK.with(|slot| *slot.borrow_mut() = None);
        return;
    }

    // SAFETY: always safe.
    let thread_id = unsafe { GetCurrentThreadId() };
    if tx.send(Ok(thread_id)).is_err() {
        // Parent dropped before receiving — tear down and exit.
        // SAFETY: `hook` is the live handle just returned by SetWindowsHookExW.
        unsafe { UnhookWindowsHookEx(hook) };
        CALLBACK.with(|slot| *slot.borrow_mut() = None);
        return;
    }

    // Message pump. Low-level hooks require the installing thread to service a
    // message queue; `stop` breaks this loop by posting `WM_QUIT`.
    // SAFETY: `msg` is a stack `MSG` we hand to `GetMessageW` by pointer.
    let mut msg: MSG = unsafe { std::mem::zeroed() };
    loop {
        // SAFETY: `&mut msg` is a valid writable `MSG`; null hwnd pumps all
        // messages for this thread.
        let ret = unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) };
        // 0 == WM_QUIT, -1 == error: either way, stop pumping.
        if ret <= 0 {
            break;
        }
        // SAFETY: `msg` was populated by `GetMessageW` above.
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    // SAFETY: `hook` is still the live handle; detaching it restores normal
    // input delivery immediately.
    unsafe { UnhookWindowsHookEx(hook) };
    CALLBACK.with(|slot| *slot.borrow_mut() = None);
}

/// Signal the hook thread to stop and join it.
pub(crate) fn stop(inner: HookInner) {
    // SAFETY: posting `WM_QUIT` to a thread id is always safe; if the thread has
    // already exited the post simply fails and the join returns immediately.
    unsafe {
        PostThreadMessageW(inner.thread_id, WM_QUIT, 0, 0);
    }
    if let Err(e) = inner.thread.join() {
        error!("hook thread panicked on shutdown: {e:?}");
    }
}

/// Mirrors the macOS no-op for symmetry; Windows surfaces no permission prompt.
pub(crate) fn prompt_accessibility() {
    warn!("prompt_accessibility is a no-op on Windows (no low-level-hook permission gate)");
}
