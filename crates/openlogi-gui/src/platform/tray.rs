//! System-tray / status-item presence. macOS-only today, via `NSStatusItem`
//! (which lives in the menu bar) over raw Cocoa FFI — GPUI exposes no
//! status-bar API.
//!
//! `tray` is the cross-platform-neutral name: macOS has the menu-bar status
//! item, Windows the system tray / notification area, Linux the
//! StatusNotifierItem spec. Only macOS is implemented, so the module carries no
//! stub — every caller gates on `cfg(target_os = "macos")` instead.
//!
//! Menu clicks can't reach GPUI's `App`, so they post a [`TrayEvent`] on a
//! channel that a dedicated task in `main.rs` drains.

#[cfg(target_os = "macos")]
pub use macos::{
    TrayEvent, hide_from_dock, install, refresh_labels, request_refresh, set_device_status,
    set_visible, show_in_dock,
};

#[cfg(target_os = "windows")]
pub use windows::{
    TrayEvent, install, refresh_labels, request_refresh, set_device_status, set_visible,
};

#[cfg(target_os = "macos")]
mod macos {
    use std::sync::OnceLock;

    use cocoa::base::id;
    use objc::runtime::{Object, Sel};
    use objc::{sel, sel_impl};
    use tokio::sync::mpsc;
    use tracing::warn;

    use super::super::status_item::{
        self, ActionCallback, ActionTarget, ActivationPolicy, Menu, MenuItem, StatusItem,
    };

    /// A request raised by clicking a status-bar menu item, or by a live
    /// language switch asking the drain task to re-localize the whole menu.
    #[derive(Debug, Clone, Copy)]
    pub enum TrayEvent {
        Open,
        Quit,
        /// Re-title Open/Quit *and* the device line for the current locale.
        Refresh,
    }

    const TARGET_CLASS: &str = "OpenLogiMenuTarget";

    // Read by the Objective-C action callbacks, which can't capture state.
    static MENU_TX: OnceLock<mpsc::UnboundedSender<TrayEvent>> = OnceLock::new();

    /// Open/Quit item pointers, kept so a live locale switch can re-title them.
    /// Stored as opaque menu-item handles; only touched on the main thread.
    static MENU_REFS: OnceLock<MenuRefs> = OnceLock::new();

    /// The device-status line item, written by [`set_device_status`]. Only ever
    /// touched on the main thread.
    static DEVICE_ITEM: OnceLock<MenuItem> = OnceLock::new();

    /// The `NSStatusItem` itself, so [`set_visible`] can show / hide the icon.
    static STATUS_ITEM: OnceLock<StatusItem> = OnceLock::new();

    struct MenuRefs {
        open: MenuItem,
        quit: MenuItem,
    }

    struct InstalledMenu {
        menu: Menu,
        refs: MenuRefs,
        device_item: MenuItem,
    }

    /// Install the status item. Main thread only.
    ///
    /// The activation policy (Dock + menu-bar visibility) is *not* set here —
    /// [`show_in_dock`] / [`hide_from_dock`] manage it as windows open and
    /// close. The status item, its menu, and the click target are all retained
    /// for the app's lifetime (a status item lives as long as the process); the
    /// target in particular *must* be retained, since `NSMenuItem` keeps only a
    /// weak reference to it.
    pub fn install(tx: mpsc::UnboundedSender<TrayEvent>) {
        let _ = MENU_TX.set(tx);

        let status_item = StatusItem::new();
        let _ = STATUS_ITEM.set(status_item);
        status_item.set_symbol_icon("computermouse.fill", "OpenLogi", "OpenLogi");

        let installed_menu = build_menu();
        let _ = DEVICE_ITEM.set(installed_menu.device_item);
        let _ = MENU_REFS.set(installed_menu.refs);
        status_item.set_menu(installed_menu.menu);
    }

    fn build_menu() -> InstalledMenu {
        let target = action_target();
        let menu = Menu::new();

        let idle = rust_i18n::t!("No device connected");
        let device_item = MenuItem::disabled(&idle);
        menu.add_item(device_item);

        menu.add_separator();

        let open_selector = sel!(openOpenLogi:);
        let quit_selector = sel!(quitOpenLogi:);
        let open_title = rust_i18n::t!("Open OpenLogi");
        let open_item = MenuItem::action(&open_title, open_selector, &target);
        menu.add_item(open_item);
        let quit_title = rust_i18n::t!("Quit OpenLogi");
        let quit_item = MenuItem::action(&quit_title, quit_selector, &target);
        menu.add_item(quit_item);

        InstalledMenu {
            menu,
            refs: MenuRefs {
                open: open_item,
                quit: quit_item,
            },
            device_item,
        }
    }

