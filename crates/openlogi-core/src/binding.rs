//! Logical mouse button identifiers and the action vocabulary each one can
//! bind to. Lives in `openlogi-core` because the [`config`](crate::config)
//! schema serializes these directly — the GUI re-exports them.
//!
//! When [`Action`] gains new variants, keep the existing variant names stable:
//! the TOML config keys/values use the enum variant identifiers verbatim, so
//! renames are migration events.

use std::fmt;

use serde::{Deserialize, Serialize};

/// One of the user-rebindable hotspots on a Logi mouse. The order matches the
/// physical layout from front to side; [`ButtonId::ALL`] is consumed by the
/// default-binding generator and the popover trigger list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ButtonId {
    LeftClick,
    RightClick,
    MiddleClick,
    Back,
    Forward,
    /// The "ModeShift" button under the wheel — typically used for SmartShift /
    /// DPI cycle. Named `DpiToggle` for historical reasons.
    DpiToggle,
    /// The horizontal thumb wheel found on MX-line devices. Bind a click
    /// action; rotation isn't bindable yet (P1.2 / P1.5 follow-up).
    Thumbwheel,
    /// The thumb-pad gesture button on MX-line devices. The press itself
    /// fires the bound action; swipe directions are P1.5 territory.
    GestureButton,
}

impl ButtonId {
    pub const ALL: [ButtonId; 8] = [
        ButtonId::LeftClick,
        ButtonId::RightClick,
        ButtonId::MiddleClick,
        ButtonId::Back,
        ButtonId::Forward,
        ButtonId::DpiToggle,
        ButtonId::Thumbwheel,
        ButtonId::GestureButton,
    ];

    /// Human-readable label for popovers and tooltips.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            ButtonId::LeftClick => "Left Click",
            ButtonId::RightClick => "Right Click",
            ButtonId::MiddleClick => "Middle Click",
            ButtonId::Back => "Back",
            ButtonId::Forward => "Forward",
            ButtonId::DpiToggle => "DPI Toggle",
            ButtonId::Thumbwheel => "Thumb Wheel",
            ButtonId::GestureButton => "Gesture Button",
        }
    }
}

impl fmt::Display for ButtonId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// One of the five sub-bindings on the gesture button: hold + swipe up/down/
/// left/right or a plain click without movement. Logi ships these as
/// independent assignments (`SLOT_NAME_GESTURE_*_BUTTON` in the
/// `device_gesture_buttons_image` metadata block) — OpenLogi mirrors the
/// same shape.
///
/// Variant identifiers are TOML-stable: renames are migration events.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum GestureDirection {
    Up,
    Down,
    Left,
    Right,
    Click,
}

impl GestureDirection {
    pub const ALL: [GestureDirection; 5] = [
        GestureDirection::Up,
        GestureDirection::Down,
        GestureDirection::Left,
        GestureDirection::Right,
        GestureDirection::Click,
    ];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            GestureDirection::Up => "Up",
            GestureDirection::Down => "Down",
            GestureDirection::Left => "Left",
            GestureDirection::Right => "Right",
            GestureDirection::Click => "Click",
        }
    }

    /// Arrow glyph for compact list rendering.
    #[must_use]
    pub fn glyph(self) -> &'static str {
        match self {
            GestureDirection::Up => "↑",
            GestureDirection::Down => "↓",
            GestureDirection::Left => "←",
            GestureDirection::Right => "→",
            GestureDirection::Click => "·",
        }
    }
}

impl fmt::Display for GestureDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// Minimum dominant-axis travel (raw-XY units) before a held gesture commits to
/// a direction. Tuned to match Logitech Options+'s responsiveness.
pub const GESTURE_SWIPE_THRESHOLD: i32 = 50;
/// Maximum cross-axis travel allowed at the threshold, so only a reasonably
/// straight swipe commits. Grows with the dominant axis (`max(deadzone, 35%)`).
pub const GESTURE_SWIPE_DEADZONE: i32 = 40;

/// Classify the *running* raw-XY travel of a held gesture button into a
/// directional swipe, the instant it commits — or `None` while it's still too
/// short or too diagonal.
///
/// The dominant axis must pass [`GESTURE_SWIPE_THRESHOLD`] while the cross axis
/// stays within `max(`[`GESTURE_SWIPE_DEADZONE`]`, 35% of dominant)`. Callers
/// fire the bound action the moment this returns `Some` — mid-swipe, like
/// Options+ — rather than waiting for the button release; a press that never
/// commits a direction is treated as [`GestureDirection::Click`] on release.
///
/// Coordinates follow the device's raw-XY convention (`+x` = right, `+y` =
/// down), so an upward swipe (negative `dy`) maps to [`GestureDirection::Up`].
#[must_use]
pub fn detect_swipe(dx: i32, dy: i32) -> Option<GestureDirection> {
    let (abs_x, abs_y) = (dx.abs(), dy.abs());
    let dominant = abs_x.max(abs_y);
    if dominant < GESTURE_SWIPE_THRESHOLD {
        return None;
    }
    let cross_limit = GESTURE_SWIPE_DEADZONE.max(dominant * 35 / 100);
    if abs_x > abs_y {
        if abs_y > cross_limit {
            return None;
        }
        Some(if dx > 0 {
            GestureDirection::Right
        } else {
            GestureDirection::Left
        })
    } else {
        if abs_x > cross_limit {
            return None;
        }
        Some(if dy > 0 {
            GestureDirection::Down
        } else {
            GestureDirection::Up
        })
    }
}

/// Grouping for popover section headers.
///
/// Used by [`Action::category`] and rendered as a small muted label above
/// each group in the action picker.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    /// Cut, copy, paste, undo, redo, select-all, find, save.
    Editing,
    /// Browser navigation: tabs, page reload, back/forward.
    Browser,
    /// Playback and volume controls.
    Media,
    /// Physical mouse clicks.
    Mouse,
    /// DPI cycle and SmartShift.
    Dpi,
    /// Scroll direction shortcuts.
    Scroll,
    /// Window/app navigation: Mission Control, Launchpad, etc.
    Navigation,
    /// Lock screen, show desktop, system-level actions.
    System,
}

impl Category {
    /// Short label for popover section headers (already uppercase so callers
    /// don't have to transform it).
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Category::Editing => "EDITING",
            Category::Browser => "BROWSER",
            Category::Media => "MEDIA",
            Category::Mouse => "MOUSE",
            Category::Dpi => "DPI",
            Category::Scroll => "SCROLL",
            Category::Navigation => "NAVIGATION",
            Category::System => "SYSTEM",
        }
    }
}

