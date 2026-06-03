//! The Settings window — a standalone OS window (⌘, / menu / footer link)
//! exposing the app-wide preferences in [`openlogi_core::config::AppSettings`].
//!
//! Two toggles for now, so the layout is a hand-rolled form rather than
//! gpui-component's [`Settings`](gpui_component::setting::Settings) widget
//! (whose 250px page sidebar would dwarf two switches). When the preference
//! set grows enough to warrant pages, this can migrate to that widget.

use gpui::{
    App, AppContext as _, BorrowAppContext as _, Context, Entity, FontWeight, IntoElement,
    ParentElement as _, Render, SharedString, Size, Styled as _, Subscription, Window, div, px, rgb,
};
use gpui_component::{
    Icon, IconName, IndexPath, Sizable,
    group_box::GroupBox,
    h_flex,
    scroll::ScrollableElement,
    select::{Select, SelectEvent, SelectItem, SelectState},
    slider::{Slider, SliderEvent, SliderState},
    switch::Switch,
    v_flex,
};

use crate::state::AppState;
use crate::theme::{self, ACCENT_BLUE, Palette};
use crate::windows::{self, AuxWindow};

// Key-repeat slider ranges (milliseconds).
const REPEAT_DELAY_MIN: f32 = 100.;
const REPEAT_DELAY_MAX: f32 = 1000.;
const REPEAT_DELAY_STEP: f32 = 50.;
const REPEAT_INTERVAL_MIN: f32 = 20.;
const REPEAT_INTERVAL_MAX: f32 = 300.;
const REPEAT_INTERVAL_STEP: f32 = 10.;

/// Standalone Settings window root view.
pub struct SettingsView {
    #[allow(dead_code, reason = "held to keep the appearance observer alive")]
    appearance_obs: Option<Subscription>,
    language_select: Entity<SelectState<Vec<LanguageOption>>>,
    repeat_delay_slider: Entity<SliderState>,
    repeat_interval_slider: Entity<SliderState>,
    #[allow(dead_code, reason = "held to keep the slider subscriptions alive")]
    repeat_subs: Vec<Subscription>,
}

impl SettingsView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let current = cx
            .try_global::<AppState>()
            .and_then(|s| s.app_settings().language.clone());
        let options = language_options();
        let selected = selected_language_index(current.as_deref(), &options);
        let language_select = cx.new(|cx| SelectState::new(options, Some(selected), window, cx));
        cx.subscribe_in(&language_select, window, Self::on_language_select)
            .detach();

        let (delay_ms, interval_ms) = cx.try_global::<AppState>().map_or((400, 60), |s| {
            let a = s.app_settings();
            (a.key_repeat_delay_ms, a.key_repeat_interval_ms)
        });
        let repeat_delay_slider = cx.new(|_| {
            SliderState::new()
                .max(REPEAT_DELAY_MAX)
                .min(REPEAT_DELAY_MIN)
                .step(REPEAT_DELAY_STEP)
                .default_value(ms_to_f32(delay_ms))
        });
        let repeat_interval_slider = cx.new(|_| {
            SliderState::new()
                .max(REPEAT_INTERVAL_MAX)
                .min(REPEAT_INTERVAL_MIN)
                .step(REPEAT_INTERVAL_STEP)
                .default_value(ms_to_f32(interval_ms))
        });
        // Commit on release only (not every drag tick) — each commit persists
        // config and republishes the live repeat config to the watcher.
        let repeat_subs = vec![
            cx.subscribe(
                &repeat_delay_slider,
                |_, _, event: &SliderEvent, cx| {
                    if let SliderEvent::Release(value) = event {
                        let ms = clamp_ms(value.start(), REPEAT_DELAY_MIN, REPEAT_DELAY_MAX);
                        cx.update_global::<AppState, _>(|s, _| s.set_key_repeat_delay_ms(ms));
                        cx.refresh_windows();
                    }
                },
            ),
            cx.subscribe(
                &repeat_interval_slider,
                |_, _, event: &SliderEvent, cx| {
                    if let SliderEvent::Release(value) = event {
                        let ms = clamp_ms(value.start(), REPEAT_INTERVAL_MIN, REPEAT_INTERVAL_MAX);
                        cx.update_global::<AppState, _>(|s, _| s.set_key_repeat_interval_ms(ms));
                        cx.refresh_windows();
                    }
                },
            ),
        ];

        Self {
            appearance_obs: None,
            language_select,
            repeat_delay_slider,
            repeat_interval_slider,
            repeat_subs,
        }
    }

    fn on_language_select(
        &mut self,
        _: &Entity<SelectState<Vec<LanguageOption>>>,
        event: &SelectEvent<Vec<LanguageOption>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let SelectEvent::Confirm(_) = event;
        let language = self
            .language_select
            .read(cx)
            .selected_value()
            .copied()
            .filter(|code| !code.is_empty())
            .map(ToOwned::to_owned);

        cx.update_global::<AppState, _>(|s, _| s.set_language(language));
        // `t!` reads the locale at render time, so a repaint is what actually
        // applies the switch; the app menu and status item aren't in any
        // window's view tree, so re-title them too. The status item's device
        // line lives on the spawn loop, so ask it to re-localize the whole menu
        // rather than writing from here.
        cx.refresh_windows();
        crate::app_menu::rebuild(cx);
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        crate::platform::tray::request_refresh();
    }
}

