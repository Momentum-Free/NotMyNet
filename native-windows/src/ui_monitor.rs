use std::time::Duration;

use gpui_kit::{
    base::StyledExt as _,
    component::{
        ActiveTheme, IconName, Theme, ThemeMode,
        button::{Button, ButtonVariants},
        input::{Input, InputEvent, InputState},
        label::Label,
        scroll::ScrollableElement,
    },
    prelude::FluentBuilder as _,
    *,
};
use notmynet_windows::{Appearance, Monitor, Phase, Sample, Settings, Snapshot, format_duration};

use crate::{
    ui_history::{Recent, TILE_GAP, TILE_MIN, columns},
    ui_settings,
};

const PAD: f32 = 22.;
const SCROLLBAR_ROOM: f32 = 12.;
const DASH: &str = "—";

pub fn run() {
    application().with_assets(assets::Assets).run(|cx| {
        gpui_kit::init(cx);
        let settings = Settings::load();
        let (main, _) = gpui_kit::open_window(
            window_options(
                "NotMyNet",
                size(px(920.), px(650.)),
                size(px(390.), px(360.)),
                cx,
            ),
            cx,
            |window, cx| cx.new(|cx| MonitorView::new(settings, window, cx)),
        )
        .expect("Could not open NotMyNet");
        cx.on_window_closed(move |cx, id| {
            if id == main.window_id() {
                cx.quit();
            }
        })
        .detach();
    });
}

pub fn window_options(
    title: &'static str,
    initial: Size<Pixels>,
    minimum: Size<Pixels>,
    cx: &App,
) -> WindowOptions {
    WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: Some(title.into()),
            ..Default::default()
        }),
        window_bounds: Some(WindowBounds::centered(initial, cx)),
        window_min_size: Some(minimum),
        ..Default::default()
    }
}

fn apply_theme(appearance: Appearance, window: &mut Window, cx: &mut App) {
    // Kit refreshes both client areas. Windows/GPUI owns the title bars, which follow the OS.
    match appearance {
        Appearance::System => Theme::sync_system_appearance(Some(window), cx),
        Appearance::Light => Theme::change(ThemeMode::Light, Some(window), cx),
        Appearance::Dark => Theme::change(ThemeMode::Dark, Some(window), cx),
    }
    Theme::update(cx, |theme| {
        let [background, foreground, muted, border, input, ring, danger] = if theme.is_dark() {
            [
                0x181c22, 0xf5f7fa, 0xbdc7d4, 0x414956, 0x758295, 0x83b9ff, 0xffaab2,
            ]
        } else {
            [
                0xf6f7fa, 0x151922, 0x495364, 0xc7cdd7, 0x788395, 0x1b57b2, 0x9f202b,
            ]
        };
        theme.colors.background = rgb(background).into();
        theme.colors.foreground = rgb(foreground).into();
        theme.colors.muted_foreground = rgb(muted).into();
        theme.colors.border = rgb(border).into();
        theme.colors.input = rgb(input).into();
        theme.colors.ring = rgb(ring).into();
        theme.colors.danger = rgb(danger).into();
    });
}

pub struct MonitorView {
    settings: Settings,
    run_settings: Settings,
    target: Entity<InputState>,
    start_stop: FocusHandle,
    settings_button: FocusHandle,
    settings_window: Option<AnyWindowHandle>,
    monitor: Option<Monitor>,
    snapshot: Option<Snapshot>,
    start_error: Option<String>,
    window_error: Option<String>,
    refresh: Option<Task<()>>,
    history: ScrollHandle,
    history_focus: FocusHandle,
    _subscriptions: [Subscription; 3],
}