/// What pressing a [`ButtonId`] should do.
///
/// Serialization uses serde's default external tagging: unit variants
/// serialize as a bare string (`"BrowserBack"`) and the tuple variant
/// serializes as a single-key table (`{ CustomShortcut = "my chord" }`).
///
/// **Stability contract:** existing variant *names* are frozen — they form the
/// on-disk `config.toml` schema. New variants may be appended freely; removing
/// or renaming a variant requires a `schema_version` bump and a migration.
///
/// `Action::execute` synthesizes the OS-level event for each variant.
/// On macOS it posts the event via `CGEventPost(kCGHIDEventTap, …)`.
/// On other platforms it logs a warning and returns immediately — the binary
/// compiles on all targets.
///
/// # Manual verification
///
/// `execute` is intentionally excluded from the automated test suite because
/// it would need to intercept the OS event queue. Smoke-test it manually:
/// bind a button to any action in the GUI and confirm the expected system event
/// fires when the button is pressed.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Action {
    // ── Mouse ────────────────────────────────────────────────────────────────
    /// Primary mouse button.
    LeftClick,
    /// Secondary mouse button.
    RightClick,
    /// Middle mouse button (wheel click).
    MiddleClick,

    // ── Editing ──────────────────────────────────────────────────────────────
    /// Copy the current selection (⌘C / Ctrl+C).
    Copy,
    /// Paste from the clipboard (⌘V / Ctrl+V).
    Paste,
    /// Cut the current selection (⌘X / Ctrl+X).
    Cut,
    /// Undo the last action (⌘Z / Ctrl+Z).
    Undo,
    /// Redo the last undone action (⌘⇧Z / Ctrl+Y).
    Redo,
    /// Select all content (⌘A / Ctrl+A).
    SelectAll,
    /// Open the find / search bar (⌘F / Ctrl+F).
    Find,
    /// Save the current document (⌘S / Ctrl+S).
    Save,

    // ── Browser / Navigation ──────────────────────────────────────────────────
    /// Navigate backward in browser history.
    BrowserBack,
    /// Navigate forward in browser history.
    BrowserForward,
    /// Open a new tab (⌘T / Ctrl+T).
    NewTab,
    /// Close the current tab (⌘W / Ctrl+W).
    CloseTab,
    /// Reopen the last closed tab (⌘⇧T / Ctrl+Shift+T).
    ReopenTab,
    /// Switch to the next tab (⌃⇥ / Ctrl+Tab).
    NextTab,
    /// Switch to the previous tab (⌃⇧⇥ / Ctrl+Shift+Tab).
    PrevTab,
    /// Reload the current page (⌘R / Ctrl+R).
    ReloadPage,

    // ── Navigation / Window ───────────────────────────────────────────────────
    /// macOS Mission Control (⌃↑).
    MissionControl,
    /// macOS App Exposé — all windows for the current app (⌃↓).
    AppExpose,
    /// Show the desktop (hide all windows).
    ShowDesktop,
    /// Open Launchpad.
    LaunchpadShow,

    // ── System ────────────────────────────────────────────────────────────────
    /// Lock the screen.
    LockScreen,
    /// Capture a screenshot.
    Screenshot,

    // ── Media ────────────────────────────────────────────────────────────────
    /// Toggle media play/pause.
    PlayPause,
    /// Skip to the next track.
    NextTrack,
    /// Go back to the previous track.
    PrevTrack,
    /// Increase system volume.
    VolumeUp,
    /// Decrease system volume.
    VolumeDown,
    /// Toggle system mute.
    MuteVolume,

    // ── DPI ──────────────────────────────────────────────────────────────────
    /// Step through the configured DPI preset list (P1.7).
    CycleDpiPresets,
    /// Jump to a specific zero-based preset in the device's DPI preset list.
    /// Out-of-range indices clamp to the list length at fire time (P1.7).
    SetDpiPreset(u8),
    /// Toggle the HID++ SmartShift ratchet/free-spin wheel mode (P1.1).
    ToggleSmartShift,

    // ── Scroll ───────────────────────────────────────────────────────────────
    /// Synthesise a vertical scroll-up tick.
    ScrollUp,
    /// Synthesise a vertical scroll-down tick.
    ScrollDown,
    /// Synthesise a horizontal scroll-left tick.
    HorizontalScrollLeft,
    /// Synthesise a horizontal scroll-right tick.
    HorizontalScrollRight,

    // ── Custom ───────────────────────────────────────────────────────────────
    /// Replay an arbitrary recorded key chord (P1.3).
    ///
    /// Holds the structured chord data so `execute` can post the real
    /// keystroke (macOS: CGEventPost with the encoded modifier flags).
    /// The `display` field is used by [`Action::label`] so the popover
    /// shows the user-friendly chord name.
    CustomShortcut(KeyCombo),
}

/// A modifier + virtual-key keystroke captured by the P1.3 recorder UI or
/// hand-authored in `config.toml`.
///
/// `modifiers` is a bitmask of [`KeyCombo::MOD_CMD`] etc. so the wire format
/// is a compact integer, not a string. `key_code` is the macOS virtual key
/// (kVK_*); other platforms map at `execute` time when they grow real
/// support.
///
/// `display` is purely for rendering — e.g. `"⌘⇧P"`. Callers regenerate it
/// from the captured chord; we keep it in the struct so older configs
/// continue to render the same label without re-deriving on every load.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyCombo {
    /// Bitmask of [`Self::MOD_CMD`] etc.
    pub modifiers: u8,
    /// macOS virtual key code (`kVK_*`). 0 means "no key" — useful for
    /// modifier-only placeholders that the recorder UI rejects.
    pub key_code: u16,
    /// Pre-rendered chord label, e.g. `"⌘⇧P"`. Empty falls through to a
    /// generated label at runtime.
    #[serde(default)]
    pub display: String,
}

impl KeyCombo {
    pub const MOD_CMD: u8 = 1 << 0;
    pub const MOD_SHIFT: u8 = 1 << 1;
    pub const MOD_CTRL: u8 = 1 << 2;
    pub const MOD_OPTION: u8 = 1 << 3;

    /// Build the human-readable label from the modifier bitmask + key code.
    /// Falls back to `"⌘key 0xNN"` when the key code isn't one of the
    /// commonly-recognised letters; the recorder UI usually overrides this
    /// with its own derivation.
    #[must_use]
    pub fn rendered_label(&self) -> String {
        if !self.display.is_empty() {
            return self.display.clone();
        }
        let mut out = String::new();
        if self.modifiers & Self::MOD_CTRL != 0 {
            out.push('⌃');
        }
        if self.modifiers & Self::MOD_OPTION != 0 {
            out.push('⌥');
        }
        if self.modifiers & Self::MOD_SHIFT != 0 {
            out.push('⇧');
        }
        if self.modifiers & Self::MOD_CMD != 0 {
            out.push('⌘');
        }
        match self.key_code {
            0x00 => out.push('A'),
            0x01 => out.push('S'),
            0x02 => out.push('D'),
            0x03 => out.push('F'),
            0x06 => out.push('Z'),
            0x07 => out.push('X'),
            0x08 => out.push('C'),
            0x09 => out.push('V'),
            0x0B => out.push('B'),
            0x0C => out.push('Q'),
            0x0D => out.push('W'),
            0x0E => out.push('E'),
            0x0F => out.push('R'),
            0x10 => out.push('Y'),
            0x11 => out.push('T'),
            0x20 => out.push('U'),
            0x22 => out.push('I'),
            0x1F => out.push('O'),
            0x23 => out.push('P'),
            _ => {
                use std::fmt::Write as _;
                let _ = write!(out, "key 0x{:02X}", self.key_code);
            }
        }
        out
    }
}

