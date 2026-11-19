use gpui_kit::{
    base::{Radio, RadioGroup, StyledExt as _},
    component::{
        ActiveTheme, IndexPath,
        button::{Button, ButtonVariants},
        checkbox::Checkbox,
        input::{Input, InputState},
        label::Label,
        scroll::ScrollableElement,
        select::{Select, SelectItem, SelectState},
    },
    prelude::FluentBuilder as _,
    *,
};
use notmynet_windows::{Appearance, OverlayEdge, OverlayMonitor, OverlaySettings, Settings};

use crate::{
    overlay,
    ui_monitor::{MonitorView, window_options},
};

const MODES: [(Appearance, &str); 3] = [
    (Appearance::System, "System"),
    (Appearance::Light, "Light"),
    (Appearance::Dark, "Dark"),
];
const THEME_HELP: &str = "System follows the Windows app theme.";
const FIELDS: [&str; 5] = [
    "Probe interval (ms)",
    "Reply timeout (ms)",
    "Display count",
    "Fast RTT cutoff (ms)",
    "Slow RTT cutoff (ms)",
];
const EDGES: [(OverlayEdge, &str); 4] = [
    (OverlayEdge::Top, "Top"),
    (OverlayEdge::Bottom, "Bottom"),
    (OverlayEdge::Left, "Left"),
    (OverlayEdge::Right, "Right"),
];
const THICKNESS: &str = "Thickness (1–64 physical px)";
const TASKBAR: &str = "Behind transparent taskbar";
const TASKBAR_HELP: &str = "Fills the taskbar area instead of using the strip thickness, only when the selected edge matches the taskbar. The strip may not show through every taskbar.";
const OVERLAY_FIRST_ROW: usize = FIELDS.len() + 4;

type OverlaySelect<T> = SelectState<Vec<OverlayChoice<T>>>;

#[derive(Clone)]
struct OverlayChoice<T>(T, SharedString);

impl<T: Copy + Eq> SelectItem for OverlayChoice<T> {
    type Value = T;

    fn title(&self) -> SharedString {
        self.1.clone()
    }

    fn value(&self) -> &T {
        &self.0
    }
}

fn overlay_select<T: Copy + Eq + 'static>(
    choices: impl IntoIterator<Item = (T, impl Into<SharedString>)>,
    selected: T,
    window: &mut Window,
    cx: &mut App,
) -> Entity<OverlaySelect<T>> {
    let choices: Vec<_> = choices
        .into_iter()
        .map(|(value, name)| OverlayChoice(value, name.into()))
        .collect();
    let initial = choices
        .iter()
        .position(|item| item.0 == selected)
        .map(IndexPath::new);
    cx.new(|cx| SelectState::new(choices, initial, window, cx))
}

pub fn open(
    applied: Settings,
    monitor: WeakEntity<MonitorView>,
    cx: &mut App,
) -> Result<AnyWindowHandle> {
    gpui_kit::open_window(
        window_options(
            "NotMyNet Settings",
            size(px(490.), px(550.)),
            size(px(350.), px(280.)),
            cx,
        ),
        cx,
        move |window, cx| cx.new(|cx| SettingsView::new(applied, monitor, window, cx)),
    )
    .map(|(window, _)| window)
}

struct SettingsView {
    monitor: WeakEntity<MonitorView>,
    choices: [FocusHandle; 3],
    selected: usize,
    fields: [Entity<InputState>; 5],
    overlay_enabled: bool,
    overlay_monitor: Entity<OverlaySelect<OverlayMonitor>>,
    overlay_edge: Entity<OverlaySelect<OverlayEdge>>,
    thickness: Entity<InputState>,
    behind_taskbar: bool,
    transparency: Option<bool>,
    overlay_focus: [FocusHandle; 4],
    scroll: ScrollHandle,
    error: Option<String>,
    _focus_scroll: Vec<Subscription>,
}

