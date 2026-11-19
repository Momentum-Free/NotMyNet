#[cfg(any(windows, test))]
mod link_peak;
#[cfg(windows)]
mod monitor;
#[cfg(windows)]
pub mod overlay;
mod overlay_config;
mod settings;
mod timeline;

#[cfg(windows)]
pub use monitor::Monitor;
pub use overlay_config::{OverlayEdge, OverlayMonitor, OverlaySettings};
pub use settings::{Appearance, Settings};
pub use timeline::{LinkKind, LinkSpeed, Phase, Sample, Snapshot, format_duration};