    fn action_target() -> ActionTarget {
        let open_selector = sel!(openOpenLogi:);
        let quit_selector = sel!(quitOpenLogi:);
        let target_methods = [
            (open_selector, open_action as ActionCallback),
            (quit_selector, quit_action as ActionCallback),
        ];
        ActionTarget::new(TARGET_CLASS, &target_methods)
    }

    /// Show the app in the Dock + menu bar — called when a window opens, so the
    /// app menu (⌘Q, Settings, …) is available while the window is up.
    pub fn show_in_dock() {
        status_item::set_activation_policy(ActivationPolicy::Regular);
    }

    /// Drop the app out of the Dock + menu bar, leaving only the status item —
    /// called when the last window closes (and on a `--minimized` launch).
    pub fn hide_from_dock() {
        status_item::set_activation_policy(ActivationPolicy::Accessory);
    }

    /// Show or hide the status-item icon without tearing it down — backs the
    /// "Show in menu bar" setting. A no-op until [`install`] has run.
    pub fn set_visible(visible: bool) {
        let Some(item) = STATUS_ITEM.get() else {
            return;
        };
        item.set_visible(visible);
    }

    /// Update the device line, e.g. `"MX Master 3S · 80%"`. Main thread only.
    /// A no-op until [`install`] has published the item.
    pub fn set_device_status(text: &str) {
        let Some(item) = DEVICE_ITEM.get() else {
            return;
        };
        item.set_title(text);
    }

    /// Re-title the Open/Quit items for the current locale. Main-thread only,
    /// like every status-item write. The device line is refreshed separately via
    /// [`set_device_status`].
    pub fn refresh_labels() {
        let Some(refs) = MENU_REFS.get() else {
            return;
        };
        let open_title = rust_i18n::t!("Open OpenLogi");
        let quit_title = rust_i18n::t!("Quit OpenLogi");
        refs.open.set_title(&open_title);
        refs.quit.set_title(&quit_title);
    }

    /// Ask the drain task to re-localize the whole menu after a live language
    /// switch. Posts through the same channel as menu clicks so the device line
    /// (recomputed from the live `AppState`, which only the task can read) is
    /// rewritten on the main thread alongside the static labels.
    pub fn request_refresh() {
        post(TrayEvent::Refresh);
    }

    extern "C" fn open_action(_this: &Object, _cmd: Sel, _sender: id) {
        post(TrayEvent::Open);
    }

    extern "C" fn quit_action(_this: &Object, _cmd: Sel, _sender: id) {
        post(TrayEvent::Quit);
    }

    fn post(event: TrayEvent) {
        if let Some(tx) = MENU_TX.get()
            && tx.send(event).is_err()
        {
            warn!(?event, "menu-bar event dropped — GPUI loop gone");
        }
    }
}

#[cfg(target_os = "windows")]
#[allow(
    unsafe_code,
    reason = "the notification-area icon is only reachable via raw Shell/Win32 FFI"
)]
mod windows {
    //! Windows notification-area (system tray) icon, via `Shell_NotifyIcon`.
    //!
    //! A hidden message-only window on a dedicated thread owns the icon and
    //! services its menu; clicks post a [`TrayEvent`] on the same channel the
    //! macOS status item uses, drained by `main.rs`. The macOS Dock entry points
    //! (`show_in_dock` / `hide_from_dock`) have no Windows analogue and are
    //! simply not part of this module — their call sites are macOS-gated.

    use std::sync::OnceLock;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

    use tokio::sync::mpsc;
    use tracing::warn;
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::Shell::{
        NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW, Shell_NotifyIconW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
        DispatchMessageW, GetCursorPos, GetMessageW, IDI_APPLICATION, LoadIconW, MF_GRAYED,
        MF_SEPARATOR, MF_STRING, MSG, PostMessageW, PostQuitMessage, RegisterClassW,
        SetForegroundWindow, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu,
        TranslateMessage, WM_APP, WM_DESTROY, WM_LBUTTONUP, WM_RBUTTONUP, WNDCLASSW,
    };

    /// A request raised by clicking the tray icon's menu, or by a live language
    /// switch asking the drain task to re-localize the device line.
    #[derive(Debug, Clone, Copy)]
    pub enum TrayEvent {
        Open,
        Quit,
        /// Recompute the device-status line for the current locale.
        Refresh,
    }