impl SettingsView {
    fn new(
        applied: Settings,
        monitor: WeakEntity<MonitorView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let choices: [FocusHandle; 3] = std::array::from_fn(|_| cx.focus_handle());
        let selected = MODES
            .iter()
            .position(|(mode, _)| *mode == applied.appearance)
            .unwrap_or(0);
        choices[selected].focus(window, cx);
        let fields = [
            applied.interval_ms.to_string(),
            applied.timeout_ms.to_string(),
            applied.display_count.to_string(),
            applied.fast_ms.to_string(),
            applied.slow_ms.to_string(),
        ]
        .map(|value| cx.new(|cx| InputState::new(window, cx).default_value(value)));
        let mut screens = overlay::screens();
        // Keep a disconnected selection rather than silently moving the strip on Apply.
        if !screens
            .iter()
            .any(|(monitor, _)| *monitor == applied.overlay.monitor)
        {
            screens.push((
                applied.overlay.monitor,
                "Saved display (unavailable)".into(),
            ));
        }
        let overlay_monitor = overlay_select(screens, applied.overlay.monitor, window, cx);
        let overlay_edge = overlay_select(EDGES, applied.overlay.edge, window, cx);
        let thickness = cx.new(|cx| {
            InputState::new(window, cx).default_value(applied.overlay.thickness_px.to_string())
        });
        // Select obtains tab order from its FocusHandle in Kit 0.10. Checkboxes have
        // their own tab stops; the surrounding handles are only for scroll-on-focus.
        let overlay_focus = [
            cx.focus_handle(),
            overlay_monitor.focus_handle(cx).tab_index(8),
            overlay_edge.focus_handle(cx).tab_index(9),
            cx.focus_handle(),
        ];
        let focus_scroll = choices
            .iter()
            .cloned()
            .map(|handle| (handle, 1))
            .chain(
                fields
                    .iter()
                    .enumerate()
                    .map(|(index, field)| (field.read(cx).focus_handle(cx), index + 2)),
            )
            .chain(
                [
                    overlay_focus[0].clone(),
                    overlay_focus[1].clone(),
                    overlay_focus[2].clone(),
                    thickness.focus_handle(cx),
                    overlay_focus[3].clone(),
                ]
                .into_iter()
                .enumerate()
                .map(|(index, handle)| (handle, OVERLAY_FIRST_ROW + index)),
            )
            .collect::<Vec<_>>()
            .into_iter()
            .map(|(handle, row)| {
                cx.on_focus_in(&handle, window, move |this, window, _| {
                    this.scroll.scroll_to_item(row);
                    window.refresh();
                })
            })
            .collect();
        Self {
            monitor,
            choices,
            selected,
            fields,
            overlay_enabled: applied.overlay.enabled,
            overlay_monitor,
            overlay_edge,
            thickness,
            behind_taskbar: applied.overlay.behind_taskbar,
            transparency: overlay::transparency_enabled(),
            overlay_focus,
            scroll: ScrollHandle::new(),
            error: None,
            _focus_scroll: focus_scroll,
        }
    }

    fn select(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.selected = index;
        self.choices[index].focus(window, cx);
        cx.notify();
    }

    fn read_field<T: std::str::FromStr>(&self, index: usize, cx: &App) -> Result<T, String> {
        self.fields[index]
            .read(cx)
            .value()
            .trim()
            .parse()
            .map_err(|_| format!("Enter a non-negative whole number for {}.", FIELDS[index]))
    }

    fn draft(&self, cx: &App) -> Result<Settings, String> {
        Settings {
            interval_ms: self.read_field(0, cx)?,
            timeout_ms: self.read_field(1, cx)?,
            display_count: self.read_field(2, cx)?,
            fast_ms: self.read_field(3, cx)?,
            slow_ms: self.read_field(4, cx)?,
            appearance: MODES[self.selected].0,
            overlay: OverlaySettings {
                enabled: self.overlay_enabled,
                monitor: self
                    .overlay_monitor
                    .read(cx)
                    .selected_value()
                    .copied()
                    .ok_or("Select a ping strip monitor.")?,
                edge: self
                    .overlay_edge
                    .read(cx)
                    .selected_value()
                    .copied()
                    .ok_or("Select a ping strip edge.")?,
                thickness_px: self
                    .thickness
                    .read(cx)
                    .value()
                    .trim()
                    .parse::<u32>()
                    .ok()
                    .filter(|px| (1..=64).contains(px))
                    .ok_or(
                        "Enter a whole-number ping strip thickness from 1 to 64 physical pixels.",
                    )?,
                behind_taskbar: self.behind_taskbar,
            },
        }
        .validate()
    }

