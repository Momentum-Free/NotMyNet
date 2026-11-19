use crate::{OverlayEdge, OverlayMonitor, OverlaySettings};
use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static SAVE_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub interval_ms: u32,
    pub timeout_ms: u32,
    pub display_count: usize,
    pub fast_ms: u32,
    pub slow_ms: u32,
    pub appearance: Appearance,
    pub overlay: OverlaySettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            interval_ms: 1000,
            timeout_ms: 1000,
            display_count: 60,
            fast_ms: 20,
            slow_ms: 150,
            appearance: Appearance::System,
            overlay: OverlaySettings::default(),
        }
    }
}

impl Settings {
    pub fn validate(self) -> Result<Self, String> {
        if !(50..=60_000).contains(&self.interval_ms) {
            return Err("Probe interval must be 50–60,000 ms.".into());
        }
        if !(1..=5_000).contains(&self.timeout_ms) {
            return Err("Reply timeout must be 1–5,000 ms.".into());
        }
        if !(1..=600).contains(&self.display_count) {
            return Err("Display count must be 1–600.".into());
        }
        if self.fast_ms >= self.slow_ms {
            return Err("Fast RTT cutoff must be lower than Slow RTT cutoff.".into());
        }
        if !(1..=64).contains(&self.overlay.thickness_px) {
            return Err("Overlay thickness must be 1–64 px.".into());
        }
        Ok(self)
    }

    pub fn load() -> Self {
        Self::path()
            .and_then(fs::read_to_string)
            .ok()
            .as_deref()
            .and_then(Self::parse)
            .unwrap_or_default()
    }

    pub fn save(self) -> io::Result<()> {
        self.validate()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        let path = Self::path()?;
        fs::create_dir_all(path.parent().expect("settings directory"))?;
        let (staged, mut file) = loop {
            let staged = path.with_extension(format!(
                "{}.{}.tmp",
                std::process::id(),
                SAVE_ID.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::File::create_new(&staged) {
                Ok(file) => break (staged, file),
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(err) => return Err(err),
            }
        };
        let written = file.write_all(self.encode().as_bytes());
        drop(file);
        written
            .and_then(|()| fs::rename(&staged, &path))
            .inspect_err(|_| {
                let _ = fs::remove_file(&staged);
            })
    }

    fn path() -> io::Result<PathBuf> {
        let dir = std::env::var_os("LOCALAPPDATA")
            .filter(|v| !v.is_empty())
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "LOCALAPPDATA is not set"))?;
        Ok(PathBuf::from(dir).join("NotMyNet").join("settings.txt"))
    }

    fn encode(self) -> String {
        let Self {
            interval_ms,
            timeout_ms,
            display_count,
            fast_ms,
            slow_ms,
            appearance,
            overlay,
        } = self;
        format!(
            "notmynet-2\n{interval_ms} {timeout_ms} {display_count} {fast_ms} {slow_ms} {appearance:?}\n{} {} {:?} {} {}\n",
            overlay.enabled,
            overlay.monitor.encode(),
            overlay.edge,
            overlay.thickness_px,
            overlay.behind_taskbar
        )
    }

    fn parse(raw: &str) -> Option<Self> {
        let [
            version,
            interval,
            timeout,
            count,
            fast,
            slow,
            appearance,
            ref rest @ ..,
        ] = raw.split_whitespace().collect::<Vec<_>>()[..]
        else {
            return None;
        };
        let overlay = match (version, rest) {
            ("notmynet-1", []) => OverlaySettings::default(),
            ("notmynet-2", [enabled, monitor, edge, thickness, behind]) => OverlaySettings {
                enabled: enabled.parse().ok()?,
                monitor: OverlayMonitor::decode(monitor)?,
                edge: match *edge {
                    "Top" => OverlayEdge::Top,
                    "Bottom" => OverlayEdge::Bottom,
                    "Left" => OverlayEdge::Left,
                    "Right" => OverlayEdge::Right,
                    _ => return None,
                },
                thickness_px: thickness.parse().ok()?,
                behind_taskbar: behind.parse().ok()?,
            },
            _ => return None,
        };
        Self {
            overlay,
            interval_ms: interval.parse().ok()?,
            timeout_ms: timeout.parse().ok()?,
            display_count: count.parse().ok()?,
            fast_ms: fast.parse().ok()?,
            slow_ms: slow.parse().ok()?,
            appearance: match appearance {
                "System" => Appearance::System,
                "Light" => Appearance::Light,
                "Dark" => Appearance::Dark,
                _ => return None,
            },
        }
        .validate()
        .ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_and_validation() {
        let legacy = Settings::parse("notmynet-1\n1500 1200 80 30 140 Dark\n").unwrap();
        assert_eq!(
            (legacy.overlay, legacy.interval_ms, legacy.appearance),
            (OverlaySettings::default(), 1500, Appearance::Dark)
        );
        let overlay = OverlaySettings {
            enabled: true,
            edge: OverlayEdge::Left,
            monitor: OverlayMonitor::decode("004a00010023").unwrap(),
            thickness_px: 13,
            behind_taskbar: true,
        };
        for appearance in [Appearance::System, Appearance::Light, Appearance::Dark] {
            let settings = Settings {
                appearance,
                overlay,
                interval_ms: 50,
                timeout_ms: 5000,
                display_count: 600,
                ..Settings::default()
            };
            assert_eq!(Settings::parse(&settings.encode()), Some(settings));
        }
        for settings in [
            Settings {
                interval_ms: 49,
                ..Settings::default()
            },
            Settings {
                timeout_ms: 0,
                ..Settings::default()
            },
            Settings {
                display_count: 601,
                ..Settings::default()
            },
            Settings {
                fast_ms: 150,
                ..Settings::default()
            },
        ] {
            assert_eq!(Settings::parse(&settings.encode()), None);
        }
    }
}
