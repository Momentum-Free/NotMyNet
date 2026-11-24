use notmynet_windows::Sample;

const TILE_MIN: f32 = 22.;
const TILE_GAP: f32 = 4.;
const TILE_HEIGHT: f32 = 34.;
const ROW_GAP: f32 = 6.;

pub fn snap(length: f32, scale: f32) -> f32 {
    // GPUI's Taffy length conversion rounds exact half pixels toward zero.
    let physical = length * scale;
    (physical.abs() - 0.5).ceil().copysign(physical) / scale
}

pub struct Grid {
    pub cols: usize,
    pub rows: usize,
    pub width: f32,
    pub tile_gap: f32,
    pub tile_height: f32,
    pub row_gap: f32,
    pub label_height: f32,
    scale: f32,
}

impl Grid {
    pub fn new(width: f32, count: usize, scale: f32, label_height: f32) -> Self {
        let width_px = (width * scale).round().max(1.);
        let gap_px = (snap(TILE_GAP, scale) * scale).round();
        let min_px = (snap(TILE_MIN, scale) * scale).round();
        let cols = (((width_px + gap_px) / (min_px + gap_px)).floor() as usize).clamp(1, 30);
        Self {
            cols,
            rows: count.div_ceil(cols),
            width: width_px / scale,
            tile_gap: gap_px / scale,
            tile_height: snap(TILE_HEIGHT, scale),
            row_gap: snap(ROW_GAP, scale),
            label_height: snap(label_height, scale),
            scale,
        }
    }

    pub fn row_step(&self) -> f32 {
        self.tile_height + self.row_gap
    }

    fn row_height(&self, rows: usize) -> f32 {
        snap(
            rows as f32 * self.row_step() + 2. * self.label_height + self.row_gap,
            self.scale,
        )
    }

    pub fn full_height(&self) -> f32 {
        self.row_height(self.rows)
    }

    pub fn viewport_height(&self, available: f32) -> f32 {
        // Leave at least one row usable; the page scrolls if even that doesn't fit.
        snap(available, self.scale).clamp(self.row_height(self.rows.min(1)), self.full_height())
    }

    fn tile_edge(&self, columns: usize) -> f32 {
        // Distribute fractional device pixels across the row instead of rounding
        // each tile independently (which can put the last tile under the scrollbar).
        let tile_space = (self.width - (self.cols - 1) as f32 * self.tile_gap) * self.scale;
        (tile_space * columns as f32 / self.cols as f32).round() / self.scale
    }

    pub fn tile_width(&self, column: usize) -> f32 {
        self.tile_edge(column + 1) - self.tile_edge(column)
    }

    pub fn row_width(&self, columns: usize) -> f32 {
        self.tile_edge(columns) + columns.saturating_sub(1) as f32 * self.tile_gap
    }
}

pub fn launch_height(chrome: f32, grid: &Grid, minimum: f32, work_height: f32) -> f32 {
    (chrome + grid.full_height()).max(minimum).min(work_height)
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
fn saved_rows_fit_until_the_work_area_caps_launch_height() {
    let short = Grid::new(864., 20, 1., 20.);
    let four_rows = Grid::new(864., 120, 1., 20.);
    assert_eq!((short.rows, four_rows.rows), (1, 4));
    let chrome = 268.; // Example measured GPUI toolbar, legends and status at launch width.
    let short_height = launch_height(chrome, &short, 360., 720.);
    assert_eq!(short_height, 360.);
    assert_eq!(
        short.viewport_height(short_height - chrome),
        short.full_height()
    );
    let four_height = launch_height(chrome, &four_rows, 360., 720.);
    assert_eq!(four_height, 474.);
    assert_eq!(
        four_rows.viewport_height(four_height - chrome),
        four_rows.full_height()
    );

    // At a fixed viewport height, a wrapped optional alert consumes space
    // that was available for history; the window itself doesn't resize.
    assert!(four_rows.viewport_height(four_height - (chrome + 56.)) < four_rows.full_height());

    // At narrower widths the toolbar/legend can wrap (measured chrome), and
    // the same saved count uses more rows.
    let narrow = Grid::new(334., 120, 1., 20.);
    assert_eq!(narrow.rows, 10);
    for chrome in [308., 364.] {
        let height = launch_height(chrome, &narrow, 360., 450.);
        assert_eq!(height, 450.);
        assert!(narrow.viewport_height(height - chrome) < narrow.full_height());
        assert_eq!(narrow.viewport_height(height - chrome), height - chrome);
    }
    assert_eq!(launch_height(364., &narrow, 360., 330.), 330.);
    assert_eq!(narrow.viewport_height(330. - 364.), short.full_height());
}

#[cfg(test)]
#[test]
fn stretched_tiles_use_the_same_row_geometry_at_fractional_dpi() {
    assert_eq!(
        [500., 775., 776., 1200.].map(|width| Grid::new(width, 120, 1., 20.).cols),
        [19, 29, 30, 30]
    );
    let standard = Grid::new(864., 120, 1., 20.);
    let wide = Grid::new(1539., 120, 1.25, 20.);
    assert_eq!(standard.cols, wide.cols);
    assert_eq!(wide.tile_height * 1.25, 42.);
    assert_eq!(wide.row_gap * 1.25, 7.);
    assert_eq!(wide.viewport_height(wide.full_height()), wide.full_height());
    assert!(wide.tile_width(0) > 36. && wide.tile_width(0) > standard.tile_width(0));
    for n in [1, 17, wide.cols] {
        let tile_edges = (0..n).map(|col| wide.tile_width(col)).sum::<f32>();
        assert!((tile_edges + (n - 1) as f32 * wide.tile_gap - wide.row_width(n)).abs() < 0.01);
    }
    assert!((wide.row_width(wide.cols) - wide.width).abs() < 0.01);
}

#[cfg(test)]
#[test]
fn reply_only_metrics() {
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