    fn apply(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let result = self.draft(cx).and_then(|settings| {
            self.monitor
                .update(cx, |monitor, cx| {
                    monitor.apply_settings(settings, window, cx)
                })
                .map_err(|_| "The monitor window has closed.".to_owned())?
        });
        match result {
            Ok(()) => window.remove_window(),
            Err(error) => {
                self.error = Some(error);
                cx.notify();
            }
        }
    }
}

impl Render for SettingsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (bg, fg, muted, border, ring, soft, danger) = (
            theme.background,
            theme.foreground,
            theme.muted_foreground,
            theme.border,
            theme.ring,
            theme.accent,
            theme.danger,
        );
        let radios = RadioGroup::new("appearance")
            .axis(Axis::Horizontal)
            .aria_label("Appearance")
            .aria_description(THEME_HELP)
            .flex()
            .flex_wrap()
            .gap_2()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let index = match event.keystroke.key.as_str() {
                    "left" | "up" => (this.selected + MODES.len() - 1) % MODES.len(),
                    "right" | "down" => (this.selected + 1) % MODES.len(),
                    _ => return,
                };
                this.select(index, window, cx);
                window.prevent_default();
                cx.stop_propagation();
            }))
            .children(MODES.iter().enumerate().map(|(index, (_, name))| {
                let selected = self.selected == index;
                let indicator = div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(16.))
                    .rounded_full()
                    .border_1()
                    .border_color(if selected { ring } else { muted })
                    .child(div().size(px(8.)).rounded_full().bg(if selected {
                        ring
                    } else {
                        gpui_kit::transparent_black()
                    }));
                Radio::new(*name)
                    .accessibility_label(*name)
                    .set_position(index + 1, MODES.len())
                    .checked(selected)
                    .track_focus(&self.choices[index])
                    .tab_stop(selected)
                    .tab_index(1)
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .border_1()
                    .border_color(border)
                    .when(selected, |el| el.bg(soft))
                    .focus(|el| el.border_color(ring).shadow_sm())
                    .on_change({
                        let handle = cx.weak_entity();
                        move |_, _, window, cx| {
                            handle
                                .update(cx, |this, cx| this.select(index, window, cx))
                                .ok();
                        }
                    })
                    .child(indicator)
                    .child(*name)
            }));

        let form = div().id("settings-form").size_full().track_scroll(&self.scroll)
            .overflow_y_scroll().flex().flex_col().gap_3().px_5().pt_4().pb_5()
            .child(Label::new("Settings").text_xl().font_semibold())
            .child(div().flex().flex_col().flex_shrink_0().gap_2()
                .child(Label::new("Appearance").font_semibold())
                .child(radios)
                .child(Label::new(THEME_HELP).text_sm().text_color(muted))
                .child(Label::new("Native title bars always follow Windows.").text_sm().text_color(muted)))
            .children(FIELDS.iter().enumerate().map(|(index, name)| {
                div().flex().items_center().justify_between().gap_3().flex_shrink_0()
                    .when(index == 0, |el| el.pt_2().border_t_1().border_color(border))
                    .child(Label::new(*name))
                    .child(Input::new(&self.fields[index]).aria_label(*name).tab_index(2 + index as isize).w(px(102.)))
            }))
            .child(Label::new("Probe settings apply next time you start. Appearance applies immediately when you select Apply.")
                .text_sm().text_color(muted))
            .child(div().flex().flex_col().gap_1().flex_shrink_0().pt_2().border_t_1().border_color(border)
                .child(Label::new("Ping strip").font_semibold())
                .child(Label::new("Click-through probe history along a monitor edge while monitoring. Applies immediately.")
                    .text_sm().text_color(muted)))
            .child(div().id("overlay-enabled-row").track_focus(&self.overlay_focus[0]).flex_shrink_0()
                .child(Checkbox::new("overlay-enabled").label("Show ping strip").tab_index(7)
                    .checked(self.overlay_enabled)
                    .on_change(cx.listener(|this, enabled: &bool, _, cx| {
                        this.overlay_enabled = *enabled;
                        cx.notify();
                    }))))
            .child(div().flex().flex_col().gap_1().flex_shrink_0()
                .child(Label::new("Monitor"))
                .child(Select::new(&self.overlay_monitor).accessibility_label("Ping strip monitor").w_full()))
            .child(div().flex().items_center().justify_between().gap_3().flex_shrink_0()
                .child(Label::new("Edge"))
                .child(div().w(px(102.)).flex_shrink_0().child(
                    Select::new(&self.overlay_edge).accessibility_label("Ping strip edge").w_full())))
            .child(div().flex().items_center().justify_between().gap_3().flex_shrink_0()
                .child(Label::new(THICKNESS))
                .child(Input::new(&self.thickness).aria_label("Ping strip thickness, 1 to 64 physical pixels")
                    .tab_index(10).w(px(102.))))
            .child(div().id("overlay-taskbar-row").track_focus(&self.overlay_focus[3]).flex().flex_col().gap_1().flex_shrink_0()
                .child(Checkbox::new("overlay-taskbar").label(TASKBAR).tab_index(11)
                    .aria_description(TASKBAR_HELP)
                    .checked(self.behind_taskbar)
                    .on_change(cx.listener(|this, enabled: &bool, _, cx| {
                        this.behind_taskbar = *enabled;
                        cx.notify();
                    })))
                .child(Label::new(TASKBAR_HELP).text_sm().text_color(muted))
                .when_some(match self.transparency {
                    Some(true) => None,
                    Some(false) => Some("Windows transparency effects are off. Turn them on in Windows Settings → Personalization → Colors."),
                    None => Some("Could not check transparency effects. Check Windows Settings → Personalization → Colors."),
                }, |el, warning| el.child(Label::new(warning).text_sm().text_color(muted))));

        div()
            .id("settings-window")
            .size_full()
            .flex()
            .flex_col()
            .bg(bg)
            .text_color(fg)
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key != "escape" {
                    return;
                }
                // An open Select exposes its list's focus handle in place of the trigger.
                let monitor = this.overlay_monitor.focus_handle(cx);
                let edge = this.overlay_edge.focus_handle(cx);
                if (monitor != this.overlay_focus[1] && monitor.is_focused(window))
                    || (edge != this.overlay_focus[2] && edge.is_focused(window))
                {
                    return;
                }
                window.remove_window();
                window.prevent_default();
                cx.stop_propagation();
            }))
            .child(
                div()
                    .min_h_0()
                    .flex_1()
                    .relative()
                    .child(form)
                    .vertical_scrollbar(&self.scroll),
            )
            .when_some(self.error.clone(), |el, error| {
                el.child(
                    div()
                        .id("settings-error")
                        .role(Role::Alert)
                        .px_5()
                        .py_2()
                        .child(Label::new(error).text_color(danger)),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .justify_end()
                    .gap_2()
                    .px_5()
                    .py_3()
                    .border_t_1()
                    .border_color(border)
                    .child(
                        Button::new("cancel")
                            .label("Cancel")
                            .tab_index(12)
                            .on_click(|_, window, _| window.remove_window()),
                    )
                    .child(
                        Button::new("apply")
                            .primary()
                            .label("Apply")
                            .tab_index(13)
                            .on_click(cx.listener(|this, _, window, cx| this.apply(window, cx))),
                    ),
            )
    }
}