impl Action {
    /// Display label for the popover row.
    ///
    /// Returns `String` rather than `&str` so parameterized variants (e.g.
    /// `SetDpiPreset(i)`, `CustomShortcut(s)`) can build a label that
    /// includes their payload.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Action::LeftClick => "Left Click".into(),
            Action::RightClick => "Right Click".into(),
            Action::MiddleClick => "Middle Click".into(),
            Action::Copy => "Copy".into(),
            Action::Paste => "Paste".into(),
            Action::Cut => "Cut".into(),
            Action::Undo => "Undo".into(),
            Action::Redo => "Redo".into(),
            Action::SelectAll => "Select All".into(),
            Action::Find => "Find".into(),
            Action::Save => "Save".into(),
            Action::BrowserBack => "Browser Back".into(),
            Action::BrowserForward => "Browser Forward".into(),
            Action::NewTab => "New Tab".into(),
            Action::CloseTab => "Close Tab".into(),
            Action::ReopenTab => "Reopen Tab".into(),
            Action::NextTab => "Next Tab".into(),
            Action::PrevTab => "Previous Tab".into(),
            Action::ReloadPage => "Reload Page".into(),
            Action::MissionControl => "Mission Control".into(),
            Action::AppExpose => "App Exposé".into(),
            Action::ShowDesktop => "Show Desktop".into(),
            Action::LaunchpadShow => "Launchpad".into(),
            Action::LockScreen => "Lock Screen".into(),
            Action::Screenshot => "Screenshot".into(),
            Action::PlayPause => "Play / Pause".into(),
            Action::NextTrack => "Next Track".into(),
            Action::PrevTrack => "Previous Track".into(),
            Action::VolumeUp => "Volume Up".into(),
            Action::VolumeDown => "Volume Down".into(),
            Action::MuteVolume => "Mute".into(),
            Action::CycleDpiPresets => "Cycle DPI Presets".into(),
            Action::SetDpiPreset(i) => format!("DPI Preset {}", i + 1),
            Action::ToggleSmartShift => "Toggle SmartShift".into(),
            Action::ScrollUp => "Scroll Up".into(),
            Action::ScrollDown => "Scroll Down".into(),
            Action::HorizontalScrollLeft => "Scroll Left".into(),
            Action::HorizontalScrollRight => "Scroll Right".into(),
            Action::CustomShortcut(combo) => combo.rendered_label(),
        }
    }

    /// Which [`Category`] this action belongs to, used for popover grouping.
    #[must_use]
    pub fn category(&self) -> Category {
        match self {
            Action::LeftClick | Action::RightClick | Action::MiddleClick => Category::Mouse,
            // CustomShortcut is assigned to Editing so it doesn't need a
            // separate arm (it's not in the picker catalog).
            Action::Copy
            | Action::Paste
            | Action::Cut
            | Action::Undo
            | Action::Redo
            | Action::SelectAll
            | Action::Find
            | Action::Save
            | Action::CustomShortcut(_) => Category::Editing,
            Action::BrowserBack
            | Action::BrowserForward
            | Action::NewTab
            | Action::CloseTab
            | Action::ReopenTab
            | Action::NextTab
            | Action::PrevTab
            | Action::ReloadPage => Category::Browser,
            Action::MissionControl
            | Action::AppExpose
            | Action::ShowDesktop
            | Action::LaunchpadShow => Category::Navigation,
            Action::LockScreen | Action::Screenshot => Category::System,
            Action::PlayPause
            | Action::NextTrack
            | Action::PrevTrack
            | Action::VolumeUp
            | Action::VolumeDown
            | Action::MuteVolume => Category::Media,
            Action::CycleDpiPresets | Action::SetDpiPreset(_) | Action::ToggleSmartShift => {
                Category::Dpi
            }
            Action::ScrollUp
            | Action::ScrollDown
            | Action::HorizontalScrollLeft
            | Action::HorizontalScrollRight => Category::Scroll,
        }
    }

    /// Whether holding the bound button should auto-repeat this action (like a
    /// held keyboard key), when key-repeat is enabled.
    ///
    /// Limited to the "analog" actions where repeating is the natural
    /// expectation — volume and scroll. One-shot actions (Copy, tab switching,
    /// DPI cycle, …) fire exactly once per press regardless: repeating them on a
    /// held button would be surprising rather than useful.
    #[must_use]
    pub fn is_repeatable(&self) -> bool {
        matches!(
            self,
            Action::VolumeUp
                | Action::VolumeDown
                | Action::ScrollUp
                | Action::ScrollDown
                | Action::HorizontalScrollLeft
                | Action::HorizontalScrollRight
        )
    }

    /// All pickable actions in a deterministic order.
    ///
    /// [`Action::CustomShortcut`] is intentionally excluded — it is opened via
    /// "Record shortcut…" (P1.3), not selected from the catalog.
    #[must_use]
    pub fn catalog() -> Vec<Action> {
        vec![
            // Mouse
            Action::LeftClick,
            Action::RightClick,
            Action::MiddleClick,
            // Editing
            Action::Copy,
            Action::Paste,
            Action::Cut,
            Action::Undo,
            Action::Redo,
            Action::SelectAll,
            Action::Find,
            Action::Save,
            // Browser
            Action::BrowserBack,
            Action::BrowserForward,
            Action::NewTab,
            Action::CloseTab,
            Action::ReopenTab,
            Action::NextTab,
            Action::PrevTab,
            Action::ReloadPage,
            // Navigation
            Action::MissionControl,
            Action::AppExpose,
            Action::ShowDesktop,
            Action::LaunchpadShow,
            // System
            Action::LockScreen,
            Action::Screenshot,
            // Media
            Action::PlayPause,
            Action::NextTrack,
            Action::PrevTrack,
            Action::VolumeUp,
            Action::VolumeDown,
            Action::MuteVolume,
            // DPI
            Action::CycleDpiPresets,
            Action::ToggleSmartShift,
            // Scroll
            Action::ScrollUp,
            Action::ScrollDown,
            Action::HorizontalScrollLeft,
            Action::HorizontalScrollRight,
        ]
    }

    /// Synthesise the OS-level event for this action.
    ///
    /// On macOS, key events are posted via `CGEventPost(kCGHIDEventTap, …)`
    /// using virtual key codes from the standard US keyboard layout, and the
    /// `LeftClick`/`RightClick`/`MiddleClick` variants synthesise a mouse click
    /// at the current cursor location. The WindowServer actions (`MissionControl`,
    /// `AppExpose`, `ShowDesktop`, `LaunchpadShow`) are posted straight to the
    /// Dock via `CoreDockSendNotification`. Device-side actions (`CycleDpiPresets`,
    /// `SetDpiPreset`, `ToggleSmartShift`) have no CGEvent equivalent and are
    /// handled at the hook/HID layer, logging a trace here.
    ///
    /// On other platforms a warning is logged and the function returns
    /// immediately — the binary compiles clean on all targets.
    pub fn execute(&self) {
        #[cfg(target_os = "macos")]
        self.execute_macos();

        #[cfg(target_os = "windows")]
        self.execute_windows();

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            tracing::warn!(
                action = self.label(),
                "Action::execute unsupported on this platform"
            );
        }
    }

    /// Windows implementation: synthesise the OS event via `SendInput`.
    ///
    /// Keyboard shortcuts use Windows virtual-key codes; mouse clicks and scroll
    /// ticks use mouse input. A few macOS-only WindowServer actions map to their
    /// closest Windows shell equivalent (documented inline). Device-side actions
    /// (`CycleDpiPresets`, `SetDpiPreset`, `ToggleSmartShift`) have no input
    /// equivalent and are handled at the hook/HID layer, logging a trace here.
    #[cfg(target_os = "windows")]
    fn execute_windows(&self) {
        use windows::vk;

        match self {
            // ── Mouse clicks ──────────────────────────────────────────────────
            Action::LeftClick => windows::left_click(),
            Action::RightClick => windows::right_click(),
            Action::MiddleClick => windows::middle_click(),
            // ── Editing (Ctrl is the primary modifier on Windows) ─────────────
            Action::Copy => windows::chord(&[vk::CONTROL], vk::C),
            Action::Paste => windows::chord(&[vk::CONTROL], vk::V),
            Action::Cut => windows::chord(&[vk::CONTROL], vk::X),
            Action::Undo => windows::chord(&[vk::CONTROL], vk::Z),
            Action::Redo => windows::chord(&[vk::CONTROL], vk::Y),
            Action::SelectAll => windows::chord(&[vk::CONTROL], vk::A),
            Action::Find => windows::chord(&[vk::CONTROL], vk::F),
            Action::Save => windows::chord(&[vk::CONTROL], vk::S),
            // ── Browser / Navigation ──────────────────────────────────────────
            // Dedicated browser keys; the hook layer also handles the physical
            // side buttons directly.
            Action::BrowserBack => windows::tap(vk::BROWSER_BACK),
            Action::BrowserForward => windows::tap(vk::BROWSER_FORWARD),
            Action::NewTab => windows::chord(&[vk::CONTROL], vk::T),
            Action::CloseTab => windows::chord(&[vk::CONTROL], vk::W),
            Action::ReopenTab => windows::chord(&[vk::CONTROL, vk::SHIFT], vk::T),
            Action::NextTab => windows::chord(&[vk::CONTROL], vk::TAB),
            Action::PrevTab => windows::chord(&[vk::CONTROL, vk::SHIFT], vk::TAB),
            Action::ReloadPage => windows::chord(&[vk::CONTROL], vk::R),
            // ── Navigation / Window: closest Windows shell equivalents ────────
            // macOS Mission Control / App Exposé both map to Task View (Win+Tab);
            // Windows has no separate "current-app windows" gesture.
            Action::MissionControl | Action::AppExpose => windows::chord(&[vk::LWIN], vk::TAB),
            Action::ShowDesktop => windows::chord(&[vk::LWIN], vk::D),
            // Launchpad ≈ the Start menu (app launcher): tap the Windows key.
            Action::LaunchpadShow => windows::tap(vk::LWIN),
            // ── System ────────────────────────────────────────────────────────
            Action::LockScreen => windows::chord(&[vk::LWIN], vk::L),
            // Win+Shift+S opens the Snip & Sketch region capture.
            Action::Screenshot => windows::chord(&[vk::LWIN, vk::SHIFT], vk::S),
            // ── Media ─────────────────────────────────────────────────────────
            Action::PlayPause => windows::tap(vk::MEDIA_PLAY_PAUSE),
            Action::NextTrack => windows::tap(vk::MEDIA_NEXT_TRACK),
            Action::PrevTrack => windows::tap(vk::MEDIA_PREV_TRACK),
            Action::VolumeUp => windows::tap(vk::VOLUME_UP),
            Action::VolumeDown => windows::tap(vk::VOLUME_DOWN),
            Action::MuteVolume => windows::tap(vk::VOLUME_MUTE),
            // ── DPI / SmartShift: handled at hook/HID layer ───────────────────
            Action::CycleDpiPresets | Action::SetDpiPreset(_) | Action::ToggleSmartShift => {
                tracing::debug!(
                    action = self.label(),
                    "device action handled by hook/HID layer"
                );
            }
            // ── Scroll ────────────────────────────────────────────────────────
            Action::ScrollUp => windows::scroll_vertical(1),
            Action::ScrollDown => windows::scroll_vertical(-1),
            Action::HorizontalScrollLeft => windows::scroll_horizontal(-1),
            Action::HorizontalScrollRight => windows::scroll_horizontal(1),
            // ── Custom recorded chord ─────────────────────────────────────────
            Action::CustomShortcut(combo) => windows::custom_shortcut(combo),
        }
    }

    /// macOS implementation: dispatch to the appropriate event helper.
    #[cfg(target_os = "macos")]
    fn execute_macos(&self) {
        use core_graphics::event::{CGEventFlags, CGMouseButton};

        // Modifier bit shorthands.
        let cmd = CGEventFlags::CGEventFlagCommand;
        let shift = CGEventFlags::CGEventFlagShift;
        let ctrl = CGEventFlags::CGEventFlagControl;
        let none = CGEventFlags::CGEventFlagNull;

        match self {
            // ── Mouse clicks: synthesise a click at the cursor ────────────────
            // Remapping a *different* button to a click lands here (e.g. Back →
            // MiddleClick). A button left on its own native click never reaches
            // this — the hook passes it straight through to the OS.
            Action::LeftClick => macos::post_click(CGMouseButton::Left),
            Action::RightClick => macos::post_click(CGMouseButton::Right),
            Action::MiddleClick => macos::post_click(CGMouseButton::Center),
            // ── Editing ───────────────────────────────────────────────────────
            Action::Copy => macos::post_key(VK_C, cmd),
            Action::Paste => macos::post_key(VK_V, cmd),
            Action::Cut => macos::post_key(VK_X, cmd),
            Action::Undo => macos::post_key(VK_Z, cmd),
            Action::Redo => macos::post_key(VK_Z, cmd | shift),
            Action::SelectAll => macos::post_key(VK_A, cmd),
            Action::Find => macos::post_key(VK_F, cmd),
            Action::Save => macos::post_key(VK_S, cmd),
            // ── Browser / Navigation ──────────────────────────────────────────
            // BrowserBack/Forward: Cmd+[ / Cmd+] as keyboard fallback; hook
            // layer handles the physical mouse buttons directly.
            // kVK_ANSI_LeftBracket = 0x21, kVK_ANSI_RightBracket = 0x1E
            Action::BrowserBack => macos::post_key(0x21, cmd),
            Action::BrowserForward => macos::post_key(0x1E, cmd),
            Action::NewTab => macos::post_key(VK_T, cmd),
            Action::CloseTab => macos::post_key(VK_W, cmd),
            Action::ReopenTab => macos::post_key(VK_T, cmd | shift),
            Action::NextTab => macos::post_key(VK_TAB, ctrl),
            Action::PrevTab => macos::post_key(VK_TAB, ctrl | shift),
            Action::ReloadPage => macos::post_key(VK_R, cmd),
            // ── Navigation / Window: posted straight to the Dock ──────────────
            // Synthesising these shortcuts is unreliable — the WindowServer
            // matcher needs the exact configured key (incl. the Fn flag) and
            // Show Desktop ignores synthetic events entirely — so they go to the
            // Dock via `CoreDockSendNotification`, which fires regardless of the
            // user's keyboard settings.
            Action::MissionControl => macos::mission_control(),
            Action::AppExpose => macos::app_expose(),
            Action::ShowDesktop => macos::show_desktop(),
            Action::LaunchpadShow => macos::launchpad(),
            // ── System ────────────────────────────────────────────────────────
            // Lock screen = Cmd+Ctrl+Q (kVK_ANSI_Q = 0x0C)
            Action::LockScreen => macos::post_key(0x0C, cmd | ctrl),
            // Screenshot = Cmd+Shift+3 (kVK_ANSI_3 = 0x14)
            Action::Screenshot => macos::post_key(0x14, cmd | shift),
            // ── Media ─────────────────────────────────────────────────────────
            // NX_KEYTYPE_PLAY=16, NEXT=17, PREVIOUS=18 via NSSystemDefined stub.
            Action::PlayPause => macos::post_media_key(0),
            Action::NextTrack => macos::post_media_key(1),
            Action::PrevTrack => macos::post_media_key(2),
            // kVK_VolumeUp/Down/Mute = 0x48/0x49/0x4A (ADB codes)
            Action::VolumeUp => macos::post_key(0x48, none),
            Action::VolumeDown => macos::post_key(0x49, none),
            Action::MuteVolume => macos::post_key(0x4A, none),
            // ── DPI / SmartShift: handled at hook/HID layer ───────────────────
            Action::CycleDpiPresets | Action::SetDpiPreset(_) | Action::ToggleSmartShift => {
                tracing::debug!(
                    action = self.label(),
                    "device action handled by hook/HID layer"
                );
            }
            // ── Scroll ────────────────────────────────────────────────────────
            Action::ScrollUp
            | Action::ScrollDown
            | Action::HorizontalScrollLeft
            | Action::HorizontalScrollRight => macos::post_scroll(self),
            // ── Custom ────────────────────────────────────────────────────────
            Action::CustomShortcut(combo) => {
                // P1.3: post the recorded chord. `key_code == 0` is the
                // "modifier-only placeholder" the recorder UI rejects;
                // skip it here too so a malformed config doesn't fire
                // bare modifier presses.
                if combo.key_code == 0 {
                    tracing::warn!(
                        chord = %combo.rendered_label(),
                        "CustomShortcut with no key code — press ignored"
                    );
                    return;
                }
                let mut flags = CGEventFlags::CGEventFlagNull;
                if combo.modifiers & KeyCombo::MOD_CMD != 0 {
                    flags |= CGEventFlags::CGEventFlagCommand;
                }
                if combo.modifiers & KeyCombo::MOD_SHIFT != 0 {
                    flags |= CGEventFlags::CGEventFlagShift;
                }
                if combo.modifiers & KeyCombo::MOD_CTRL != 0 {
                    flags |= CGEventFlags::CGEventFlagControl;
                }
                if combo.modifiers & KeyCombo::MOD_OPTION != 0 {
                    flags |= CGEventFlags::CGEventFlagAlternate;
                }
                macos::post_key(combo.key_code, flags);
            }
        }
    }
}