impl MonitorView {
    fn new(settings: Settings, window: &mut Window, cx: &mut Context<Self>) -> Self {
        apply_theme(settings.appearance, window, cx);
        let target = cx.new(|cx| InputState::new(window, cx).placeholder("IP address or hostname"));
        cx.focus_view(&target, window);
        let target_events = cx.subscribe_in(&target, window, |this, _, event, window, cx| {
            if let InputEvent::PressEnter { .. } = event
                && this.monitor.is_none()
            {
                this.toggle(window, cx);
            }
        });
        // gpui-pre-windows 0.3.12 updates window.appearance() on ImmersiveColorSet.
        let appearance = cx.observe_window_appearance(window, |this, window, cx| {
            if this.settings.appearance == Appearance::System
                && cx.theme().mode != ThemeMode::from(window.appearance())
            {
                apply_theme(Appearance::System, window, cx);
            }
        });
        let main = window.window_handle();
        let view = cx.weak_entity();
        let close = cx.on_window_closed(move |cx, id| {
            let _ = main.update(cx, |_, window, cx| {
                let _ = view.update(cx, |this, cx| {
                    if this
                        .settings_window
                        .is_some_and(|handle| handle.window_id() == id)
                    {
                        this.settings_window = None;
                        window.activate_window();
                        this.settings_button.focus(window, cx);
                        cx.notify();
                    }
                });
            });
        });

        Self {
            settings,
            run_settings: settings,
            target,
            start_stop: cx.focus_handle(),
            settings_button: cx.focus_handle(),
            settings_window: None,
            monitor: None,
            snapshot: None,
            start_error: None,
            window_error: None,
            refresh: None,
            history: ScrollHandle::new(),
            history_focus: cx.focus_handle(),
            _subscriptions: [target_events, appearance, close],
        }
    }

    pub fn apply_settings(
        &mut self,
        settings: Settings,
        settings_window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        settings
            .save()
            .map_err(|err| format!("Could not save settings: {err}"))?;
        let appearance_changed = settings.appearance != self.settings.appearance;
        self.settings = settings;
        if appearance_changed {
            apply_theme(settings.appearance, settings_window, cx);
        }
        cx.notify();
        Ok(())
    }

    fn open_settings(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.settings_window
            && handle
                .update(cx, |_, window, _| window.activate_window())
                .is_ok()
        {
            return;
        }
        self.settings_window = match ui_settings::open(self.settings, cx.weak_entity(), cx) {
            Ok(handle) => {
                self.window_error = None;
                Some(handle)
            }
            Err(err) => {
                self.window_error = Some(format!("Could not open Settings: {err}"));
                None
            }
        };
        cx.notify();
    }

    fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(monitor) = self.monitor.take() {
            self.snapshot = Some(monitor.snapshot());
            self.refresh = None;
        } else {
            let target = self.target.read(cx).value();
            self.snapshot = None;
            self.start_error = None;
            match Monitor::start(target.trim(), self.settings) {
                Ok(monitor) => {
                    self.run_settings = self.settings;
                    self.snapshot = Some(monitor.snapshot());
                    self.monitor = Some(monitor);
                    self.history.scroll_to_bottom();
                    self.start_stop.focus(window, cx);
                    self.refresh = Some(cx.spawn_in(window, async move |view, cx| {
                        let executor = cx.background_executor().clone();
                        loop {
                            executor.timer(Duration::from_millis(100)).await;
                            let updated = cx.update(|_, cx| {
                                view.update(cx, |this, cx| {
                                    let Some(monitor) = &this.monitor else {
                                        return false;
                                    };
                                    let snapshot = monitor.snapshot();
                                    let running = !matches!(snapshot.phase, Phase::Failed(_));
                                    this.snapshot = Some(snapshot);
                                    if !running {
                                        this.monitor = None;
                                    }
                                    cx.notify();
                                    running
                                })
                            });
                            if !matches!(updated, Ok(Ok(true))) {
                                break;
                            }
                        }
                    }));
                }
                Err(error) => self.start_error = Some(error),
            }
        }
        cx.notify();
    }

    fn status(&self, samples: &[Sample]) -> (Mark, &'static str, Option<String>) {
        if let Some(error) = &self.start_error {
            return (Mark::Error, "Error/Timeout", Some(error.clone()));
        }
        if let Some(snapshot) = &self.snapshot {
            match &snapshot.phase {
                Phase::Failed(reason) => {
                    return (Mark::Error, "Error/Timeout", Some(reason.clone()));
                }
                Phase::Resolving if self.monitor.is_some() => {
                    return (Mark::Pending, "Resolving target…", None);
                }
                _ => {}
            }
            // An outstanding or skipped slot is not a new success or failure.
            if let Some(result) = samples.iter().rev().find(|s| {
                matches!(
                    s,
                    Sample::Reply(_) | Sample::Error(_) | Sample::LocalError(_)
                )
            }) {
                return match result {
                    Sample::Reply(_) => (
                        Mark::for_sample(result, self.run_settings),
                        "Echo reply",
                        None,
                    ),
                    Sample::Error(reason) | Sample::LocalError(reason) => {
                        (Mark::Error, "Error/Timeout", Some(reason.clone()))
                    }
                    _ => unreachable!(),
                };
            }
            if snapshot.outage_elapsed.is_some() {
                return (Mark::Error, "Error/Timeout", None);
            }
            if self.monitor.is_none() {
                return (Mark::Empty, "Stopped", None);
            }
            return match samples.last() {
                Some(Sample::Pending) => (Mark::Pending, "Awaiting reply", None),
                Some(Sample::Skipped) => (Mark::Skipped, "Probe skipped", None),
                _ => (Mark::Pending, "Waiting for first probe", None),
            };
        }
        (Mark::Empty, "Ready to start", None)
    }

    fn history_grid(
        &self,
        samples: &[Sample],
        count: usize,
        cutoffs: Settings,
        width: f32,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let cols = columns(width);
        let tile_width =
            ((width - (cols - 1) as f32 * TILE_GAP) / cols as f32).clamp(TILE_MIN, 36.);
        let rows = count.div_ceil(cols);
        let last_cols = (count - 1) % cols + 1;
        let last_width = last_cols as f32 * (tile_width + TILE_GAP) - TILE_GAP;
        let muted = cx.theme().muted_foreground;
        let ring = cx.theme().ring;
        let dark = cx.theme().is_dark();
        let empty = Sample::Empty;
        let scroll = self.history.clone();
        // Start at the newest tail and follow it through reflow; respect a user who scrolls back.
        if scroll.offset().y <= -scroll.max_offset().y + px(1.) {
            scroll.scroll_to_bottom();
        }
        let row_limit =
            (((f32::from(window.viewport_size().height) - 174.) / 40.) as usize).clamp(3, 6);
        let overflow = rows > row_limit;
        let height = px((rows.min(row_limit) as f32) * 40. + 44.);
        div()
            .relative()
            .h(height)
            .child(
                div()
                    .id("history-scroll")
                    .size_full()
                    .pr(px(SCROLLBAR_ROOM))
                    .track_scroll(&scroll)
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .role(Role::Group)
                    .aria_label("Recent probe history, oldest to newest")
                    .track_focus(&self.history_focus)
                    .tab_index(4)
                    .tab_stop(overflow)
                    .when(self.history_focus.is_focused(window), |el| {
                        el.bg(cx.theme().accent).rounded_md()
                    })
                    .on_key_down(move |event, window, cx| {
                        let delta = match event.keystroke.key.as_str() {
                            "home" => scroll.max_offset().y,
                            "end" => -scroll.max_offset().y,
                            "pageup" => height - px(40.),
                            "pagedown" => px(40.) - height,
                            "up" => px(40.),
                            "down" => px(-40.),
                            _ => return,
                        };
                        scroll.set_offset(point(
                            px(0.),
                            (scroll.offset().y + delta).clamp(-scroll.max_offset().y, px(0.)),
                        ));
                        window.refresh();
                        window.prevent_default();
                        cx.stop_propagation();
                    })
                    .child(Label::new("Oldest").text_xs().text_color(muted))
                    .children((0..rows).map(|row| {
                        div().flex().flex_shrink_0().gap(px(TILE_GAP)).children(
                            (row * cols..((row + 1) * cols).min(count)).map(|index| {
                                let sample = samples.get(index).unwrap_or(&empty);
                                let mark = Mark::for_sample(sample, cutoffs);
                                let detail = match sample {
                                    Sample::Reply(ms) => format!("Reply, {ms} ms"),
                                    Sample::Error(reason) => format!("Error/Timeout: {reason}"),
                                    Sample::LocalError(reason) => format!("Local error: {reason}"),
                                    _ => mark.name().to_string(),
                                };
                                let position = if index + 1 == count {
                                    "Newest slot"
                                } else if index == 0 {
                                    "Oldest slot"
                                } else {
                                    "Slot"
                                };
                                tile(mark, dark, tile_width, 34.)
                                    .id(("sample", index))
                                    .role(Role::Label)
                                    .aria_label(format!("{position} {}, {detail}", index + 1))
                            }),
                        )
                    }))
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .flex_shrink_0()
                            .w(px(last_width.max(55.)))
                            .child(Label::new("Newest").text_xs().text_color(muted)),
                    ),
            )
            .when(self.history_focus.is_focused(window), |el| {
                el.child(
                    div()
                        .absolute()
                        .size_full()
                        .border_1()
                        .border_color(ring)
                        .rounded_md(),
                )
            })
            .when(overflow, |el| el.vertical_scrollbar(&self.history))
    }
}

