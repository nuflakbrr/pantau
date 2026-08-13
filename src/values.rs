/// Unit-aware formatting for sensor readings. Each variant owns its own
/// format function rather than one generic formatter — sensor categories
/// have incompatible display rules (percent vs hertz vs runtime breakdown).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SizeUnit {
    Binary,
    Decimal,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FormatKind {
    Percent,
    Hertz,
    Memory(SizeUnit),
    Runtime,
    Load,
}

/// `use-higher-precision` setting: false = fewer decimals, true = more.
#[derive(Debug, Clone, Copy)]
pub struct FormatOptions {
    pub higher_precision: bool,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self {
            higher_precision: false,
        }
    }
}

impl FormatKind {
    pub fn format(&self, value: f64, opts: FormatOptions) -> String {
        match self {
            FormatKind::Percent => {
                let dp = if opts.higher_precision { 1 } else { 0 };
                // Width-padded so "5%" and "100%" take the same on-screen
                // space — without this, the status bar button (and any
                // fixed-width container built from it) visibly resizes
                // every refresh tick as the digit count changes.
                format!("{value:>3.dp$}%", dp = dp)
            }
            FormatKind::Hertz => format_scaled(value, 1000.0, &["Hz", "kHz", "MHz", "GHz", "THz"], opts),
            FormatKind::Memory(unit) => {
                let (base, labels): (f64, &[&str]) = match unit {
                    SizeUnit::Binary => (1024.0, &["B", "KiB", "MiB", "GiB", "TiB"]),
                    SizeUnit::Decimal => (1000.0, &["B", "KB", "MB", "GB", "TB"]),
                };
                format_scaled(value, base, labels, opts)
            }
            FormatKind::Runtime => format_runtime(value),
            FormatKind::Load => {
                let dp = if opts.higher_precision { 2 } else { 1 };
                format!("{value:.dp$}", dp = dp)
            }
        }
    }
}

fn format_scaled(mut value: f64, base: f64, labels: &[&str], opts: FormatOptions) -> String {
    let mut idx = 0;
    while value.abs() >= base && idx < labels.len() - 1 {
        value /= base;
        idx += 1;
    }
    let dp = if idx == 0 {
        0
    } else if opts.higher_precision {
        2
    } else {
        1
    };
    format!("{value:.dp$} {}", labels[idx], dp = dp)
}

/// Seconds -> "1d 02:03:04" style breakdown.
fn format_runtime(total_seconds: f64) -> String {
    let secs = total_seconds.max(0.0) as u64;
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;
    if days > 0 {
        format!("{days}d {hours:02}:{minutes:02}:{seconds:02}")
    } else {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    }
}

/// Half-open band threshold color matching: bands sorted ascending,
/// value >= threshold picks the highest matching band's color.
#[derive(Debug, Clone, Copy)]
pub struct ColorThreshold {
    pub threshold: f64,
    pub rgba: (u8, u8, u8, u8),
}

const GREEN: (u8, u8, u8, u8) = (52, 199, 89, 255);
const YELLOW: (u8, u8, u8, u8) = (255, 204, 0, 255);
const RED: (u8, u8, u8, u8) = (255, 59, 48, 255);

/// Default severity bands — hardcoded for now since there's no settings UI
/// yet to make these configurable (that's Phase 7 scope); reasonable
/// defaults in the spirit of vitals-gnome's ascending-threshold model,
/// not the user's own tuned values.
pub const PERCENT_BANDS: [ColorThreshold; 3] = [
    ColorThreshold { threshold: 0.0, rgba: GREEN },
    ColorThreshold { threshold: 60.0, rgba: YELLOW },
    ColorThreshold { threshold: 80.0, rgba: RED },
];

pub const TEMPERATURE_BANDS: [ColorThreshold; 3] = [
    ColorThreshold { threshold: 0.0, rgba: GREEN },
    ColorThreshold { threshold: 70.0, rgba: YELLOW },
    ColorThreshold { threshold: 85.0, rgba: RED },
];

/// Renders a resolved severity color as a colored-dot prefix. NSMenuItem's
/// per-item foreground color fights the system's own selection-highlight
/// tinting in AppKit, so a Unicode dot is a more reliable signal than
/// `NSAttributedString` coloring across light/dark mode and hover states.
pub fn severity_dot(rgba: (u8, u8, u8, u8)) -> &'static str {
    match rgba {
        GREEN => "\u{1F7E2} ",
        YELLOW => "\u{1F7E1} ",
        RED => "\u{1F534} ",
        _ => "",
    }
}

pub fn resolve_color(value: f64, bands: &[ColorThreshold]) -> Option<(u8, u8, u8, u8)> {
    let mut sorted: Vec<&ColorThreshold> = bands.iter().collect();
    sorted.sort_by(|a, b| a.threshold.partial_cmp(&b.threshold).unwrap());
    sorted
        .into_iter()
        .filter(|band| value >= band.threshold)
        .last()
        .map(|band| band.rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_format() {
        assert_eq!(FormatKind::Percent.format(42.0, FormatOptions::default()), " 42%");
        assert_eq!(FormatKind::Percent.format(100.0, FormatOptions::default()), "100%");
    }

    #[test]
    fn hertz_scales_up() {
        assert_eq!(
            FormatKind::Hertz.format(3_500_000_000.0, FormatOptions::default()),
            "3.5 GHz"
        );
    }

    #[test]
    fn memory_binary_scales() {
        assert_eq!(
            FormatKind::Memory(SizeUnit::Binary).format(1073741824.0, FormatOptions::default()),
            "1.0 GiB"
        );
    }

    #[test]
    fn runtime_breaks_down_days() {
        assert_eq!(format_runtime(90061.0), "1d 01:01:01");
    }

    #[test]
    fn runtime_under_a_day_has_no_day_prefix() {
        assert_eq!(format_runtime(3661.0), "01:01:01");
    }

    #[test]
    fn threshold_picks_highest_matching_band() {
        let bands = [
            ColorThreshold { threshold: 0.0, rgba: (0, 255, 0, 255) },
            ColorThreshold { threshold: 60.0, rgba: (255, 255, 0, 255) },
            ColorThreshold { threshold: 80.0, rgba: (255, 0, 0, 255) },
        ];
        assert_eq!(resolve_color(75.0, &bands), Some((255, 255, 0, 255)));
        assert_eq!(resolve_color(80.0, &bands), Some((255, 0, 0, 255)));
        assert_eq!(resolve_color(-1.0, &bands), None);
    }
}