    /// `HWND_MESSAGE` — parent for a message-only window (no taskbar presence).
    const HWND_MESSAGE: HWND = -3isize as HWND;
    /// Our tray icon's id within the owning window.
    const TRAY_UID: u32 = 1;
    /// Icon callback message (mouse events on the tray icon land here).
    const WM_TRAYICON: u32 = WM_APP + 1;
    /// Ask the tray window to add / remove the icon (wParam != 0 → add).
    const WM_TRAY_SETVISIBLE: u32 = WM_APP + 2;
    /// Ask the tray window to destroy itself.
    const WM_TRAY_QUIT: u32 = WM_APP + 3;
    /// Menu command ids.
    const ID_OPEN: usize = 1;
    const ID_QUIT: usize = 2;

    static TRAY_TX: OnceLock<mpsc::UnboundedSender<TrayEvent>> = OnceLock::new();
    /// The tray window handle (as `isize`), or 0 before it's created.
    static TRAY_HWND: AtomicIsize = AtomicIsize::new(0);
    /// Whether the icon is currently shown, so visibility toggles are idempotent.
    static VISIBLE: AtomicBool = AtomicBool::new(false);
    /// The device-status line, read live when the menu is built.
    static DEVICE_STATUS: Mutex<String> = Mutex::new(String::new());

    /// UTF-16, null-terminated — the form the wide Win32 APIs expect.
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Install the tray icon on a dedicated thread. Call once at startup.
    pub fn install(tx: mpsc::UnboundedSender<TrayEvent>) {
        let _ = TRAY_TX.set(tx);
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<isize>();
        if std::thread::Builder::new()
            .name("openlogi-tray".into())
            .spawn(move || tray_thread(&ready_tx))
            .is_err()
        {
            warn!("could not spawn tray thread");
            return;
        }
        // Block until the window exists (or the thread reports failure with 0).
        match ready_rx.recv() {
            Ok(0) | Err(_) => warn!("tray window could not be created"),
            Ok(_) => {}
        }
    }

    /// Show or hide the icon without tearing the window down — backs the
    /// "Show in notification area" setting. No-op before [`install`].
    pub fn set_visible(visible: bool) {
        let hwnd = TRAY_HWND.load(Ordering::Relaxed);
        if hwnd != 0 {
            // SAFETY: `hwnd` is a live window we created; posting a message to it
            // is safe and merely queues the visibility toggle on its thread.
            unsafe {
                PostMessageW(hwnd as HWND, WM_TRAY_SETVISIBLE, usize::from(visible), 0);
            }
        }
    }

    /// Update the device line shown atop the menu, e.g. `"MX Master 3S · 80%"`.
    pub fn set_device_status(text: &str) {
        if let Ok(mut guard) = DEVICE_STATUS.lock() {
            text.clone_into(&mut guard);
        }
    }

    /// No-op on Windows: the menu is rebuilt with current-locale labels every
    /// time it opens, so there are no persistent labels to re-title.
    pub fn refresh_labels() {}

    /// Ask the drain task to recompute the device line after a locale switch.
    pub fn request_refresh() {
        post(TrayEvent::Refresh);
    }

    fn post(event: TrayEvent) {
        if let Some(tx) = TRAY_TX.get()
            && tx.send(event).is_err()
        {
            warn!(?event, "tray event dropped — GPUI loop gone");
        }
    }

