#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OverlayEdge {
    Top,
    #[default]
    Bottom,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OverlayMonitor(pub(crate) [u16; 32]);

impl OverlayMonitor {
    pub fn is_primary(self) -> bool {
        self == Self::default()
    }

    pub(crate) fn decode(value: &str) -> Option<Self> {
        if value == "-" {
            return Some(Self::default());
        }
        if value.is_empty()
            || value.len() > 124
            || !value.len().is_multiple_of(4)
            || !value.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return None;
        }
        let mut id = Self::default();
        for (i, offset) in (0..value.len()).step_by(4).enumerate() {
            let ch = u16::from_str_radix(&value[offset..offset + 4], 16).ok()?;
            if ch == 0 {
                return None;
            }
            id.0[i] = ch;
        }
        Some(id)
    }

    pub(crate) fn encode(self) -> String {
        if self.is_primary() {
            return "-".into();
        }
        self.0
            .iter()
            .take_while(|&&c| c != 0)
            .map(|c| format!("{c:04x}"))
            .collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OverlaySettings {
    pub enabled: bool,
    pub monitor: OverlayMonitor,
    pub edge: OverlayEdge,
    pub thickness_px: u32,
    pub behind_taskbar: bool,
}

impl Default for OverlaySettings {
    fn default() -> Self {
        Self {
            enabled: false,
            monitor: OverlayMonitor::default(),
            edge: OverlayEdge::Bottom,
            thickness_px: 6,
            behind_taskbar: false,
        }
    }
}

#[cfg(any(windows, test))]
pub(crate) fn strip_rect(
    mut area: [i32; 4],
    edge: OverlayEdge,
    thickness: u32,
) -> Option<[i32; 4]> {
    let [left, top, right, bottom] = area;
    if right <= left || bottom <= top || !(1..=64).contains(&thickness) {
        return None;
    }
    let t = thickness as i32;
    match edge {
        OverlayEdge::Top => area[3] = top.saturating_add(t).min(bottom),
        OverlayEdge::Bottom => area[1] = bottom.saturating_sub(t).max(top),
        OverlayEdge::Left => area[2] = left.saturating_add(t).min(right),
        OverlayEdge::Right => area[0] = right.saturating_sub(t).max(left),
    }
    Some(area)
}

#[cfg(any(windows, test))]
pub(crate) fn slice_boundary(index: usize, count: usize, length: i32) -> i32 {
    (index as u64 * length as u64 / count as u64) as i32
}

#[cfg(test)]
#[test]
fn strip_handles_negative_desktop_origins_and_fractional_slices() {
    use OverlayEdge::*;
    let r = [-1920, -100, 0, 940];
    assert_eq!(strip_rect(r, Top, 6), Some([-1920, -100, 0, -94]));
    assert_eq!(strip_rect(r, Bottom, 6), Some([-1920, 934, 0, 940]));
    assert_eq!(strip_rect(r, Left, 6), Some([-1920, -100, -1914, 940]));
    assert_eq!(strip_rect(r, Right, 6), Some([-6, -100, 0, 940]));
    let bounds: Vec<_> = (0..=60).map(|i| slice_boundary(i, 60, 391)).collect();
    assert_eq!((bounds[0], bounds[60]), (0, 391));
    assert!(bounds.windows(2).all(|w| (6..=7).contains(&(w[1] - w[0]))));
    assert_eq!(slice_boundary(600, 600, 200), 200);
}