/// Synthesise a horizontal scroll of `delta` wheel lines at the current focus.
///
/// Used by the gesture/thumbwheel capture watcher to re-inject the MX thumb
/// wheel's scrolling after the wheel has been diverted over HID++ to capture its
/// click. `delta` is the device's raw rotation; its sign follows the wheel's
/// rotation convention and its magnitude (one line per rotation increment) may
/// need tuning per device, since the diverted resolution differs from native.
///
/// No-op (logs nothing) on platforms without CGEvent.
pub fn post_horizontal_scroll(delta: i32) {
    #[cfg(target_os = "macos")]
    macos::post_horizontal_scroll(delta);

    #[cfg(target_os = "windows")]
    windows::post_horizontal_scroll(delta);

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = delta;
}

// ── macOS virtual key codes ────────────────────────────────────────────────
// Source: <HIToolbox/Events.h> kVK_* constants. Values are layout-independent
// for the US ANSI keyboard.
#[cfg(target_os = "macos")]
const VK_A: u16 = 0x00;
#[cfg(target_os = "macos")]
const VK_C: u16 = 0x08;
#[cfg(target_os = "macos")]
const VK_F: u16 = 0x03;
#[cfg(target_os = "macos")]
const VK_R: u16 = 0x0F;
#[cfg(target_os = "macos")]
const VK_S: u16 = 0x01;
#[cfg(target_os = "macos")]
const VK_T: u16 = 0x11;
#[cfg(target_os = "macos")]
const VK_V: u16 = 0x09;
#[cfg(target_os = "macos")]
const VK_W: u16 = 0x0D;
#[cfg(target_os = "macos")]
const VK_X: u16 = 0x07;
#[cfg(target_os = "macos")]
const VK_Z: u16 = 0x06;
#[cfg(target_os = "macos")]
const VK_TAB: u16 = 0x30;

