//! Auto-repeat timing shared with the HID++ capture watcher.
//!
//! Holds the live key-repeat settings the gesture watcher reads when a
//! hold-capable side button (Back / Forward / DPI) bound to a repeatable
//! action is pressed. Mirrored from [`AppSettings`] and updated in place when
//! the Settings UI changes any of the three fields, so a change takes effect
//! on the next button hold without a restart.

use std::time::Duration;

use openlogi_core::config::AppSettings;

/// Live, resolved key-repeat configuration.
#[derive(Debug, Clone, Copy)]
pub struct RepeatConfig {
    /// Whether auto-repeat is active at all.
    pub enabled: bool,
    /// Grace period after the first fire before repeats begin.
    pub delay: Duration,
    /// Gap between successive repeats (clamped to ≥ 1 ms so a misconfigured
    /// `0` can never spin a tight loop).
    pub interval: Duration,
}

impl RepeatConfig {
    /// Resolve the runtime config from the persisted [`AppSettings`].
    #[must_use]
    pub fn from_settings(settings: &AppSettings) -> Self {
        Self {
            enabled: settings.key_repeat_enabled,
            delay: Duration::from_millis(u64::from(settings.key_repeat_delay_ms)),
            interval: Duration::from_millis(u64::from(settings.key_repeat_interval_ms.max(1))),
        }
    }
}

impl Default for RepeatConfig {
    fn default() -> Self {
        Self::from_settings(&AppSettings::default())
    }
}
