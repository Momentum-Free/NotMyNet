use gpui_kit::{
    base::{Radio, RadioGroup, StyledExt as _},
    component::{
        ActiveTheme,
        button::{Button, ButtonVariants},
        input::{Input, InputState},
        label::Label,
        scroll::ScrollableElement,
    },
    prelude::FluentBuilder as _,
    *,
};
use notmynet_windows::{Appearance, Settings};

use crate::ui_monitor::{MonitorView, window_options};

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
            .collect::<Vec<_>>()
            .into_iter()
            .map(|(handle, row)| {
                cx.on_focus(&handle, window, move |this, window, _| {
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
                .text_sm().text_color(muted));

        div()
            .id("settings-window")
            .size_full()
            .flex()
            .flex_col()
            .bg(bg)
            .text_color(fg)
            .capture_key_down(|event, window, cx| {
                if event.keystroke.key == "escape" {
                    window.remove_window();
                    window.prevent_default();
                    cx.stop_propagation();
                }
            })
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
                            .tab_index(7)
                            .on_click(|_, window, _| window.remove_window()),
                    )
                    .child(
                        Button::new("apply")
                            .primary()
                            .label("Apply")
                            .tab_index(8)
                            .on_click(cx.listener(|this, _, window, cx| this.apply(window, cx))),
                    ),
            )
    }
}
