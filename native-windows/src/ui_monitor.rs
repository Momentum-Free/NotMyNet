use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

use gpui_kit::{
    base::StyledExt as _,
    component::{
        ActiveTheme, Icon, IconName, Theme, ThemeMode,
        button::{Button, ButtonVariants},
        input::{Input, InputEvent, InputState},
        label::Label,
        scroll::ScrollableElement,
        tooltip::Tooltip,
    },
    prelude::FluentBuilder as _,
    *,
};
use notmynet_windows::{
    Appearance, LinkKind, LinkSpeed, Monitor, Phase, Sample, Settings, Snapshot, format_duration,
};

use windows_sys::Win32::{
    Foundation::{POINT, RECT},
    Graphics::Gdi::{MONITOR_DEFAULTTOPRIMARY, MonitorFromPoint},
    UI::{
        HiDpi::{
            AdjustWindowRectExForDpi, GetDpiForMonitor, GetDpiForSystem, GetSystemMetricsForDpi,
            MDT_EFFECTIVE_DPI,
        },
        WindowsAndMessaging::{
            SM_CXFRAME, SM_CXPADDEDBORDER, SM_CYFRAME, USER_DEFAULT_SCREEN_DPI, WS_EX_APPWINDOW,
            WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_SYSMENU, WS_THICKFRAME,
        },
    },
};

use crate::{
    overlay::Overlay,
    ui_history::{Grid, Recent, launch_height, snap},
    ui_settings,
};

const PAD: f32 = 22.;
const MIN_HEIGHT: f32 = 360.;
const SCROLLBAR_ROOM: f32 = 12.;
const DASH: &str = "—";
const UNAVAILABLE: &str = "Unavailable";

pub fn run() {
    application().with_assets(assets::AllAssets).run(|cx| {
        gpui_kit::init(cx);
        let settings = Settings::load();
        let (main, view) = gpui_kit::open_window(monitor_options(cx), cx, |window, cx| {
            cx.new(|cx| MonitorView::new(settings, window, cx))
        })
        .expect("Could not open NotMyNet");
        // open_window draws once before returning. Fit once from that measured
        // layout, before entering the event loop; never resize on Apply or render.
        main.update(cx, |_, window, cx| {
            let viewport = window.viewport_size();
            let grid = monitor_grid(settings.display_count, window);
            let height = px(launch_height(
                view.read(cx).chrome_height.get(),
                &grid,
                MIN_HEIGHT,
                f32::from(viewport.height),
            ));
            if height < viewport.height {
                window.resize(size(viewport.width, height));
            }
        })
        .expect("Could not fit NotMyNet");
        cx.on_window_closed(move |cx, id| {
            if id == main.window_id() {
                cx.quit();
            }
        })
        .detach();
    });
}

fn monitor_options(cx: &App) -> WindowOptions {
    let initial = size(px(920.), px(540.));
    let minimum = size(px(390.), px(MIN_HEIGHT));
    let mut options = window_options("NotMyNet", initial, minimum, cx);
    if let Some(display) = cx.primary_display() {
        let work = display.visible_bounds();
        let frame = primary_frame();
        let cap = (work.size - frame).max(&size(px(1.), px(1.)));
        let client = size(initial.width.min(cap.width), cap.height);
        let mut bounds = Bounds::centered_at(work.center(), client);
        // GPUI supplies these bounds to Win32 SetWindowPlacement, which uses
        // workspace coordinates for a normal app window (not screen coordinates).
        bounds.origin -= work.origin - display.bounds().origin;
        options.display_id = Some(display.id());
        options.window_bounds = Some(WindowBounds::Windowed(bounds));
        options.window_min_size = Some(minimum.min(&client));
    }
    options
}

