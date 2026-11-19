use std::time::Instant;

#[derive(Default)]
pub(crate) struct LinkPeak {
    previous: Option<(u64, Instant, (u64, u64))>,
    pub(crate) bps: Option<(u64, u64)>,
}

impl LinkPeak {
    pub(crate) fn observe(&mut self, interface: u64, at: Instant, octets: (u64, u64)) {
        if let Some((previous_if, previous_at, previous)) = self.previous {
            if at <= previous_at {
                return;
            }
            if previous_if != interface {
                self.bps = None;
            } else if let (Some(rx), Some(tx)) = (
                octets.0.checked_sub(previous.0),
                octets.1.checked_sub(previous.1),
            ) {
                let nanos = at.duration_since(previous_at).as_nanos();
                let rate = |bytes: u64| {
                    (u128::from(bytes) * 8_000_000_000 / nanos).min(u128::from(u64::MAX)) as u64
                };
                let (peak_rx, peak_tx) = self.bps.unwrap_or_default();
                self.bps = Some((peak_rx.max(rate(rx)), peak_tx.max(rate(tx))));
            }
        }
        self.previous = Some((interface, at, octets));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn peaks_use_elapsed_time_and_never_mix_interfaces_or_counter_resets() {
        let mut peak = LinkPeak::default();
        let start = Instant::now();
        let at = |ms| start + Duration::from_millis(ms);
        peak.observe(1, at(0), (10, 20));
        assert_eq!(peak.bps, None);
        peak.observe(1, at(2000), (10, 20));
        assert_eq!(peak.bps, Some((0, 0)));
        peak.observe(1, at(4500), (500_010, 1_000_020));
        assert_eq!(peak.bps, Some((1_600_000, 3_200_000)));
        peak.observe(1, at(6500), (1_500_010, 1_000_020));
        assert_eq!(peak.bps, Some((4_000_000, 3_200_000)));
        peak.observe(1, at(8500), (0, 2_000_020));
        peak.observe(1, at(8500), (u64::MAX, u64::MAX));
        assert_eq!(peak.bps, Some((4_000_000, 3_200_000)));
        peak.observe(2, at(10_500), (99_000_000, 88_000_000));
        assert_eq!(peak.bps, None);
        peak.observe(2, at(12_500), (99_000_001, 88_000_002));
        assert_eq!(peak.bps, Some((4, 8)));
    }
}