/// Platform helpers for synthesising OS-level input events on macOS.
#[cfg(target_os = "macos")]
mod macos {
    use core_graphics::event::{
        CGEvent, CGEventFlags, CGEventTapLocation, CGEventType, CGMouseButton, ScrollEventUnit,
    };
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
    use core_graphics::geometry::CGPoint;

    use crate::binding::Action;

    /// Post a mouse-down + mouse-up pair for `button` at the cursor's current
    /// location.
    ///
    /// Posted at the HID tap location, so OpenLogi's own event tap sees the
    /// synthetic click too: a `LeftClick`/`RightClick` flows straight through
    /// (the tap never owns the primary buttons), and a `MiddleClick` is left
    /// alone unless the user has *also* remapped the middle button.
    pub(super) fn post_click(button: CGMouseButton) {
        let Ok(src) = CGEventSource::new(CGEventSourceStateID::HIDSystemState) else {
            tracing::warn!("CGEventSource::new failed for click");
            return;
        };
        // A fresh event reports the current pointer location; mouse events need
        // an explicit position or they land at (0, 0).
        let location = CGEvent::new(src.clone()).map_or(CGPoint::new(0., 0.), |e| e.location());
        let (down, up) = match button {
            CGMouseButton::Left => (CGEventType::LeftMouseDown, CGEventType::LeftMouseUp),
            CGMouseButton::Right => (CGEventType::RightMouseDown, CGEventType::RightMouseUp),
            CGMouseButton::Center => (CGEventType::OtherMouseDown, CGEventType::OtherMouseUp),
        };
        for (kind, phase) in [(down, "down"), (up, "up")] {
            if let Ok(ev) = CGEvent::new_mouse_event(src.clone(), kind, location, button) {
                ev.post(CGEventTapLocation::HID);
            } else {
                tracing::warn!(phase, "CGEvent::new_mouse_event failed");
            }
        }
    }

    /// Post a key-down + key-up pair for `vk` with `flags` set.
    pub(super) fn post_key(vk: u16, flags: CGEventFlags) {
        let Ok(src) = CGEventSource::new(CGEventSourceStateID::HIDSystemState) else {
            tracing::warn!("CGEventSource::new failed");
            return;
        };
        let Ok(down) = CGEvent::new_keyboard_event(src.clone(), vk, true) else {
            tracing::warn!("CGEvent::new_keyboard_event(down) failed");
            return;
        };
        down.set_flags(flags);
        down.post(CGEventTapLocation::HID);
        let Ok(up) = CGEvent::new_keyboard_event(src, vk, false) else {
            tracing::warn!("CGEvent::new_keyboard_event(up) failed");
            return;
        };
        up.set_flags(flags);
        up.post(CGEventTapLocation::HID);
    }

    /// Post a media key event (Play/Pause, Next, Previous).
    ///
    /// `kind`: 0 = play/pause, 1 = next track, 2 = previous track.
    ///
    /// The proper implementation uses an `NSSystemDefined` event (type 14,
    /// subtype 8) which requires AppKit bindings. Until those land this
    /// function logs a debug trace so manual smoke tests can confirm the
    /// correct execution path.
    pub(super) fn post_media_key(kind: i32) {
        // NX_KEYTYPE_PLAY=16, NX_KEYTYPE_NEXT=17, NX_KEYTYPE_PREVIOUS=18.
        let nx_key: i64 = match kind {
            0 => 16,
            1 => 17,
            _ => 18,
        };
        tracing::debug!(
            nx_key,
            "media key event: NSSystemDefined stub — full AppKit impl tracked in P1.x"
        );
    }

    /// Post a synthetic scroll event for `action` (one of the `Scroll*` variants).
    pub(super) fn post_scroll(action: &Action) {
        let Ok(src) = CGEventSource::new(CGEventSourceStateID::HIDSystemState) else {
            tracing::warn!("CGEventSource::new failed for scroll");
            return;
        };
        let (v, h): (i32, i32) = match action {
            Action::ScrollUp => (3, 0),
            Action::ScrollDown => (-3, 0),
            Action::HorizontalScrollLeft => (0, -3),
            Action::HorizontalScrollRight => (0, 3),
            _ => return,
        };
        let Ok(ev) = CGEvent::new_scroll_event(src, ScrollEventUnit::PIXEL, 2, v, h, 0) else {
            tracing::warn!("CGEvent::new_scroll_event failed");
            return;
        };
        ev.post(CGEventTapLocation::HID);
    }