fn primary_frame() -> Size<Pixels> {
    let (mut dpi, mut dpi_y) = (0, 0);
    let mut rect = RECT::default();
    // SAFETY: all outputs are initialized stack values, and these calls only
    // query display metrics. Use GPUI Windows' normal resizable window styles.
    let (width, height) = unsafe {
        let monitor = MonitorFromPoint(POINT::default(), MONITOR_DEFAULTTOPRIMARY);
        if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi, &mut dpi_y) < 0 || dpi == 0 {
            dpi = GetDpiForSystem().max(USER_DEFAULT_SCREEN_DPI);
        }
        let style = WS_SYSMENU | WS_THICKFRAME | WS_MINIMIZEBOX | WS_MAXIMIZEBOX;
        if AdjustWindowRectExForDpi(&mut rect, style, 0, WS_EX_APPWINDOW, dpi) != 0 {
            (rect.right - rect.left, rect.bottom - rect.top)
        } else {
            let frame = |index| GetSystemMetricsForDpi(index, dpi);
            let padding = frame(SM_CXPADDEDBORDER);
            (
                2 * (frame(SM_CXFRAME) + padding),
                2 * (frame(SM_CYFRAME) + padding),
            )
        }
    };
    let scale = dpi as f32 / USER_DEFAULT_SCREEN_DPI as f32;
    size(px(width as f32 / scale), px(height as f32 / scale))
}

