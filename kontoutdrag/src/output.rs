//! Printing rows in the three output formats.

use std::io::Write;

use anyhow::Result;

/// A table of strings, printed aligned, tab-separated, or as JSON objects
/// keyed by the header names.
pub struct Rows {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    /// Columns holding numbers, right-aligned in table output.
    numeric: Vec<usize>,
}

impl Rows {
    pub fn new(headers: &[&str]) -> Rows {
        Rows {
            headers: headers.iter().map(|h| h.to_string()).collect(),
            rows: Vec::new(),
            numeric: Vec::new(),
        }
    }

    pub fn right_align(mut self, columns: &[usize]) -> Rows {
        self.numeric = columns.to_vec();
        self
    }

    pub fn push<I, S>(&mut self, row: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.rows.push(row.into_iter().map(Into::into).collect());
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn write(&self, out: &mut impl Write, format: crate::cli::OutputFormat) -> Result<()> {
        match format {
            crate::cli::OutputFormat::Table => self.write_table(out),
            crate::cli::OutputFormat::Tsv => self.write_tsv(out),
            crate::cli::OutputFormat::Json => self.write_json(out),
        }
    }

    fn write_table(&self, out: &mut impl Write) -> Result<()> {
        let widths: Vec<usize> = (0..self.headers.len())
            .map(|i| {
                std::iter::once(&self.headers[i])
                    .chain(self.rows.iter().filter_map(|r| r.get(i)))
                    .map(|s| s.chars().count())
                    .max()
                    .unwrap_or(0)
            })
            .collect();

        let line = |out: &mut dyn Write, cells: &[String]| -> Result<()> {
            let mut parts = Vec::new();
            for (i, width) in widths.iter().enumerate() {
                let cell = cells.get(i).map(String::as_str).unwrap_or("");
                let padding = width.saturating_sub(cell.chars().count());
                parts.push(if i == widths.len() - 1 && !self.numeric.contains(&i) {
                    cell.to_string()
                } else if self.numeric.contains(&i) {
                    format!("{}{}", " ".repeat(padding), cell)
                } else {
                    format!("{}{}", cell, " ".repeat(padding))
                });
            }
            writeln!(out, "{}", parts.join("  ").trim_end())?;
            Ok(())
        };

        line(out, &self.headers)?;
        let rule: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
        line(out, &rule)?;
        for row in &self.rows {
            line(out, row)?;
        }
        Ok(())
    }

    fn write_tsv(&self, out: &mut impl Write) -> Result<()> {
        writeln!(out, "{}", self.headers.join("\t"))?;
        for row in &self.rows {
            writeln!(out, "{}", row.join("\t"))?;
        }
        Ok(())
    }

    fn write_json(&self, out: &mut impl Write) -> Result<()> {
        for row in &self.rows {
            let object: serde_json::Map<String, serde_json::Value> = self
                .headers
                .iter()
                .zip(row)
                .map(|(header, cell)| (header.clone(), serde_json::Value::String(cell.clone())))
                .collect();
            writeln!(out, "{}", serde_json::Value::Object(object))?;
        }
        Ok(())
    }
}
