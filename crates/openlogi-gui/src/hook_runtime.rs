//! Runtime bridge between background input events and OpenLogi actions.
//!
//! The GPUI thread owns `AppState`, while the CGEventTap hook and HID++
//! gesture watcher run outside it. This module contains the shared runtime
//! surface between them: the binding map mirrored from `AppState`, lazy hook
//! installation, and action dispatch for both hook and gesture events.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError, RwLock};

use openlogi_core::binding::{Action, ButtonId};
use openlogi_hid::CaptureChannel;
use openlogi_hook::{EventDisposition, Hook, MouseEvent};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{debug, info, warn};

use crate::hardware::{toggle_smartshift_in_background, write_dpi_in_background};
use crate::state::{DpiCycleState, RepeatConfig};

/// Active auto-repeat timers for hook-captured buttons, keyed by button. Each
/// value is the stop flag the repeat thread polls; setting it ends the loop.
type HookRepeats = Arc<Mutex<HashMap<ButtonId, Arc<AtomicBool>>>>;

/// Shared binding map threaded between `AppState` and the hook callback.
pub type BindingMap = Arc<RwLock<BTreeMap<ButtonId, Action>>>;

/// Sink for "a button was pressed" notifications, drained by the GPUI loop in
/// `main.rs` to briefly highlight the button on the mouse diagram. Set once at
/// startup; both the OS hook and the HID++ gesture watcher publish to it.
static FLASH_SINK: OnceLock<UnboundedSender<ButtonId>> = OnceLock::new();

/// Register the UI flash sink. Call once before the hook / gesture watcher run.
pub fn set_flash_sink(tx: UnboundedSender<ButtonId>) {
    let _ = FLASH_SINK.set(tx);
}

/// Notify the UI that `button` was just pressed, so it can flash on the diagram.
/// No-op until [`set_flash_sink`] has run, and silently drops if the UI is gone.
pub fn flash_button(button: ButtonId) {
    if let Some(tx) = FLASH_SINK.get() {
        let _ = tx.send(button);
    }
}

/// Attempt to start the OS hook. Returns `None` if Accessibility is not
/// granted or on an unsupported platform — the app continues without crashing.
pub fn start(
    bindings: BindingMap,
    dpi_cycle: Arc<RwLock<DpiCycleState>>,
    capture: CaptureChannel,
    repeat_config: Arc<RwLock<RepeatConfig>>,
    rearm_tx: UnboundedSender<()>,
) -> Option<Hook> {
    if !Hook::has_accessibility() {
        warn!(
            "Accessibility not granted — events will not be captured. \
             Open System Settings → Privacy & Security → Accessibility."
        );
        return None;
    }

    // Auto-repeat timers for buttons that arrive on this (OS-hook) path rather
    // than over HID++ — e.g. a side button the device still delivers as a
    // standard mouse button. Mirrors the gesture watcher's repeat behaviour.
    let repeats: HookRepeats = Arc::new(Mutex::new(HashMap::new()));

    let result = Hook::start(move |event| match event {
        MouseEvent::Button { id, pressed } => {
            // The CGEventTap only sees standard buttons 0-4. We remap
            // Middle/Back/Forward; the primary L/R clicks always pass through
            // (suppressing them would brick the mouse), and the DPI / thumb /
            // gesture buttons aren't visible to the tap at all — the gesture
            // button is captured separately over HID++.
            if !matches!(
                id,
                ButtonId::MiddleClick | ButtonId::Back | ButtonId::Forward
            ) {
                return EventDisposition::PassThrough;
            }

            // Flash the button on the diagram for every press the hook sees —
            // even unbound ones — so the UI doubles as a detection indicator.
            if pressed {
                flash_button(id);
                // A Back/Forward press reaching this OS-hook path means the HID++
                // diversion has lapsed — typically after the (Bluetooth) mouse
                // slept and woke and dropped its diverted-control state. A
                // diverted button would arrive over HID++, not here. Nudge the
                // capture watcher to re-arm so hold-to-repeat works again.
                if matches!(id, ButtonId::Back | ButtonId::Forward) {
                    let _ = rearm_tx.send(());
                }
            }

            let action = bindings.read().ok().and_then(|g| g.get(&id).cloned());
            let Some(action) = action else {
                // Unbound → leave the physical button to the OS.
                return EventDisposition::PassThrough;
            };

            // A button left on its own native click (e.g. Middle → MiddleClick)
            // should just do that click; suppressing and re-synthesising it
            // would be pointless churn.
            if is_native_click(id, &action) {
                return EventDisposition::PassThrough;
            }

            if pressed {
                info!(button = %id, action = %action.label(), "button → executing bound action");
                dispatch_action(&action, &dpi_cycle, &capture);
                start_hook_repeat(id, &action, &repeat_config, &dpi_cycle, &capture, &repeats);
            } else {
                // Button up: stop any auto-repeat started on its press.
                stop_hook_repeat(id, &repeats);
            }
            EventDisposition::Suppress
        }
        MouseEvent::Scroll { .. } => EventDisposition::PassThrough,
    });

    match result {
        Ok(hook) => {
            info!("OS mouse hook installed");
            Some(hook)
        }
        Err(e) => {
            warn!(error = %e, "could not install OS mouse hook — events will not be captured");
            None
        }
    }
}