    /// Body of the tray thread: create the hidden window, add the icon, pump
    /// messages until `WM_QUIT`.
    fn tray_thread(ready_tx: &std::sync::mpsc::Sender<isize>) {
        // SAFETY: a straight-line sequence of Win32 calls with valid arguments;
        // the window and icon live until the message loop ends.
        unsafe {
            let hinstance = GetModuleHandleW(std::ptr::null());
            let class_name = wide("OpenLogiTrayWindow");
            let mut wc: WNDCLASSW = std::mem::zeroed();
            wc.lpfnWndProc = Some(wndproc);
            wc.hInstance = hinstance;
            wc.lpszClassName = class_name.as_ptr();
            RegisterClassW(&wc);

            let window_name = wide("OpenLogi");
            let hwnd = CreateWindowExW(
                0,
                class_name.as_ptr(),
                window_name.as_ptr(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                std::ptr::null_mut(),
                hinstance,
                std::ptr::null(),
            );
            if hwnd.is_null() {
                let _ = ready_tx.send(0);
                return;
            }

            TRAY_HWND.store(hwnd as isize, Ordering::Relaxed);
            add_icon(hwnd);
            VISIBLE.store(true, Ordering::Relaxed);
            let _ = ready_tx.send(hwnd as isize);

            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }

    /// Build the base `NOTIFYICONDATAW` shared by add / delete.
    unsafe fn base_nid(hwnd: HWND) -> NOTIFYICONDATAW {
        let mut nid: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = TRAY_UID;
        nid
    }

    unsafe fn add_icon(hwnd: HWND) {
        let mut nid = unsafe { base_nid(hwnd) };
        nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        nid.uCallbackMessage = WM_TRAYICON;
        // Prefer our embedded app icon (resource id 1, see build.rs); fall back
        // to the stock application icon if it isn't present.
        // SAFETY: `MAKEINTRESOURCEW(1)` is the documented way to name a resource
        // by id; a null icon from a missing resource is handled by the fallback.
        nid.hIcon = unsafe {
            let module = GetModuleHandleW(std::ptr::null());
            let from_resource = LoadIconW(module, 1 as *const u16);
            if from_resource.is_null() {
                LoadIconW(std::ptr::null_mut(), IDI_APPLICATION)
            } else {
                from_resource
            }
        };
        let tip = wide("OpenLogi");
        let n = tip.len().min(nid.szTip.len());
        nid.szTip[..n].copy_from_slice(&tip[..n]);
        // SAFETY: `nid` is a fully-initialised NOTIFYICONDATAW for our window.
        unsafe { Shell_NotifyIconW(NIM_ADD, &nid) };
    }

    unsafe fn delete_icon(hwnd: HWND) {
        let nid = unsafe { base_nid(hwnd) };
        // SAFETY: matches the icon added with the same hWnd + uID.
        unsafe { Shell_NotifyIconW(NIM_DELETE, &nid) };
    }

    /// Pop up the right-click menu at the cursor and act on the chosen item.
    unsafe fn show_context_menu(hwnd: HWND) {
        let device = DEVICE_STATUS
            .lock()
            .ok()
            .map(|g| g.clone())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| rust_i18n::t!("No device connected").into_owned());
        let device_w = wide(&device);
        let open_w = wide(&rust_i18n::t!("Open OpenLogi"));
        let quit_w = wide(&rust_i18n::t!("Quit OpenLogi"));

        // SAFETY: standard menu construction; every pointer is a live wide buffer
        // that outlives the TrackPopupMenu call below.
        unsafe {
            let menu = CreatePopupMenu();
            AppendMenuW(menu, MF_STRING | MF_GRAYED, 0, device_w.as_ptr());
            AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
            AppendMenuW(menu, MF_STRING, ID_OPEN, open_w.as_ptr());
            AppendMenuW(menu, MF_STRING, ID_QUIT, quit_w.as_ptr());

            let mut pt = POINT { x: 0, y: 0 };
            GetCursorPos(&mut pt);
            // Required so the menu dismisses when the user clicks elsewhere.
            SetForegroundWindow(hwnd);
            let cmd = TrackPopupMenu(
                menu,
                TPM_RIGHTBUTTON | TPM_RETURNCMD | TPM_NONOTIFY,
                pt.x,
                pt.y,
                0,
                hwnd,
                std::ptr::null(),
            );
            DestroyMenu(menu);

            match cmd as usize {
                ID_OPEN => post(TrayEvent::Open),
                ID_QUIT => post(TrayEvent::Quit),
                _ => {}
            }
        }
    }

    /// Window procedure for the hidden tray window.
    unsafe extern "system" fn wndproc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_TRAYICON => {
                // The low word of lParam carries the mouse message.
                let mouse = (lparam as u32) & 0xFFFF;
                if mouse == WM_LBUTTONUP {
                    post(TrayEvent::Open);
                } else if mouse == WM_RBUTTONUP {
                    unsafe { show_context_menu(hwnd) };
                }
                0
            }
            WM_TRAY_SETVISIBLE => {
                let want = wparam != 0;
                let current = VISIBLE.load(Ordering::Relaxed);
                if want && !current {
                    unsafe { add_icon(hwnd) };
                    VISIBLE.store(true, Ordering::Relaxed);
                } else if !want && current {
                    unsafe { delete_icon(hwnd) };
                    VISIBLE.store(false, Ordering::Relaxed);
                }
                0
            }
            WM_TRAY_QUIT => {
                unsafe { DestroyWindow(hwnd) };
                0
            }
            WM_DESTROY => {
                unsafe { delete_icon(hwnd) };
                unsafe { PostQuitMessage(0) };
                0
            }
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }
}