    /// Post a horizontal scroll of `delta` lines (wheel2 axis). Line units suit
    /// the thumb wheel's ratchet-like increments better than pixels.
    pub(super) fn post_horizontal_scroll(delta: i32) {
        let Ok(src) = CGEventSource::new(CGEventSourceStateID::HIDSystemState) else {
            tracing::warn!("CGEventSource::new failed for thumbwheel scroll");
            return;
        };
        let Ok(ev) = CGEvent::new_scroll_event(src, ScrollEventUnit::LINE, 2, 0, delta, 0) else {
            tracing::warn!("CGEvent::new_scroll_event failed for thumbwheel");
            return;
        };
        ev.post(CGEventTapLocation::HID);
    }

    pub(super) use dock::{app_expose, launchpad, mission_control, show_desktop};

    /// WindowServer window/space actions (Mission Control, App Exposé, Show
    /// Desktop, Launchpad).
    ///
    /// These are driven by the Dock, and synthesising their keyboard shortcut is
    /// unreliable — the WindowServer matcher needs the exact configured key
    /// (incl. the Fn flag) and Show Desktop's in particular doesn't respond. So
    /// we post the action straight to the Dock via the private
    /// `CoreDockSendNotification` SPI, which fires it regardless of the user's
    /// Keyboard settings.
    ///
    /// Isolated in its own submodule so the `unsafe` the `dlopen`/`dlsym` FFI
    /// needs is scoped here rather than spread across the platform helpers.
    #[allow(
        unsafe_code,
        reason = "the private CoreDockSendNotification SPI is only reachable via dlopen/dlsym FFI"
    )]
    mod dock {
        use std::ffi::{CStr, c_char, c_int, c_void};
        use std::sync::OnceLock;

        use core_foundation::base::TCFType;
        use core_foundation::string::CFString;

        /// Show all windows across spaces (Mission Control).
        pub(crate) fn mission_control() {
            send("com.apple.expose.awake");
        }

        /// Show the front app's windows (App Exposé).
        pub(crate) fn app_expose() {
            send("com.apple.expose.front.awake");
        }

        /// Move all windows aside to reveal the desktop.
        pub(crate) fn show_desktop() {
            send("com.apple.showdesktop.awake");
        }

        /// Toggle Launchpad. A no-op on macOS 26, which removed Launchpad.
        pub(crate) fn launchpad() {
            send("com.apple.launchpad.toggle");
        }

        /// Post `notification` to the Dock. Logs and returns on any failure.
        fn send(notification: &str) {
            let Some(core_dock_send) = core_dock_send_notification() else {
                tracing::warn!(notification, "CoreDockSendNotification unavailable");
                return;
            };
            let name = CFString::new(notification);
            // SAFETY: resolved AppServices symbol called with its documented
            // signature; `name` is a live CFString for the call's duration.
            let err = unsafe { core_dock_send(name.as_concrete_TypeRef().cast(), 0) };
            if err != 0 {
                tracing::warn!(notification, err, "CoreDockSendNotification failed");
            }
        }

        type CoreDockSendNotificationFn = unsafe extern "C" fn(*const c_void, c_int) -> c_int;

        /// Resolve `CoreDockSendNotification` from `ApplicationServices`, caching
        /// the `dlopen` handle for the process lifetime. `None` if unavailable.
        fn core_dock_send_notification() -> Option<CoreDockSendNotificationFn> {
            const RTLD_LAZY: c_int = 0x1;
            const APP_SERVICES: &CStr =
                c"/System/Library/Frameworks/ApplicationServices.framework/ApplicationServices";
            static HANDLE: OnceLock<usize> = OnceLock::new();

            // SAFETY: `dlopen`/`dlsym` come from libSystem; APP_SERVICES is a
            // valid C string. The handle is cached and intentionally never
            // closed.
            let sym = unsafe {
                let handle =
                    *HANDLE.get_or_init(|| dlopen(APP_SERVICES.as_ptr(), RTLD_LAZY) as usize);
                if handle == 0 {
                    return None;
                }
                dlsym(handle as *mut c_void, c"CoreDockSendNotification".as_ptr())
            };
            // SAFETY: the symbol, when present, has the documented signature.
            (!sym.is_null()).then(|| unsafe {
                std::mem::transmute::<*mut c_void, CoreDockSendNotificationFn>(sym)
            })
        }

        unsafe extern "C" {
            fn dlopen(filename: *const c_char, flag: c_int) -> *mut c_void;
            fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
        }
    }
}