/// Start (or restart) a hook-path auto-repeat thread for `id` when key-repeat
/// is on and `action` is repeatable (volume / scroll). The first fire already
/// happened on the press; this schedules the repeats — the first after the
/// configured delay, then every interval — until [`stop_hook_repeat`] flips the
/// stop flag on the button's release.
///
/// Uses a plain OS thread rather than a Tokio timer: this path has no async
/// runtime, and running `SendInput` off the low-level-hook callback thread is
/// also healthier than blocking inside it.
fn start_hook_repeat(
    id: ButtonId,
    action: &Action,
    repeat_config: &Arc<RwLock<RepeatConfig>>,
    dpi_cycle: &Arc<RwLock<DpiCycleState>>,
    capture: &CaptureChannel,
    repeats: &HookRepeats,
) {
    let cfg = repeat_config
        .read()
        .map_or_else(|_| RepeatConfig::default(), |g| *g);
    if !cfg.enabled || !action.is_repeatable() {
        return;
    }

    let stop = Arc::new(AtomicBool::new(false));
    {
        let mut guard = repeats.lock().unwrap_or_else(PoisonError::into_inner);
        // Replace any stale timer for this button before starting a new one.
        if let Some(old) = guard.insert(id, Arc::clone(&stop)) {
            old.store(true, Ordering::Relaxed);
        }
    }

    let action = action.clone();
    let dpi_cycle = Arc::clone(dpi_cycle);
    let capture = Arc::clone(capture);
    std::thread::spawn(move || {
        std::thread::sleep(cfg.delay);
        let mut fires = 0u32;
        while !stop.load(Ordering::Relaxed) {
            dispatch_action(&action, &dpi_cycle, &capture);
            fires += 1;
            std::thread::sleep(cfg.interval);
        }
        debug!(button = %id, fires, "hook repeat ended");
    });
}

/// Stop the auto-repeat thread for `id`, if one is running.
fn stop_hook_repeat(id: ButtonId, repeats: &HookRepeats) {
    if let Some(stop) = repeats
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(&id)
    {
        stop.store(true, Ordering::Relaxed);
    }
}

/// Whether `action` is just `id`'s own native click — i.e. the button is mapped
/// to the very click it already produces. In that case the hook should pass the
/// event through to the OS rather than suppress and re-synthesise it.
fn is_native_click(id: ButtonId, action: &Action) -> bool {
    matches!(
        (id, action),
        (ButtonId::LeftClick, Action::LeftClick)
            | (ButtonId::RightClick, Action::RightClick)
            | (ButtonId::MiddleClick, Action::MiddleClick)
    )
}

/// Route a bound action either to OS-level event synthesis
/// ([`Action::execute`]) or to one of OpenLogi's hardware-side handlers.
///
/// `dpi_cycle` is held across a write lock long enough to advance the index
/// and snapshot the new DPI + target; the actual HID write spawns its own
/// thread via [`write_dpi_in_background`] to keep event callbacks non-blocking.
/// `capture` lets those writes reuse the capture session's open channel.
pub fn dispatch_action(
    action: &Action,
    dpi_cycle: &Arc<RwLock<DpiCycleState>>,
    capture: &CaptureChannel,
) {
    let next = match action {
        Action::CycleDpiPresets => match dpi_cycle.write() {
            Ok(mut guard) => guard.cycle(),
            Err(e) => {
                warn!(error = %e, "dpi_cycle lock poisoned — cycle skipped");
                None
            }
        },
        Action::SetDpiPreset(i) => match dpi_cycle.write() {
            Ok(mut guard) => guard.set(usize::from(*i)),
            Err(e) => {
                warn!(error = %e, "dpi_cycle lock poisoned — set skipped");
                None
            }
        },
        Action::ToggleSmartShift => {
            let target = dpi_cycle.read().ok().and_then(|g| g.target.clone());
            info!("SmartShift toggle → flipping wheel mode");
            toggle_smartshift_in_background(Some(capture), target);
            return;
        }
        other => {
            other.execute();
            None
        }
    };
    if let Some((dpi, target)) = next {
        info!(dpi, "DPI action → writing to device");
        write_dpi_in_background(Some(capture), target, dpi);
    } else if matches!(action, Action::CycleDpiPresets | Action::SetDpiPreset(_)) {
        info!(
            action = %action.label(),
            "no DPI presets configured for active device — press ignored"
        );
    }
}
