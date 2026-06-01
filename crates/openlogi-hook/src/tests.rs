//! Tests for the platform-agnostic hook API.

use super::*;

/// All `HookError` variants produce non-empty display messages.
#[test]
fn hook_error_display() {
    let errors: &[HookError] = &[
        HookError::Unsupported,
        HookError::AccessibilityDenied,
        HookError::MacOsTap("test reason".into()),
        HookError::WindowsHook("test reason".into()),
    ];
    for e in errors {
        assert!(!e.to_string().is_empty(), "empty display for {e:?}");
    }
}

/// `MouseEvent` is `Clone + Debug` — both variants exercise without panic.
#[test]
fn mouse_event_clone_and_debug() {
    let events = [
        MouseEvent::Button {
            id: ButtonId::Back,
            pressed: true,
        },
        MouseEvent::Scroll {
            delta_x: 1.0,
            delta_y: -1.5,
        },
    ];
    for e in &events {
        let cloned = e.clone();
        let _ = format!("{e:?}");
        let _ = format!("{cloned:?}");
    }
}

/// `EventDisposition` implements `PartialEq` correctly.
#[test]
fn event_disposition_equality() {
    assert_eq!(EventDisposition::PassThrough, EventDisposition::PassThrough);
    assert_eq!(EventDisposition::Suppress, EventDisposition::Suppress);
    assert_ne!(EventDisposition::PassThrough, EventDisposition::Suppress);
}

/// On platforms with no hook implementation (Linux), `Hook::start` returns
/// `Unsupported`. macOS and Windows have real implementations and are excluded.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
#[test]
fn unsupported_platform_start_returns_unsupported() {
    let result = Hook::start(|_| EventDisposition::PassThrough);
    assert!(matches!(result, Err(HookError::Unsupported)));
}

/// On non-macOS targets, `Hook::has_accessibility` is always `true`.
#[cfg(not(target_os = "macos"))]
#[test]
fn non_macos_has_accessibility_is_true() {
    assert!(Hook::has_accessibility());
}

/// On Windows, a hook can be installed and torn down cleanly. This exercises
/// the full `SetWindowsHookEx` → message-pump → `WM_QUIT` lifecycle.
#[cfg(target_os = "windows")]
#[test]
fn windows_start_and_stop_roundtrip() {
    let hook = Hook::start(|_| EventDisposition::PassThrough)
        .expect("install WH_MOUSE_LL hook on Windows");
    hook.stop();
}