/// Platform helpers for synthesising OS-level input events on Windows via the
/// `SendInput` API. Mirrors the shape of the macOS [`macos`] helper module.
#[cfg(target_os = "windows")]
#[allow(
    unsafe_code,
    reason = "SendInput is a raw Win32 FFI call; isolated to this module"
)]
mod windows {
    #![allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss
    )]

    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
        KEYEVENTF_KEYUP, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
        MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
        MOUSEEVENTF_WHEEL, MOUSEINPUT, SendInput,
    };

    use super::KeyCombo;

    /// One wheel notch in `mouseData` units (`WHEEL_DELTA`).
    const WHEEL_DELTA: i32 = 120;

    /// Windows virtual-key codes used by the action catalog. Letters and digits
    /// are their ASCII codes; the rest are the documented `VK_*` constants.
    pub(super) mod vk {
        pub(in crate::binding) const CONTROL: u16 = 0x11;
        pub(in crate::binding) const SHIFT: u16 = 0x10;
        pub(in crate::binding) const MENU: u16 = 0x12; // Alt
        pub(in crate::binding) const LWIN: u16 = 0x5B;
        pub(in crate::binding) const TAB: u16 = 0x09;

        pub(in crate::binding) const A: u16 = 0x41;
        pub(in crate::binding) const C: u16 = 0x43;
        pub(in crate::binding) const D: u16 = 0x44;
        pub(in crate::binding) const F: u16 = 0x46;
        pub(in crate::binding) const L: u16 = 0x4C;
        pub(in crate::binding) const R: u16 = 0x52;
        pub(in crate::binding) const S: u16 = 0x53;
        pub(in crate::binding) const T: u16 = 0x54;
        pub(in crate::binding) const V: u16 = 0x56;
        pub(in crate::binding) const W: u16 = 0x57;
        pub(in crate::binding) const X: u16 = 0x58;
        pub(in crate::binding) const Y: u16 = 0x59;
        pub(in crate::binding) const Z: u16 = 0x5A;

        pub(in crate::binding) const BROWSER_BACK: u16 = 0xA6;
        pub(in crate::binding) const BROWSER_FORWARD: u16 = 0xA7;
        pub(in crate::binding) const VOLUME_MUTE: u16 = 0xAD;
        pub(in crate::binding) const VOLUME_DOWN: u16 = 0xAE;
        pub(in crate::binding) const VOLUME_UP: u16 = 0xAF;
        pub(in crate::binding) const MEDIA_NEXT_TRACK: u16 = 0xB0;
        pub(in crate::binding) const MEDIA_PREV_TRACK: u16 = 0xB1;
        pub(in crate::binding) const MEDIA_PLAY_PAUSE: u16 = 0xB3;
    }

    /// Keys that must carry `KEYEVENTF_EXTENDEDKEY`. Only the Windows key needs
    /// it here. The consumer keys (volume / media / browser) are recognised from
    /// their virtual-key code alone, and tagging them as extended actually stops
    /// the volume keys from registering on some systems — so they are left off.
    fn is_extended(key: u16) -> bool {
        key == vk::LWIN
    }

    /// Build a keyboard `INPUT` for `key`, as a press (`up == false`) or
    /// release (`up == true`).
    fn key_input(key: u16, up: bool) -> INPUT {
        let mut flags = 0u32;
        if up {
            flags |= KEYEVENTF_KEYUP;
        }
        if is_extended(key) {
            flags |= KEYEVENTF_EXTENDEDKEY;
        }
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: key,
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    /// Build a mouse `INPUT` carrying `flags` and `mouse_data` (used for the
    /// wheel amount / which X button; zero for plain clicks).
    fn mouse_input(flags: u32, mouse_data: i32) -> INPUT {
        INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: mouse_data as u32,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    /// Inject a batch of synthetic input events in order.
    fn send(inputs: &[INPUT]) {
        if inputs.is_empty() {
            return;
        }
        // SAFETY: `inputs` is a valid, non-empty slice of `INPUT`; we pass its
        // length and the exact `size_of::<INPUT>()` Windows expects.
        let sent = unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr(),
                core::mem::size_of::<INPUT>() as i32,
            )
        };
        if sent as usize != inputs.len() {
            tracing::warn!(
                sent,
                expected = inputs.len(),
                "SendInput injected fewer events than requested"
            );
        }
    }

    /// Press `modifiers` (in order), tap `key`, then release `modifiers` (in
    /// reverse) — a standard keyboard shortcut.
    pub(in crate::binding) fn chord(modifiers: &[u16], key: u16) {
        let mut inputs = Vec::with_capacity(modifiers.len() * 2 + 2);
        for &m in modifiers {
            inputs.push(key_input(m, false));
        }
        inputs.push(key_input(key, false));
        inputs.push(key_input(key, true));
        for &m in modifiers.iter().rev() {
            inputs.push(key_input(m, true));
        }
        send(&inputs);
    }

    /// Tap a single key (press + release) with no modifiers.
    pub(in crate::binding) fn tap(key: u16) {
        chord(&[], key);
    }

    pub(in crate::binding) fn left_click() {
        send(&[
            mouse_input(MOUSEEVENTF_LEFTDOWN, 0),
            mouse_input(MOUSEEVENTF_LEFTUP, 0),
        ]);
    }

    pub(in crate::binding) fn right_click() {
        send(&[
            mouse_input(MOUSEEVENTF_RIGHTDOWN, 0),
            mouse_input(MOUSEEVENTF_RIGHTUP, 0),
        ]);
    }

    pub(in crate::binding) fn middle_click() {
        send(&[
            mouse_input(MOUSEEVENTF_MIDDLEDOWN, 0),
            mouse_input(MOUSEEVENTF_MIDDLEUP, 0),
        ]);
    }

    /// Scroll `notches` vertically (positive = up, negative = down).
    pub(in crate::binding) fn scroll_vertical(notches: i32) {
        send(&[mouse_input(MOUSEEVENTF_WHEEL, notches * WHEEL_DELTA)]);
    }

    /// Scroll `notches` horizontally (positive = right, negative = left).
    pub(in crate::binding) fn scroll_horizontal(notches: i32) {
        send(&[mouse_input(MOUSEEVENTF_HWHEEL, notches * WHEEL_DELTA)]);
    }

    /// Re-inject a horizontal wheel scroll for the diverted MX thumb wheel.
    /// `delta` is the device's raw rotation (sign follows the wheel).
    pub(in crate::binding) fn post_horizontal_scroll(delta: i32) {
        send(&[mouse_input(MOUSEEVENTF_HWHEEL, delta * WHEEL_DELTA)]);
    }

    /// Translate a recorded macOS virtual key (`kVK_*`) to its Windows VK.
    /// Covers the ANSI letters the recorder UI captures; returns `None` for keys
    /// without a known mapping (the chord is then skipped with a warning).
    fn mac_vk_to_win(mac: u16) -> Option<u16> {
        Some(match mac {
            0x00 => vk::A,
            0x01 => vk::S,
            0x02 => vk::D,
            0x03 => vk::F,
            0x06 => vk::Z,
            0x07 => vk::X,
            0x08 => vk::C,
            0x09 => vk::V,
            0x0B => 0x42, // B
            0x0C => 0x51, // Q
            0x0D => vk::W,
            0x0E => 0x45, // E
            0x0F => vk::R,
            0x10 => vk::Y,
            0x11 => vk::T,
            0x20 => 0x55, // U
            0x22 => 0x49, // I
            0x1F => 0x4F, // O
            0x23 => 0x50, // P
            _ => return None,
        })
    }

    /// Replay a recorded custom shortcut. Modifiers map macOS → Windows
    /// (Command and Control both become Ctrl; Option becomes Alt).
    pub(in crate::binding) fn custom_shortcut(combo: &KeyCombo) {
        if combo.key_code == 0 {
            tracing::warn!(
                chord = %combo.rendered_label(),
                "CustomShortcut with no key code — press ignored"
            );
            return;
        }
        let Some(key) = mac_vk_to_win(combo.key_code) else {
            tracing::warn!(
                chord = %combo.rendered_label(),
                key_code = combo.key_code,
                "CustomShortcut key has no Windows mapping — press ignored"
            );
            return;
        };

        let mut modifiers: Vec<u16> = Vec::new();
        if combo.modifiers & (KeyCombo::MOD_CMD | KeyCombo::MOD_CTRL) != 0 {
            modifiers.push(vk::CONTROL);
        }
        if combo.modifiers & KeyCombo::MOD_SHIFT != 0 {
            modifiers.push(vk::SHIFT);
        }
        if combo.modifiers & KeyCombo::MOD_OPTION != 0 {
            modifiers.push(vk::MENU);
        }
        chord(&modifiers, key);
    }
}

/// Sensible defaults for a fresh device so the panel isn't empty on first run.
///
/// Thumbwheel / GestureButton defaults match what Logi Options+ ships for
/// MX-line devices: thumb wheel click → App Exposé, gesture button →
/// Mission Control. The thumb wheel isn't captured yet; the gesture button is
/// (per-direction, see [`default_gesture_binding`]). The bindings persist
/// regardless so the user only configures once.
///
/// `GestureButton`'s entry here is the legacy single-binding placeholder;
/// the per-direction sub-bindings live in [`default_gesture_binding`] and
/// are what the UI now edits.
#[must_use]
pub fn default_binding(button: ButtonId) -> Action {
    match button {
        ButtonId::LeftClick => Action::LeftClick,
        ButtonId::RightClick => Action::RightClick,
        ButtonId::MiddleClick => Action::MiddleClick,
        ButtonId::Back => Action::BrowserBack,
        ButtonId::Forward => Action::BrowserForward,
        ButtonId::DpiToggle => Action::CycleDpiPresets,
        ButtonId::Thumbwheel => Action::AppExpose,
        ButtonId::GestureButton => Action::MissionControl,
    }
}

/// Per-direction defaults for the gesture button. These are captured live over
/// HID++ `0x1b04` (raw-XY diversion) and dispatched like any other binding; the
/// defaults give the picker something sensible to show on first run.
#[must_use]
pub fn default_gesture_binding(direction: GestureDirection) -> Action {
    match direction {
        GestureDirection::Up => Action::MissionControl,
        GestureDirection::Down => Action::ShowDesktop,
        GestureDirection::Left => Action::PrevTab,
        GestureDirection::Right => Action::NextTab,
        GestureDirection::Click => Action::AppExpose,
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, reason = "expect/unwrap are idiomatic in tests")]
mod tests {
    use std::collections::BTreeMap;

    use serde::{Deserialize, Serialize};

    use super::*;

    // ── Roundtrip wrapper: defined here so it precedes any `let` statements ──

