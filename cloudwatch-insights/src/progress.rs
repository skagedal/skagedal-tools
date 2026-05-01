//! Live progress reporter for `query` polling. On a TTY, rewrites a single
//! line on stderr; off-TTY, appends one line per status change.

use std::io::{IsTerminal, Write};

use crate::insights::{QueryProgress, QueryStatistics};
use crate::terminal::{colorize, ColorOptions};

/// Width of the bold prompt column shared by the header and the live status
/// line (e.g. "AWS region:  ", "Status:      ").
pub const HEADER_COLUMN_WIDTH: usize = 13;

pub struct ProgressReporter {
    quiet: bool,
    tty: bool,
    last_line: String,
    last_status: String,
    written: bool,
}

impl ProgressReporter {
    pub fn new(quiet: bool) -> Self {
        Self {
            quiet,
            tty: std::io::stderr().is_terminal(),
            last_line: String::new(),
            last_status: String::new(),
            written: false,
        }
    }

    pub fn update(&mut self, progress: &QueryProgress) {
        if self.quiet {
            return;
        }
        let line = format_progress_line(progress);
        let mut stderr = std::io::stderr().lock();
        let prefix = status_prefix();
        if self.tty {
            if line == self.last_line {
                return;
            }
            // \x1b[2K erases the current line; \r returns to column 0.
            let _ = write!(stderr, "\r\x1b[2K{prefix}{line}");
            let _ = stderr.flush();
            self.last_line = line;
            self.written = true;
        } else {
            if progress.status == self.last_status {
                return;
            }
            self.last_status = progress.status.clone();
            let _ = writeln!(stderr, "{prefix}{line}");
        }
    }

    pub fn done(&mut self) {
        if self.quiet || !self.tty || !self.written {
            return;
        }
        let _ = writeln!(std::io::stderr());
    }
}

fn status_prefix() -> String {
    let label = "Status:";
    let bold = colorize(label, ColorOptions { bold: true, ..Default::default() });
    let pad = " ".repeat(HEADER_COLUMN_WIDTH.saturating_sub(label.len()));
    format!("{bold}{pad}")
}

fn format_progress_line(progress: &QueryProgress) -> String {
    let stats = format_statistics(&progress.statistics);
    if stats.is_empty() {
        progress.status.clone()
    } else {
        format!("{} ({stats})", progress.status)
    }
}

fn format_statistics(stats: &QueryStatistics) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(n) = stats.records_scanned {
        parts.push(format!("scanned {} records", format_count(n)));
    }
    if let Some(n) = stats.bytes_scanned {
        parts.push(format_bytes(n));
    }
    if let Some(n) = stats.records_matched {
        parts.push(format!("{} rows", format_count(n)));
    }
    parts.join(", ")
}

fn format_count(n: f64) -> String {
    if n < 1_000.0 {
        format!("{}", n as i64)
    } else if n < 1_000_000.0 {
        format!("{:.1}k", n / 1_000.0)
    } else if n < 1_000_000_000.0 {
        format!("{:.1}M", n / 1_000_000.0)
    } else {
        format!("{:.1}B", n / 1_000_000_000.0)
    }
}

fn format_bytes(n: f64) -> String {
    if n < 1024.0 {
        format!("{} B", n as i64)
    } else if n < 1024.0 * 1024.0 {
        format!("{:.1} KiB", n / 1024.0)
    } else if n < 1024.0_f64.powi(3) {
        format!("{:.1} MiB", n / (1024.0_f64.powi(2)))
    } else {
        format!("{:.1} GiB", n / (1024.0_f64.powi(3)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_count() {
        assert_eq!(format_count(42.0), "42");
        assert_eq!(format_count(1_500.0), "1.5k");
        assert_eq!(format_count(1_200_000.0), "1.2M");
        assert_eq!(format_count(2_500_000_000.0), "2.5B");
    }

    #[test]
    fn formats_bytes() {
        assert_eq!(format_bytes(456.0), "456 B");
        assert_eq!(format_bytes(2048.0), "2.0 KiB");
        assert_eq!(format_bytes(1.5 * 1024.0 * 1024.0), "1.5 MiB");
        assert_eq!(format_bytes(2.0 * 1024.0_f64.powi(3)), "2.0 GiB");
    }

    #[test]
    fn formats_progress_line_with_stats() {
        let p = QueryProgress {
            status: "Running".into(),
            statistics: QueryStatistics {
                records_scanned: Some(1_200_000.0),
                bytes_scanned: Some(456.0 * 1024.0 * 1024.0),
                records_matched: Some(0.0),
            },
        };
        assert_eq!(
            format_progress_line(&p),
            "Running (scanned 1.2M records, 456.0 MiB, 0 rows)"
        );
    }

    #[test]
    fn formats_progress_line_no_stats() {
        let p = QueryProgress {
            status: "Scheduled".into(),
            statistics: QueryStatistics::default(),
        };
        assert_eq!(format_progress_line(&p), "Scheduled");
    }
}
