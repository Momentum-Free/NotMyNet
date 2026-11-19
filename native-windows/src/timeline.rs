use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    Resolving,
    Running,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sample {
    Empty,
    Pending,
    Skipped,
    Reply(u32),
    Error(String),
    LocalError(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkKind {
    Ethernet,
    Wifi,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinkSpeed {
    pub rx_bps: Option<u64>,
    pub tx_bps: Option<u64>,
    pub kind: LinkKind,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub phase: Phase,
    pub samples: Vec<Sample>,
    pub session_elapsed: Duration,
    pub outage_elapsed: Option<Duration>,
    pub last_reply_ms: Option<u32>,
    pub total_sent: u64,
    pub total_replies: u64,
    pub success_streak: u64,
    pub link_speed: Option<LinkSpeed>,
}

pub fn format_duration(duration: Duration) -> String {
    let total = duration.as_secs();
    let (d, h, m, s) = (
        total / 86_400,
        total / 3600 % 24,
        total / 60 % 60,
        total % 60,
    );
    if d > 0 {
        format!("{d:02}d:{h:02}h:{m:02}m:{s:02}s")
    } else if h > 0 {
        format!("{h:02}h:{m:02}m:{s:02}s")
    } else if m > 0 {
        format!("{m:02}m:{s:02}s")
    } else {
        format!("{s:02}s")
    }
}

#[cfg(any(windows, test))]
mod engine {
    use super::*;
    use std::collections::{BTreeMap, VecDeque};

    pub(crate) struct State {
        phase: Phase,
        interval_ms: u32,
        next: u64,
        history: VecDeque<Sample>,
        // For each outstanding slot, remember the first confirmed failure after it. If it
        // comes back successfully, that later failure is the new start of any ongoing outage.
        pending: BTreeMap<u64, Option<u64>>,
        first_failed: Option<u64>,
        last_reply: Option<(u64, u32)>,
        sent: u64,
        replies: u64,
        // Merge contiguous reply ranges; only unresolved gaps after the last break need separate ranges.
        streak_ranges: BTreeMap<u64, u64>,
        last_break: Option<u64>,
    }

    impl State {
        pub(crate) fn new(settings: crate::Settings) -> Self {
            Self {
                phase: Phase::Resolving,
                interval_ms: settings.interval_ms,
                next: 0,
                history: VecDeque::from(vec![Sample::Empty; settings.display_count]),
                pending: BTreeMap::new(),
                first_failed: None,
                last_reply: None,
                sent: 0,
                replies: 0,
                streak_ranges: BTreeMap::new(),
                last_break: None,
            }
        }

        pub(crate) fn set_phase(&mut self, phase: Phase) {
            self.phase = phase;
        }
        fn time(&self, id: u64) -> Duration {
            Duration::from_millis(id.saturating_mul(self.interval_ms.into()))
        }
        pub(crate) fn next_tick(&self) -> Duration {
            self.time(self.next)
        }
        pub(crate) fn send_before(&self, id: u64) -> Duration {
            self.time(id) + Duration::from_millis(self.interval_ms.into()) / 2
        }

        /// The latest due slot may send if still near its scheduled boundary. Never fill a gap
        /// with packets: overdue or saturated slots remain in the display as skipped.
        pub(crate) fn tick(&mut self, now: Duration, permit: bool) -> Option<u64> {
            let due = (now.as_millis() / u128::from(self.interval_ms)) as u64;
            if due < self.next {
                return None;
            }
            let send = permit && now < self.send_before(due);
            if !send {
                self.break_streak(due);
            } else if due > self.next {
                self.break_streak(due - 1);
            }
            let skipped = (due - self.next).min(self.history.len() as u64 - 1);
            for _ in 0..skipped {
                self.history.pop_front();
                self.history.push_back(Sample::Skipped);
            }
            self.history.pop_front();
            self.history.push_back(if send {
                Sample::Pending
            } else {
                Sample::Skipped
            });
            self.next = due + 1;
            if send {
                self.pending.insert(due, None);
            }
            send.then_some(due)
        }

        pub(crate) fn complete(&mut self, id: u64, sample: Sample) {
            let Some(failure_after) = self.pending.remove(&id) else {
                return;
            };
            if let Sample::Reply(ms) = sample {
                self.extend_streak(id);
                self.sent += 1;
                self.replies += 1;
                if self.last_reply.is_none_or(|(last, _)| id > last) {
                    self.last_reply = Some((id, ms));
                    self.first_failed = failure_after;
                }
            } else if let Sample::Error(_) = sample {
                self.sent += 1;
                if self.last_reply.is_none_or(|(last, _)| id > last) {
                    self.first_failed = Some(self.first_failed.map_or(id, |f| f.min(id)));
                    for (_, earliest) in self.pending.range_mut(..id) {
                        *earliest = Some(earliest.map_or(id, |f| f.min(id)));
                    }
                }
            }
            if !matches!(sample, Sample::Reply(_)) {
                self.break_streak(id);
            }
            let offset = self.next - id;
            if offset <= self.history.len() as u64 {
                let index = self.history.len() - offset as usize;
                self.history[index] = sample;
            }
        }

        fn break_streak(&mut self, id: u64) {
            if self.last_break.is_none_or(|last| id > last) {
                self.last_break = Some(id);
                self.streak_ranges = self.streak_ranges.split_off(&(id + 1));
            }
        }

        fn extend_streak(&mut self, id: u64) {
            if self.last_break.is_some_and(|last| id <= last) {
                return;
            }
            let start = if let Some((&start, &end)) = self.streak_ranges.range(..id).next_back()
                && end + 1 == id
            {
                self.streak_ranges.remove(&start);
                start
            } else {
                id
            };
            let end = self.streak_ranges.remove(&(id + 1)).unwrap_or(id);
            self.streak_ranges.insert(start, end);
        }

        pub(crate) fn snapshot(&self, now: Duration) -> Snapshot {
            Snapshot {
                phase: self.phase.clone(),
                samples: self.history.iter().cloned().collect(),
                session_elapsed: now,
                outage_elapsed: self
                    .first_failed
                    .map(|id| now.saturating_sub(self.time(id))),
                last_reply_ms: self.last_reply.map(|(_, ms)| ms),
                total_sent: self.sent,
                total_replies: self.replies,
                success_streak: self
                    .streak_ranges
                    .last_key_value()
                    .map_or(0, |(start, end)| end - start + 1),
                link_speed: None,
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::Settings;
        const fn ms(n: u64) -> Duration {
            Duration::from_millis(n)
        }
        fn error() -> Sample {
            Sample::Error("Reply timed out".into())
        }

        #[test]
        fn fixed_grid_overlaps_preserves_order_and_never_replays_gaps() {
            let mut state = State::new(Settings {
                interval_ms: 50,
                timeout_ms: 160,
                display_count: 4,
                ..Settings::default()
            });
            state.set_phase(Phase::Running);
            assert_eq!(state.tick(ms(0), true), Some(0));
            assert_eq!(state.next_tick(), ms(50));
            assert_eq!(state.send_before(0), ms(25));
            assert_eq!(state.tick(ms(50), true), Some(1));
            state.complete(1, Sample::Reply(0));
            assert_eq!(state.snapshot(ms(55)).total_replies, 1);
            assert_eq!(state.tick(ms(150), true), Some(3));
            assert_eq!(
                state.snapshot(ms(151)).samples,
                [
                    Sample::Pending,
                    Sample::Reply(0),
                    Sample::Skipped,
                    Sample::Pending
                ]
            );
            state.complete(0, Sample::Reply(123)); // Late completion, still a valid OS reply.
            assert_eq!(state.snapshot(ms(180)).last_reply_ms, Some(0));
            assert_eq!(state.snapshot(ms(180)).total_replies, 2);
            assert_eq!(state.tick(ms(249), true), None);
            assert_eq!(state.tick(ms(250), true), Some(5));
        }

        #[test]
        fn recovery_is_immediate_and_a_late_failure_cannot_reopen_it() {
            let mut state = State::new(Settings {
                display_count: 2,
                ..Settings::default()
            });
            state.tick(ms(0), true);
            state.complete(0, error());
            state.tick(ms(1000), true);
            state.tick(ms(2000), true);
            assert_eq!(state.snapshot(ms(2045)).outage_elapsed, Some(ms(2045)));
            state.complete(2, Sample::Reply(3));
            state.complete(1, error());
            assert_eq!(state.snapshot(ms(2050)).outage_elapsed, None);
            assert_eq!(state.snapshot(ms(2050)).total_sent, 3);
            assert_eq!(state.tick(ms(86_400_000), false), None);
            assert_eq!(
                state.snapshot(ms(86_400_000)).samples,
                [Sample::Skipped, Sample::Skipped]
            );
        }

        #[test]
        fn streak_survives_ring_wrap_and_handles_out_of_order_results() {
            let mut state = State::new(Settings {
                display_count: 1,
                ..Settings::default()
            });
            for slot in 0..4 {
                state.tick(ms(slot * 1000), true);
            }
            state.complete(3, Sample::Reply(2));
            state.complete(1, Sample::Reply(0));
            assert_eq!(state.snapshot(ms(3050)).success_streak, 1);
            state.complete(2, Sample::Reply(7));
            state.complete(0, Sample::Reply(8));
            assert_eq!(state.snapshot(ms(3100)).success_streak, 4);
            state.tick(ms(4000), true);
            state.tick(ms(5000), true);
            state.complete(5, error());
            state.complete(4, Sample::Reply(7));
            assert_eq!(state.snapshot(ms(5100)).success_streak, 0);
            state.tick(ms(6000), true);
            state.complete(6, Sample::Reply(7));
            assert_eq!(state.snapshot(ms(6100)).success_streak, 1);
            state.tick(ms(7000), false);
            assert_eq!(state.snapshot(ms(7100)).success_streak, 0);
        }

        #[test]
        fn intermediate_reply_preserves_newer_failure_after_ring_wrap() {
            let mut state = State::new(Settings {
                display_count: 1,
                ..Settings::default()
            });
            state.tick(ms(0), true);
            state.complete(0, error());
            state.tick(ms(1_000), true);
            state.tick(ms(2_000), true);
            state.complete(2, error());
            state.tick(ms(3_000), true);
            state.complete(3, Sample::LocalError("local".into()));
            state.complete(1, Sample::Reply(5));
            assert_eq!(state.snapshot(ms(3200)).outage_elapsed, Some(ms(1200)));
            assert_eq!(state.snapshot(ms(3200)).total_sent, 3);
            assert_eq!(
                state.snapshot(ms(3200)).samples,
                [Sample::LocalError("local".into())]
            );
        }
    }
}

#[cfg(windows)]
pub(crate) use engine::State;

#[cfg(test)]
#[test]
fn duration_hides_only_leading_units() {
    for (seconds, expected) in [
        (0, "00s"),
        (1003, "16m:43s"),
        (86_399, "23h:59m:59s"),
        (86_400, "01d:00h:00m:00s"),
        (86_401, "01d:00h:00m:01s"),
        (8_640_000, "100d:00h:00m:00s"),
    ] {
        assert_eq!(format_duration(Duration::from_secs(seconds)), expected);
    }
}