    /// Minimal TOML-serializable wrapper used by `roundtrip`.
    /// Defined at module scope to satisfy `clippy::items_after_statements`.
    #[derive(Serialize, Deserialize)]
    struct RoundtripWrapper {
        binding: BTreeMap<ButtonId, Action>,
    }

    // ── Catalog tests ─────────────────────────────────────────────────────────

    #[test]
    fn catalog_has_at_least_29_entries() {
        let catalog = Action::catalog();
        assert!(
            catalog.len() >= 29,
            "catalog has {} entries, need ≥ 29",
            catalog.len()
        );
    }

    #[test]
    fn catalog_excludes_custom_shortcut() {
        let catalog = Action::catalog();
        for action in &catalog {
            assert!(
                !matches!(action, Action::CustomShortcut(_)),
                "catalog must not contain CustomShortcut"
            );
        }
    }

    // ── Gesture classification ────────────────────────────────────────────────

    #[test]
    fn detect_swipe_below_threshold_keeps_accumulating() {
        // Too little travel to commit — caller keeps summing raw-XY.
        assert_eq!(detect_swipe(40, 5), None);
        assert_eq!(detect_swipe(0, 0), None);
    }

    #[test]
    fn detect_swipe_commits_clean_direction() {
        assert_eq!(detect_swipe(120, 5), Some(GestureDirection::Right));
        assert_eq!(detect_swipe(-120, 5), Some(GestureDirection::Left));
        assert_eq!(detect_swipe(5, 120), Some(GestureDirection::Down));
        assert_eq!(detect_swipe(5, -120), Some(GestureDirection::Up));
    }

    #[test]
    fn detect_swipe_rejects_diagonal() {
        // Past the threshold but too diagonal (cross axis beyond the band).
        assert_eq!(detect_swipe(60, 60), None);
        assert_eq!(detect_swipe(-60, -60), None);
    }

    // ── TOML roundtrip ────────────────────────────────────────────────────────

    /// Serialize then deserialize `action` through TOML, using a wrapper
    /// struct because TOML requires a top-level table.
    fn roundtrip(action: &Action) -> Action {
        let mut map: BTreeMap<ButtonId, Action> = BTreeMap::new();
        map.insert(ButtonId::Back, action.clone());
        let w = RoundtripWrapper { binding: map };
        let s = toml::to_string(&w).expect("serialize");
        let back: RoundtripWrapper = toml::from_str(&s).expect("deserialize");
        back.binding
            .into_values()
            .next()
            .expect("binding present after roundtrip")
    }

    #[test]
    fn all_catalog_variants_roundtrip_toml() {
        for action in Action::catalog() {
            let back = roundtrip(&action);
            assert_eq!(action, back, "TOML roundtrip failed for {action:?}");
        }
    }

    #[test]
    fn custom_shortcut_roundtrips_toml() {
        let action = Action::CustomShortcut(KeyCombo {
            modifiers: KeyCombo::MOD_CMD | KeyCombo::MOD_SHIFT,
            key_code: 0x23, // kVK_ANSI_P
            display: "⌘⇧P".into(),
        });
        assert_eq!(roundtrip(&action), action);
    }

    #[test]
    fn key_combo_rendered_label_uses_display_when_set() {
        let combo = KeyCombo {
            modifiers: 0,
            key_code: 0,
            display: "preset".into(),
        };
        assert_eq!(combo.rendered_label(), "preset");
    }

    #[test]
    fn key_combo_rendered_label_falls_back_to_modifiers_plus_key() {
        let combo = KeyCombo {
            modifiers: KeyCombo::MOD_CMD | KeyCombo::MOD_SHIFT,
            key_code: 0x23, // P
            display: String::new(),
        };
        assert_eq!(combo.rendered_label(), "⇧⌘P");
    }

    // ── Category tests ────────────────────────────────────────────────────────

    #[test]
    fn category_editing_variants() {
        assert_eq!(Action::Copy.category(), Category::Editing);
        assert_eq!(Action::Undo.category(), Category::Editing);
        assert_eq!(Action::SelectAll.category(), Category::Editing);
        assert_eq!(Action::Find.category(), Category::Editing);
        assert_eq!(Action::Save.category(), Category::Editing);
        assert_eq!(Action::Cut.category(), Category::Editing);
        assert_eq!(Action::Redo.category(), Category::Editing);
        assert_eq!(Action::Paste.category(), Category::Editing);
    }

    #[test]
    fn category_browser_variants() {
        assert_eq!(Action::BrowserBack.category(), Category::Browser);
        assert_eq!(Action::BrowserForward.category(), Category::Browser);
        assert_eq!(Action::NewTab.category(), Category::Browser);
        assert_eq!(Action::CloseTab.category(), Category::Browser);
        assert_eq!(Action::ReopenTab.category(), Category::Browser);
        assert_eq!(Action::NextTab.category(), Category::Browser);
        assert_eq!(Action::PrevTab.category(), Category::Browser);
        assert_eq!(Action::ReloadPage.category(), Category::Browser);
    }

    #[test]
    fn category_media_variants() {
        assert_eq!(Action::PlayPause.category(), Category::Media);
        assert_eq!(Action::NextTrack.category(), Category::Media);
        assert_eq!(Action::PrevTrack.category(), Category::Media);
        assert_eq!(Action::VolumeUp.category(), Category::Media);
        assert_eq!(Action::VolumeDown.category(), Category::Media);
        assert_eq!(Action::MuteVolume.category(), Category::Media);
    }

    #[test]
    fn category_mouse_variants() {
        assert_eq!(Action::LeftClick.category(), Category::Mouse);
        assert_eq!(Action::RightClick.category(), Category::Mouse);
        assert_eq!(Action::MiddleClick.category(), Category::Mouse);
    }

    #[test]
    fn category_dpi_variants() {
        assert_eq!(Action::CycleDpiPresets.category(), Category::Dpi);
        assert_eq!(Action::ToggleSmartShift.category(), Category::Dpi);
    }

    #[test]
    fn category_scroll_variants() {
        assert_eq!(Action::ScrollUp.category(), Category::Scroll);
        assert_eq!(Action::ScrollDown.category(), Category::Scroll);
        assert_eq!(Action::HorizontalScrollLeft.category(), Category::Scroll);
        assert_eq!(Action::HorizontalScrollRight.category(), Category::Scroll);
    }

    #[test]
    fn category_navigation_variants() {
        assert_eq!(Action::MissionControl.category(), Category::Navigation);
        assert_eq!(Action::AppExpose.category(), Category::Navigation);
        assert_eq!(Action::ShowDesktop.category(), Category::Navigation);
        assert_eq!(Action::LaunchpadShow.category(), Category::Navigation);
    }

    #[test]
    fn category_system_variants() {
        assert_eq!(Action::LockScreen.category(), Category::System);
        assert_eq!(Action::Screenshot.category(), Category::System);
    }

    // ── Category label smoke test ─────────────────────────────────────────────

    #[test]
    fn category_labels_are_nonempty() {
        let categories = [
            Category::Editing,
            Category::Browser,
            Category::Media,
            Category::Mouse,
            Category::Dpi,
            Category::Scroll,
            Category::Navigation,
            Category::System,
        ];
        for cat in categories {
            assert!(!cat.label().is_empty(), "label empty for {cat:?}");
        }
    }

    // ── Default binding ───────────────────────────────────────────────────────

    #[test]
    fn dpi_toggle_default_is_cycle_dpi_presets() {
        assert_eq!(
            default_binding(ButtonId::DpiToggle),
            Action::CycleDpiPresets
        );
    }
}