impl AuxWindow for SettingsView {
    fn set_appearance_obs(&mut self, sub: Subscription) {
        self.appearance_obs = Some(sub);
    }
}

/// Open the Settings window, or focus it if it's already open.
pub fn open(cx: &mut App) {
    windows::open_or_focus(
        |reg| &mut reg.settings,
        "Settings",
        Size::new(px(540.), px(620.)),
        SettingsView::new,
        cx,
    );
}

impl Render for SettingsView {
    #[allow(
        clippy::too_many_lines,
        reason = "a flat settings form reads more clearly inline than split across helpers"
    )]
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = theme::palette(cx);
        let (launch, updates) = cx.try_global::<AppState>().map_or((false, false), |s| {
            let a = s.app_settings();
            (a.launch_at_login, a.check_for_updates)
        });

        #[cfg(target_os = "macos")]
        let launch_desc = tr!("Automatically start OpenLogi when you log in to macOS.");
        #[cfg(not(target_os = "macos"))]
        let launch_desc = tr!("Automatically start OpenLogi when you log in.");

        let general = GroupBox::new()
            .title(group_title(IconName::Settings, tr!("General")))
            .child(setting_row(
                Switch::new("launch-at-login")
                    .checked(launch)
                    .on_click(cx.listener(|_, checked: &bool, _, cx| {
                        let enabled = *checked;
                        cx.update_global::<AppState, _>(move |s, _| {
                            s.set_launch_at_login(enabled);
                        });
                        cx.notify();
                    })),
                tr!("Launch at login"),
                launch_desc,
                pal,
            ))
            .child(setting_row(
                Switch::new("check-for-updates")
                    .checked(updates)
                    .on_click(cx.listener(|_, checked: &bool, _, cx| {
                        let enabled = *checked;
                        cx.update_global::<AppState, _>(move |s, _| {
                            s.set_check_for_updates(enabled);
                        });
                        cx.notify();
                    })),
                tr!("Check for updates"),
                tr!(
                    "Check once per launch for a new version (query only — no automatic download)."
                ),
                pal,
            ));

        // The tray toggle exists on platforms with a tray (macOS menu bar /
        // Windows notification area), with platform-appropriate wording.
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        let general = {
            let in_menu_bar = cx
                .try_global::<AppState>()
                .is_some_and(|s| s.app_settings().show_in_menu_bar);

            #[cfg(target_os = "macos")]
            let (tray_title, tray_desc) = (
                tr!("Show in menu bar"),
                tr!("Keep OpenLogi's icon in the menu bar. When off, it stays in the Dock instead."),
            );
            #[cfg(target_os = "windows")]
            let (tray_title, tray_desc) = (
                tr!("Show in tray"),
                tr!(
                    "Keep OpenLogi's icon in the notification area so it keeps running when you close the window."
                ),
            );

            general.child(setting_row(
                Switch::new("show-in-menu-bar")
                    .checked(in_menu_bar)
                    .on_click(cx.listener(|_, checked: &bool, _, cx| {
                        let enabled = *checked;
                        cx.update_global::<AppState, _>(move |s, _| {
                            s.set_show_in_menu_bar(enabled);
                        });
                        cx.notify();
                    })),
                tray_title,
                tray_desc,
                pal,
            ))
        };

        let (repeat_on, delay_ms, interval_ms) =
            cx.try_global::<AppState>().map_or((true, 400u32, 60u32), |s| {
                let a = s.app_settings();
                (
                    a.key_repeat_enabled,
                    a.key_repeat_delay_ms,
                    a.key_repeat_interval_ms,
                )
            });

        let key_repeat = GroupBox::new()
            .title(group_title(IconName::Redo2, tr!("Key repeat")))
            .child(setting_row(
                Switch::new("key-repeat-enabled")
                    .checked(repeat_on)
                    .on_click(cx.listener(|_, checked: &bool, _, cx| {
                        let enabled = *checked;
                        cx.update_global::<AppState, _>(move |s, _| {
                            s.set_key_repeat_enabled(enabled);
                        });
                        cx.notify();
                    })),
                tr!("Repeat while held"),
                tr!(
                    "Holding a side button bound to volume or scrolling repeats the action, like a held key."
                ),
                pal,
            ))
            .child(repeat_slider_row(
                tr!("Start delay"),
                format!("{delay_ms} ms"),
                &self.repeat_delay_slider,
                tr!("How long to hold before the repeat begins."),
                pal,
            ))
            .child(repeat_slider_row(
                tr!("Repeat rate"),
                format!("{interval_ms} ms"),
                &self.repeat_interval_slider,
                tr!("Time between repeats — smaller is faster."),
                pal,
            ));

        v_flex()
            .size_full()
            .bg(pal.bg)
            .text_color(pal.text_primary)
            .child(
                v_flex()
                    .w_full()
                    .p_6()
                    .gap_6()
                    .overflow_y_scrollbar()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(tr!("Settings")),
                    )
                    .child(general)
                    .child(key_repeat)
                    .child(
                        GroupBox::new()
                            .title(group_title(IconName::Globe, tr!("Language")))
                            .child(language_row(&self.language_select, pal)),
                    ),
            )
    }
}