impl Render for MonitorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (bg, fg, muted, border, dark) = (
            theme.background,
            theme.foreground,
            theme.muted_foreground,
            theme.border,
            theme.is_dark(),
        );
        let running = self.monitor.is_some();
        let snapshot = self.snapshot.as_ref();
        let samples = snapshot.map_or(&[][..], |data| data.samples.as_slice());
        let cutoffs = if snapshot.is_some() {
            self.run_settings
        } else {
            self.settings
        };
        let count = snapshot.map_or(self.settings.display_count, |data| data.samples.len());
        let recent = Recent::from_samples(samples);
        let (mark, status, detail) = self.status(samples);
        let session = snapshot.map_or_else(|| DASH.into(), |s| format_duration(s.session_elapsed));
        let outage = snapshot
            .and_then(|s| s.outage_elapsed)
            .map_or_else(|| DASH.into(), format_duration);
        let last_reply = rtt(snapshot.and_then(|s| s.last_reply_ms));
        let width = f32::from(window.viewport_size().width) - 2. * PAD - SCROLLBAR_ROOM;
        let awaiting_restart =
            running && probe_values(self.settings) != probe_values(self.run_settings);
        let loss = recent
            .loss()
            .map_or_else(|| DASH.to_owned(), |pct| format!("{pct:.1}%"));
        let avg = recent
            .avg()
            .map_or_else(|| DASH.to_owned(), |ms| format!("{ms:.1} ms"));
        let divider = || div().h(px(1.)).w_full().bg(border);

        let toolbar = div()
            .flex()
            .items_end()
            .justify_between()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_end()
                    .gap_2()
                    .min_w_0()
                    .flex_1()
                    .child(
                        div()
                            .flex()
                            .when(width < 500., |el| el.flex_col().gap_1())
                            .when(width >= 500., |el| el.items_center().gap_3())
                            .w_full()
                            .max_w(px(365.))
                            .min_w_0()
                            .child(Label::new("Target").font_semibold().flex_shrink_0())
                            .child(
                                Input::new(&self.target)
                                    .aria_label("Target (IP address or hostname)")
                                    .tab_index(1)
                                    .disabled(running)
                                    .w_full()
                                    .min_w_0(),
                            ),
                    )
                    .child(
                        Button::new("start-stop")
                            .primary()
                            .label(if running { "Stop" } else { "Start" })
                            .icon(if running {
                                IconName::Square
                            } else {
                                IconName::Play
                            })
                            .track_focus(&self.start_stop)
                            .tab_index(2)
                            .on_click(cx.listener(|this, _, window, cx| this.toggle(window, cx))),
                    ),
            )
            .child(
                Button::new("settings")
                    .secondary()
                    .label("Settings")
                    .icon(IconName::Settings)
                    .track_focus(&self.settings_button)
                    .tab_index(3)
                    .on_click(cx.listener(|this, _, _, cx| this.open_settings(cx))),
            );

        let history = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .flex_wrap()
                    .child(Label::new("Recent probes").text_xl().font_semibold())
                    .child(Label::new(format!("{count} slots")).text_color(muted)),
            )
            .child(self.history_grid(samples, count, cutoffs, width, window, cx))
            .child(
                div().flex().flex_wrap().gap_x_4().gap_y_2().children(
                    [
                        (Mark::Fast, format!("<{} ms", cutoffs.fast_ms)),
                        (
                            Mark::Medium,
                            format!("{}–<{} ms", cutoffs.fast_ms, cutoffs.slow_ms),
                        ),
                        (Mark::Slow, format!("≥{} ms", cutoffs.slow_ms)),
                        (Mark::Error, "Error/Timeout".into()),
                        (Mark::Pending, "Pending".into()),
                        (Mark::Skipped, "Skipped".into()),
                        (Mark::Empty, "Not sampled".into()),
                    ]
                    .map(|(mark, title)| {
                        div()
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .child(tile(mark, dark, 25., 24.))
                            .child(Label::new(title).text_sm())
                    }),
                ),
            )
            .child(
                Label::new(format!(
                    "{} replies · {} failed · {} local errors · {} pending · {} skipped",
                    recent.replies,
                    recent.errors,
                    recent.local_errors,
                    recent.pending,
                    recent.skipped
                ))
                .text_sm()
                .text_color(muted),
            );

        let status = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .flex_wrap()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(tile(mark, dark, 35., 33.))
                            .child(Label::new(status).text_xl().font_semibold()),
                    )
                    .child(metric("Last reply RTT", last_reply, muted)),
            )
            .when_some(detail, |el, detail| {
                el.child(Label::new(detail).text_sm().text_color(muted))
            })
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_x_4()
                    .gap_y_1()
                    .child(metric("Outage", outage, muted))
                    .child(metric(
                        "Session",
                        if snapshot.is_some() && !running {
                            format!("{session} (stopped)")
                        } else {
                            session
                        },
                        muted,
                    )),
            );

        div()
            .size_full()
            .bg(bg)
            .text_color(fg)
            .overflow_y_scrollbar()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .px(px(PAD))
                    .py_4()
                    .child(toolbar)
                    .when_some(self.window_error.clone(), |el, error| {
                        el.child(
                            div()
                                .id("window-error")
                                .role(Role::Alert)
                                .child(Label::new(error)),
                        )
                    })
                    .when(awaiting_restart, |el| {
                        el.child(
                            Label::new("New probe settings apply on next Start.")
                                .text_sm()
                                .text_color(muted),
                        )
                    })
                    .child(history)
                    .child(divider())
                    .child(status)
                    .child(divider())
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1p5()
                            .child(
                                Label::new("Recent probes · RTT from replies only")
                                    .text_sm()
                                    .text_color(muted),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap_x_5()
                                    .gap_y_2()
                                    .child(metric("Loss", loss, muted))
                                    .child(metric("Min RTT", rtt(recent.min), muted))
                                    .child(metric("Avg RTT", avg, muted))
                                    .child(metric("Worst RTT", rtt(recent.worst), muted)),
                            ),
                    )
                    .child(divider())
                    .child(
                        Label::new("ICMP replies do not measure service availability.")
                            .text_sm()
                            .text_color(muted),
                    ),
            )
    }
}