fn monitor_grid(count: usize, window: &Window) -> Grid {
    let scale = window.scale_factor();
    let width = f32::from(window.viewport_size().width)
        - 2. * snap(PAD, scale)
        - snap(SCROLLBAR_ROOM, scale);
    Grid::new(
        width,
        count,
        scale,
        1.25 * f32::from(window.rem_size()), // Label's single-line height.
    )
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
    overlay: Overlay,
    overlay_error: Option<String>,
    refresh: Option<Task<()>>,
    chrome_height: Rc<Cell<f32>>,
    history: ScrollHandle,
    tile_bounds: Rc<RefCell<Vec<Bounds<Pixels>>>>,
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
            overlay: Overlay::default(),
            overlay_error: None,
            refresh: None,
            chrome_height: Rc::default(),
            history: ScrollHandle::new(),
            tile_bounds: Rc::default(),
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
        self.update_overlay(cx);
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
            let mut snapshot = monitor.snapshot();
            // Keep the captured peak and RTT; only clear live link metadata on Stop.
            snapshot.link_speed = None;
            self.snapshot = Some(snapshot);
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
                    // Foreground polling is tied to the view's lifetime, not its window drawing.
                    self.refresh = Some(cx.spawn(async move |view, cx| {
                        let executor = cx.background_executor().clone();
                        loop {
                            executor.timer(Duration::from_millis(100)).await;
                            let updated = cx.update(|cx| {
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
                                    if !running || this.settings.overlay.enabled {
                                        this.update_overlay(cx);
                                    }
                                    cx.notify();
                                    running
                                })
                            });
                            if !matches!(updated, Ok(true)) {
                                break;
                            }
                        }
                    }));
                }
                Err(error) => self.start_error = Some(error),
            }
        }
        self.update_overlay(cx);
        cx.notify();
    }

    fn update_overlay(&mut self, cx: &App) {
        let config = self.settings.overlay;
        if config.enabled
            && self.monitor.is_some()
            && let Some(snapshot) = &self.snapshot
            && !matches!(snapshot.phase, Phase::Failed(_))
        {
            let colors = overlay_colors(&snapshot.samples, self.run_settings, cx.theme().is_dark());
            self.overlay_error = self.overlay.update(config, &colors).err();
        } else {
            self.overlay.hide();
            self.overlay_error = None;
        }
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
        grid: &Grid,
        height: f32,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let Grid {
            cols, rows, width, ..
        } = *grid;
        let last_width = grid.row_width(count - rows.saturating_sub(1) * cols);
        let step = px(grid.row_step());
        let label_height = px(grid.label_height);
        let overflow = height < grid.full_height();
        let height = px(height);
        let muted = cx.theme().muted_foreground;
        let ring = cx.theme().ring;
        let dark = cx.theme().is_dark();
        let view = cx.weak_entity();
        let scroll = self.history.clone();
        let tile_bounds = Rc::clone(&self.tile_bounds);
        tile_bounds.borrow_mut().resize(count, Bounds::default());
        // Start at the newest tail and follow it through reflow; respect a user who scrolls back.
        if scroll.offset().y <= -scroll.max_offset().y + px(1.) {
            scroll.scroll_to_bottom();
        }
        div()
            .relative()
            .h(height)
            .flex_shrink_0()
            .child(
                div()
                    // A reflow invalidates the tooltip's original hitbox.
                    .id(format!(
                        "history-scroll-{}-{count}-{}",
                        width.to_bits(),
                        f32::from(height).to_bits()
                    ))
                    .size_full()
                    .pr(px(SCROLLBAR_ROOM))
                    .track_scroll(&scroll)
                    .overflow_y_scroll()
                    .tooltip(move |_, cx| cx.new(|_| ProbeTooltip { view: view.clone() }).into())
                    .flex()
                    .flex_col()
                    .gap(px(grid.row_gap))
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
                            "pageup" => height - step,
                            "pagedown" => step - height,
                            "up" => step,
                            "down" => -step,
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
                    .child(
                        Label::new("Oldest")
                            .text_xs()
                            .h(label_height)
                            .whitespace_nowrap()
                            .flex_shrink_0()
                            .text_color(muted),
                    )
                    .children((0..rows).map(|row| {
                        let row_bounds = Rc::clone(&tile_bounds);
                        div()
                            .on_children_prepainted(move |bounds, window, _| {
                                let mask = window.content_mask().bounds;
                                for (column, bounds) in bounds.into_iter().enumerate() {
                                    if let Some(tile) =
                                        row_bounds.borrow_mut().get_mut(row * cols + column)
                                    {
                                        *tile = bounds.intersect(&mask);
                                    }
                                }
                            })
                            .flex()
                            .flex_shrink_0()
                            .gap(px(grid.tile_gap))
                            .children((row * cols..((row + 1) * cols).min(count)).map(|index| {
                                let sample = samples.get(index).unwrap_or(&Sample::Empty);
                                let mark = Mark::for_sample(sample, cutoffs);
                                let detail = probe_detail(sample);
                                let position = if index + 1 == count {
                                    "Newest slot"
                                } else if index == 0 {
                                    "Oldest slot"
                                } else {
                                    "Slot"
                                };
                                tile(mark, dark, grid.tile_width(index % cols), grid.tile_height)
                                    .id(("sample", index))
                                    .role(Role::Label)
                                    .aria_label(format!("{position} {}, {detail}", index + 1))
                            }))
                    }))
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .flex_shrink_0()
                            .w(px(last_width.max(55.).min(width)))
                            .h(label_height)
                            .child(
                                Label::new("Newest")
                                    .text_xs()
                                    .whitespace_nowrap()
                                    .text_color(muted),
                            ),
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

impl Drop for MonitorView {
    fn drop(&mut self) {
        self.overlay.hide();
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
        let streak = snapshot.map_or_else(|| DASH.into(), |s| s.success_streak.to_string());
        let link = snapshot.and_then(|s| s.link_speed.as_ref());
        let peak = snapshot.and_then(|s| s.peak_bps);
        let grid = monitor_grid(count, window);
        let width = grid.width;
        let grid_height = grid
            .viewport_height(f32::from(window.viewport_size().height) - self.chrome_height.get());
        let scale = window.scale_factor();
        let block_padding = window.rem_size();
        let chrome_height = Rc::clone(&self.chrome_height);
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
            .child(self.history_grid(samples, count, cutoffs, &grid, grid_height, window, cx))
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
                    ))
                    .child(metric("Streak", streak, muted)),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_baseline()
                    .gap_x_5()
                    .gap_y_1()
                    .child(link_metric(link, muted))
                    .child(peak_metric(peak, muted)),
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
                    .py(block_padding)
                    .on_children_prepainted(move |bounds, window, _| {
                        // The real layout includes toolbar/legend wrapping and any
                        // notices (including a wrapped overlay error). Subtract only
                        // the known history viewport; the rest is measured chrome.
                        if let (Some(first), Some(last)) = (bounds.first(), bounds.last()) {
                            let measured = snap(
                                f32::from(last.bottom() - first.top())
                                    + 2. * snap(f32::from(block_padding), scale)
                                    - grid_height,
                                scale,
                            );
                            if chrome_height.replace(measured) != measured {
                                window.request_animation_frame();
                            }
                        }
                    })
                    .child(toolbar)
                    .when_some(self.window_error.clone(), |el, error| {
                        el.child(
                            div()
                                .id("window-error")
                                .role(Role::Alert)
                                .child(Label::new(error)),
                        )
                    })
                    .when_some(self.overlay_error.as_deref(), |el, error| {
                        el.child(
                            div().id("overlay-error").role(Role::Alert).child(
                                Label::new(format!(
                                    "Ping strip unavailable: {error} You can change or turn off the strip in Settings."
                                ))
                                .text_sm()
                                .text_color(muted),
                            ),
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

fn link_metric(link: Option<&LinkSpeed>, muted: Hsla) -> Div {
    let kind = link.map(|link| match link.kind {
        LinkKind::Ethernet => ("Ethernet", Some(assets::IconName::EthernetPort)),
        LinkKind::Wifi => ("Wi-Fi", Some(assets::IconName::Wifi)),
        LinkKind::Other => ("Other adapter", None),
    });
    let rx = link.and_then(|link| link.rx_bps);
    let tx = link.and_then(|link| link.tx_bps);
    let reported = rx.is_some() || tx.is_some();
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .min_w_0()
        .gap_x_2()
        .gap_y_1()
        .child(Label::new("Link Speed").text_color(muted))
        .when_some(kind, |el, (name, icon)| {
            el.child(
                div()
                    .flex()
                    .items_center()
                    .flex_shrink_0()
                    .gap_1()
                    .when_some(icon, |el, icon| {
                        el.child(Icon::new(icon).size(px(14.)).text_color(muted))
                    })
                    .child(Label::new(name)),
            )
        })
        .when(reported, |el| {
            el.child(metric("Rx", link_rate(rx), muted).flex_shrink_0())
                .child(metric("Tx", link_rate(tx), muted).flex_shrink_0())
        })
        .when(!reported, |el| {
            el.child(Label::new(UNAVAILABLE).font_semibold())
        })
}

fn peak_metric(peak: Option<(u64, u64)>, muted: Hsla) -> impl IntoElement {
    const TITLE: &str = "Max. Speed captured";
    const HELP: &str =
        "Passive peak adapter throughput this session, not a speed test or target throughput.";
    div()
        .id("peak-speed")
        .role(Role::Group)
        .aria_label(TITLE)
        .aria_description(HELP)
        .tooltip(|window, cx| Tooltip::new(HELP).max_w(px(280.)).build(window, cx))
        .flex()
        .flex_wrap()
        .items_center()
        .min_w_0()
        .gap_x_2()
        .gap_y_1()
        .child(Label::new(TITLE).text_color(muted).flex_shrink_0())
        .child(metric("Rx", link_rate(peak.map(|(rx, _)| rx)), muted).flex_shrink_0())
        .child(metric("Tx", link_rate(peak.map(|(_, tx)| tx)), muted).flex_shrink_0())
}

fn link_rate(bps: Option<u64>) -> String {
    let Some(bps) = bps else {
        return UNAVAILABLE.into();
    };
    let (divisor, unit) = if bps >= 1_000_000_000 {
        (1_000_000_000., "Gbps")
    } else {
        (1_000_000., "Mbps")
    };
    if (1..10_000).contains(&bps) {
        return "<0.01 Mbps".into();
    }
    let rate = format!("{:.2}", bps as f64 / divisor);
    format!(
        "{} {unit}",
        rate.trim_end_matches('0').trim_end_matches('.')
    )
}

struct ProbeTooltip {
    view: WeakEntity<MonitorView>,
}

impl Render for ProbeTooltip {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let text = self.view.upgrade().and_then(|view| {
            let view = view.read(cx);
            // One tooltip owner for the grid: retained per-tile requests must never compete.
            // Recompute the current clipped tile under the pointer after scrolling or reflow.
            let index = tile_at(&view.tile_bounds.borrow(), window.mouse_position())?;
            let sample = view
                .snapshot
                .as_ref()
                .and_then(|snap| snap.samples.get(index))
                .unwrap_or(&Sample::Empty);
            Some(probe_detail(sample))
        });
        div().when_some(text, |el, text| {
            el.child(Tooltip::new(text).build(window, cx))
        })
    }
}

fn tile_at(tiles: &[Bounds<Pixels>], pointer: Point<Pixels>) -> Option<usize> {
    tiles.iter().position(|bounds| {
        bounds.size.width > px(0.) && bounds.size.height > px(0.) && bounds.contains(&pointer)
    })
}

fn probe_detail(sample: &Sample) -> String {
    match sample {
        Sample::Reply(ms) => format!("Reply, {ms} ms"),
        Sample::Error(reason) => format!("Error/Timeout: {reason}"),
        Sample::LocalError(reason) => format!("Local error: {reason}"),
        Sample::Pending => "Pending".into(),
        Sample::Skipped => "Skipped".into(),
        Sample::Empty => "Not sampled".into(),
    }
}

fn rtt(ms: Option<u32>) -> String {
    ms.map_or_else(|| DASH.into(), |ms| format!("{ms} ms"))
}

#[cfg(test)]
mod tests {
    use super::{Sample, Settings, link_rate, overlay_colors, probe_detail, tile_at};
    use gpui_kit::{bounds, point, px, size};

    #[test]
    fn tooltip_uses_current_clipped_tile_geometry_after_scroll_and_resize() {
        let point_at = |x, y| point(px(x), px(y));
        let rect = |x, y, w, h| bounds(point_at(x, y), size(px(w), px(h)));
        let mask = rect(0., 0., 110., 40.);
        let tiles = [rect(0., -40., 24., 34.), rect(0., 0., 24., 34.)];
        let clipped = tiles.map(|tile| tile.intersect(&mask));
        assert_eq!(tile_at(&clipped, point_at(12., 10.)), Some(1));
        assert_eq!(tile_at(&clipped, point_at(30., 10.)), None);
        assert_eq!(tile_at(&clipped, point_at(12., -10.)), None);

        let wider = [rect(0., 0., 48., 34.), rect(52., 0., 48., 34.)];
        assert_eq!(tile_at(&wider, point_at(30., 10.)), Some(0));
        assert_eq!(tile_at(&wider, point_at(50., 10.)), None);
        assert_eq!(tile_at(&wider, point_at(56., 10.)), Some(1));
    }

    #[test]
    fn strip_uses_history_order_and_active_run_cutoffs_in_rgb() {
        let settings = Settings {
            fast_ms: 10,
            slow_ms: 20,
            ..Settings::default()
        };
        let samples = [
            Sample::Empty,
            Sample::Reply(9),
            Sample::Reply(10),
            Sample::Reply(20),
            Sample::Error("Timed out".into()),
            Sample::Pending,
            Sample::Skipped,
        ];
        assert_eq!(
            overlay_colors(&samples, settings, false),
            [
                0x7b8897, 0x17632f, 0x8d6800, 0xa44810, 0xb52e36, 0x466e9e, 0x61758d
            ]
        );
    }

    #[test]
    fn monitor_labels_preserve_probe_outcomes_and_link_rates() {
        for (sample, expected) in [
            (Sample::Reply(0), "Reply, 0 ms"),
            (
                Sample::Error("Timed out".into()),
                "Error/Timeout: Timed out",
            ),
            (
                Sample::LocalError("ICMP unavailable".into()),
                "Local error: ICMP unavailable",
            ),
            (Sample::Pending, "Pending"),
            (Sample::Skipped, "Skipped"),
            (Sample::Empty, "Not sampled"),
        ] {
            assert_eq!(probe_detail(&sample), expected);
        }
        for (bps, expected) in [
            (None, "Unavailable"),
            (Some(0), "0 Mbps"),
            (Some(5_000), "<0.01 Mbps"),
            (Some(144_400_000), "144.4 Mbps"),
            (Some(1_000_000_000), "1 Gbps"),
            (Some(2_500_000_000), "2.5 Gbps"),
        ] {
            assert_eq!(link_rate(bps), expected);
        }
    }
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
}

// (Fill, ink, outline), as 0xRRGGBB. The strip uses the higher-contrast outline.
fn mark_colors(mark: Mark, dark: bool) -> (u32, u32, u32) {
    match (mark, dark) {
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
    }
}

fn overlay_colors(samples: &[Sample], run_settings: Settings, dark: bool) -> Vec<u32> {
    samples
        .iter()
        .map(|sample| mark_colors(Mark::for_sample(sample, run_settings), dark).2)
        .collect()
}

fn tile(mark: Mark, dark: bool, width: f32, height: f32) -> Div {
    let (bg, ink, edge) = mark_colors(mark, dark);
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