#[derive(Clone)]
struct LanguageOption {
    label: &'static str,
    value: &'static str,
    localize_label: bool,
}

impl SelectItem for LanguageOption {
    type Value = &'static str;

    fn title(&self) -> SharedString {
        if self.localize_label {
            SharedString::from(rust_i18n::t!("Follow system").into_owned())
        } else {
            SharedString::from(self.label)
        }
    }

    fn value(&self) -> &Self::Value {
        &self.value
    }
}

fn language_options() -> Vec<LanguageOption> {
    let mut options = vec![LanguageOption {
        label: "Follow system",
        value: "",
        localize_label: true,
    }];
    options.extend(
        crate::i18n::SUPPORTED
            .iter()
            .map(|(code, name)| LanguageOption {
                label: name,
                value: code,
                localize_label: false,
            }),
    );
    options
}

fn selected_language_index(current: Option<&str>, options: &[LanguageOption]) -> IndexPath {
    let value = current.unwrap_or_default();
    let row = options
        .iter()
        .position(|option| option.value == value)
        .unwrap_or_default();
    IndexPath::default().row(row)
}

/// A GroupBox title with a small leading icon. `GroupBox::title` styles the
/// text itself, so this only lays the icon and label out inline.
fn group_title(icon: IconName, label: SharedString) -> impl IntoElement {
    h_flex()
        .gap_1p5()
        .items_center()
        .child(Icon::new(icon))
        .child(label)
}

/// One row: title + muted description on the left, the control on the right.
fn setting_row(
    control: Switch,
    title: impl Into<SharedString>,
    description: impl Into<SharedString>,
    pal: Palette,
) -> impl IntoElement {
    h_flex()
        .w_full()
        .items_center()
        .justify_between()
        .gap_4()
        .child(
            v_flex()
                .flex_1()
                .min_w(px(0.))
                .gap_1()
                .child(div().text_sm().child(title.into()))
                .child(
                    div()
                        .text_xs()
                        .text_color(pal.text_muted)
                        .child(description.into()),
                ),
        )
        .child(control)
}

/// One key-repeat slider row: a title + live value on top, the slider beneath,
/// and a muted one-line description under it. Mirrors [`setting_row`]'s spacing
/// but stacks vertically since a slider needs the full width.
fn repeat_slider_row(
    title: impl Into<SharedString>,
    value_label: impl Into<SharedString>,
    slider_state: &Entity<SliderState>,
    description: impl Into<SharedString>,
    pal: Palette,
) -> impl IntoElement {
    v_flex()
        .w_full()
        .gap_1p5()
        .child(
            h_flex()
                .w_full()
                .justify_between()
                .items_baseline()
                .child(div().text_sm().child(title.into()))
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(ACCENT_BLUE))
                        .child(value_label.into()),
                ),
        )
        .child(Slider::new(slider_state).horizontal())
        .child(
            div()
                .text_xs()
                .text_color(pal.text_muted)
                .child(description.into()),
        )
}

/// Widen a small millisecond value into f32 for slider math.
#[allow(
    clippy::cast_precision_loss,
    reason = "ms values are ≤ 1000 — far below f32 mantissa precision"
)]
fn ms_to_f32(ms: u32) -> f32 {
    ms as f32
}

/// Round + clamp a slider's raw f32 value into `[lo, hi]` milliseconds.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "value is rounded and clamped into a small ms range before the cast"
)]
fn clamp_ms(raw: f32, lo: f32, hi: f32) -> u32 {
    raw.clamp(lo, hi).round() as u32
}

/// The language picker. "Follow system" clears the stored preference (`None`);
/// the explicit locale entries come from [`crate::i18n::SUPPORTED`]. Selecting
/// one switches the locale live, then repaints every window and the menu bar so
/// the whole UI re-renders without a restart.
fn language_row(
    language_select: &Entity<SelectState<Vec<LanguageOption>>>,
    pal: Palette,
) -> impl IntoElement {
    h_flex()
        .w_full()
        .items_center()
        .justify_between()
        .gap_4()
        .child(
            div()
                .flex_1()
                .text_xs()
                .text_color(pal.text_muted)
                .child(tr!("Choose the interface language.")),
        )
        .child(
            Select::new(language_select)
                .small()
                .w(px(190.))
                .menu_width(px(190.)),
        )
}