fn probe_values(settings: Settings) -> (u32, u32, usize, u32, u32) {
    (
        settings.interval_ms,
        settings.timeout_ms,
        settings.display_count,
        settings.fast_ms,
        settings.slow_ms,
    )
}

fn metric(label: &'static str, value: String, muted: Hsla) -> Div {
    div()
        .flex()
        .items_baseline()
        .gap_1p5()
        .child(Label::new(label).text_color(muted))
        .child(Label::new(value).font_semibold())
}

fn rtt(ms: Option<u32>) -> String {
    ms.map_or_else(|| DASH.into(), |ms| format!("{ms} ms"))
}

#[derive(Clone, Copy)]
enum Mark {
    Fast,
    Medium,
    Slow,
    Error,
    Pending,
    Skipped,
    Empty,
}

impl Mark {
    fn for_sample(sample: &Sample, settings: Settings) -> Self {
        match sample {
            Sample::Reply(ms) if *ms < settings.fast_ms => Self::Fast,
            Sample::Reply(ms) if *ms < settings.slow_ms => Self::Medium,
            Sample::Reply(_) => Self::Slow,
            Sample::Error(_) | Sample::LocalError(_) => Self::Error,
            Sample::Pending => Self::Pending,
            Sample::Skipped => Self::Skipped,
            Sample::Empty => Self::Empty,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::Skipped => "Skipped",
            Self::Empty => "Not sampled",
            Self::Fast | Self::Medium | Self::Slow => "Reply",
            Self::Error => "Error/Timeout",
        }
    }
}

