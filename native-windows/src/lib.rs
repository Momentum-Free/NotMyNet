#[cfg(any(windows, test))]
mod link_peak;
#[cfg(windows)]
mod monitor;
mod settings;
mod timeline;

#[cfg(windows)]
pub use monitor::Monitor;
pub use settings::{Appearance, Settings};
pub use timeline::{LinkKind, LinkSpeed, Phase, Sample, Snapshot, format_duration};
