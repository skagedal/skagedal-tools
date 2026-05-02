//! Row coloring helpers for the TUI list view.

use ratatui::style::Color;

/// Six-color palette mirroring stern's pod-hash colors (the high-intensity
/// ANSI variants). FNV-32 of the value mod 6 picks the slot, matching
/// stern's `colorIndex` so the same pod gets the same color in both tools.
const POD_PALETTE: [Color; 6] = [
    Color::LightCyan,
    Color::LightGreen,
    Color::LightMagenta,
    Color::LightYellow,
    Color::LightBlue,
    Color::LightRed,
];

pub fn hash_color(value: &str) -> Color {
    POD_PALETTE[(fnv32(value.as_bytes()) as usize) % POD_PALETTE.len()]
}

fn fnv32(bytes: &[u8]) -> u32 {
    // FNV-1, 32-bit. Same seed/prime stern's `hash/fnv.New32` uses.
    let mut hash: u32 = 0x811c9dc5;
    for b in bytes {
        hash = hash.wrapping_mul(0x01000193);
        hash ^= u32::from(*b);
    }
    hash
}

/// Mirrors the old TS log-viewer's `levelColor`: classify a level string
/// into a foreground color. Returns `None` for unknown / empty levels.
pub fn level_color(level: &str) -> Option<Color> {
    let v = level.to_ascii_lowercase();
    if v.contains("error") || v == "err" || v.contains("fatal") {
        Some(Color::Red)
    } else if v.contains("warn") {
        Some(Color::Yellow)
    } else if v.contains("info") {
        Some(Color::Cyan)
    } else if v.contains("debug") || v.contains("trace") {
        Some(Color::Gray)
    } else {
        None
    }
}

/// True when the level should override a pod-hash row color — i.e., it's
/// an "alert me" level that we don't want visually drowned out by the
/// hash-based grouping.
pub fn level_overrides_pod_color(level: &str) -> bool {
    let v = level.to_ascii_lowercase();
    v.contains("error") || v == "err" || v.contains("fatal") || v.contains("warn")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv32_matches_stern_examples() {
        // Sanity: FNV-1 32-bit of "" is the offset basis.
        assert_eq!(fnv32(b""), 0x811c9dc5);
        // Different inputs should produce different hashes.
        assert_ne!(fnv32(b"pod-a"), fnv32(b"pod-b"));
    }

    #[test]
    fn hash_color_is_deterministic() {
        let a = hash_color("aira-internal-web-5dd6ff88ff-9k7xq");
        let b = hash_color("aira-internal-web-5dd6ff88ff-9k7xq");
        assert_eq!(a, b);
    }

    #[test]
    fn level_color_classifies_canonical_levels() {
        assert_eq!(level_color("ERROR"), Some(Color::Red));
        assert_eq!(level_color("err"), Some(Color::Red));
        assert_eq!(level_color("FATAL"), Some(Color::Red));
        assert_eq!(level_color("Warning"), Some(Color::Yellow));
        assert_eq!(level_color("INFO"), Some(Color::Cyan));
        assert_eq!(level_color("debug"), Some(Color::Gray));
        assert_eq!(level_color("trace"), Some(Color::Gray));
        assert_eq!(level_color(""), None);
        assert_eq!(level_color("audit"), None);
    }

    #[test]
    fn override_only_for_warn_and_error_levels() {
        assert!(level_overrides_pod_color("ERROR"));
        assert!(level_overrides_pod_color("warn"));
        assert!(level_overrides_pod_color("FATAL"));
        assert!(!level_overrides_pod_color("INFO"));
        assert!(!level_overrides_pod_color("DEBUG"));
        assert!(!level_overrides_pod_color(""));
    }
}