fn tile(mark: Mark, dark: bool, width: f32, height: f32) -> Div {
    let (bg, ink, edge) = match (mark, dark) {
        (Mark::Fast, false) => (0x17632f, 0xffffff, 0x17632f),
        (Mark::Fast, true) => (0x1d5938, 0xe6ffe9, 0x59a876),
        (Mark::Medium, false) => (0xf2c420, 0x211b09, 0x8d6800),
        (Mark::Medium, true) => (0x57440f, 0xffe99c, 0xcda943),
        (Mark::Slow, false) => (0xe87221, 0x211306, 0xa44810),
        (Mark::Slow, true) => (0x613016, 0xffdbc0, 0xdb874f),
        (Mark::Error, false) => (0xffe4e5, 0x891d26, 0xb52e36),
        (Mark::Error, true) => (0x55252c, 0xffd0d4, 0xf1808b),
        (Mark::Pending, false) => (0xdcecff, 0x183d70, 0x466e9e),
        (Mark::Pending, true) => (0x213b56, 0xc7e6ff, 0x7aafd7),
        (Mark::Skipped, false) => (0xe4e8ed, 0x384a60, 0x61758d),
        (Mark::Skipped, true) => (0x343c49, 0xd0d9e6, 0x879bb4),
        (Mark::Empty, false) => (0xf3f4f6, 0x536070, 0x7b8897),
        (Mark::Empty, true) => (0x24272d, 0xaab7c7, 0x697b92),
    };
    let ink = rgb(ink);
    let glyph = match mark {
        Mark::Fast | Mark::Medium | Mark::Slow => {
            let bars = match mark {
                Mark::Fast => 1,
                Mark::Medium => 2,
                _ => 3,
            };
            div()
                .flex()
                .flex_col()
                .gap(px(3.))
                .children((0..bars).map(|_| div().w(px(12.)).h(px(3.)).rounded_full().bg(ink)))
        }
        _ => div()
            .text_size(px(18.))
            .line_height(px(20.))
            .font_bold()
            .child(match mark {
                Mark::Error => "×",
                Mark::Pending => "…",
                Mark::Skipped => "∕",
                _ => "·",
            }),
    };
    div()
        .w(px(width))
        .h(px(height))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .border_1()
        .border_color(rgb(edge))
        .rounded_md()
        .bg(rgb(bg))
        .text_color(ink)
        .child(glyph)
}
