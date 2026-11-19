use notmynet_windows::Sample;

pub const TILE_MIN: f32 = 22.;
pub const TILE_GAP: f32 = 4.;

pub fn columns(width: f32) -> usize {
    (((width + TILE_GAP) / (TILE_MIN + TILE_GAP)).floor() as usize).clamp(1, 30)
}

#[derive(Default)]
pub struct Recent {
    pub replies: usize,
    pub errors: usize,
    pub local_errors: usize,
    pub pending: usize,
    pub skipped: usize,
    pub min: Option<u32>,
    pub worst: Option<u32>,
    sum: u64,
}

impl Recent {
    pub fn from_samples(samples: &[Sample]) -> Self {
        let mut recent = Self::default();
        for sample in samples {
            match sample {
                Sample::Reply(ms) => {
                    recent.replies += 1;
                    recent.sum += u64::from(*ms);
                    recent.min = Some(recent.min.map_or(*ms, |min| min.min(*ms)));
                    recent.worst = Some(recent.worst.map_or(*ms, |worst| worst.max(*ms)));
                }
                Sample::Error(_) => recent.errors += 1,
                Sample::LocalError(_) => recent.local_errors += 1,
                Sample::Pending => recent.pending += 1,
                Sample::Skipped => recent.skipped += 1,
                Sample::Empty => {}
            }
        }
        recent
    }

    pub fn avg(&self) -> Option<f64> {
        (self.replies > 0).then(|| self.sum as f64 / self.replies as f64)
    }

    pub fn loss(&self) -> Option<f64> {
        let finished = self.errors + self.replies;
        (finished > 0).then(|| 100. * self.errors as f64 / finished as f64)
    }
}

#[cfg(test)]
#[test]
fn history_fit_and_reply_only_metrics() {
    assert_eq!(
        [columns(500.), columns(775.), columns(776.), columns(1200.)],
        [19, 29, 30, 30]
    );
    let recent = Recent::from_samples(&[
        Sample::Empty,
        Sample::Pending,
        Sample::Skipped,
        Sample::Reply(0),
        Sample::Reply(25),
        Sample::Error("Timed out".into()),
        Sample::LocalError("ICMP unavailable".into()),
    ]);
    assert_eq!(
        (recent.pending, recent.skipped, recent.local_errors),
        (1, 1, 1)
    );
    assert_eq!(
        (recent.min, recent.avg(), recent.worst),
        (Some(0), Some(12.5), Some(25))
    );
    assert_eq!(recent.loss(), Some(100. / 3.));
    assert_eq!(Recent::from_samples(&[Sample::Pending]).loss(), None);
}
