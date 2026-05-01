use crate::entry::Entry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    List,
    Detail,
}

/// Hardcoded column layout. Config-driven fields are tracked in DESIGN.md as
/// follow-up work; this MVP matches the TS log-viewer's defaults.
pub const COLUMNS: &[Column] = &[
    Column { name: "time", from: &["@timestamp", "timestamp", "time", "ts"] },
    Column { name: "level", from: &["level", "severity", "lvl"] },
    Column { name: "message", from: &["message", "msg", "@message"] },
];

#[derive(Debug)]
pub struct Column {
    pub name: &'static str,
    pub from: &'static [&'static str],
}

pub struct App {
    pub entries: Vec<Entry>,
    pub source_label: String,
    pub selected: usize,
    pub view: View,
    pub should_quit: bool,
}

impl App {
    pub fn new(entries: Vec<Entry>, source_label: String) -> Self {
        Self {
            entries,
            source_label,
            selected: 0,
            view: View::List,
            should_quit: false,
        }
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.entries.len() {
            self.selected += 1;
        }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn jump_top(&mut self) {
        self.selected = 0;
    }

    pub fn jump_bottom(&mut self) {
        if !self.entries.is_empty() {
            self.selected = self.entries.len() - 1;
        }
    }

    pub fn open_detail(&mut self) {
        if !self.entries.is_empty() {
            self.view = View::Detail;
        }
    }

    pub fn close_detail(&mut self) {
        self.view = View::List;
    }

    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    pub fn selected_entry(&self) -> Option<&Entry> {
        self.entries.get(self.selected)
    }

    pub fn row_cells(&self, entry: &Entry) -> Vec<String> {
        COLUMNS.iter().map(|col| entry.pick(col.from)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::Entry;

    fn make(n: usize) -> App {
        let entries = (0..n)
            .map(|i| Entry::parse(&format!(r#"{{"i":{i}}}"#)))
            .collect();
        App::new(entries, "test".into())
    }

    #[test]
    fn navigation_clamps_at_bounds() {
        let mut app = make(3);
        app.move_up(); // already at 0
        assert_eq!(app.selected, 0);
        app.move_down();
        app.move_down();
        app.move_down(); // would go to 3, clamped to 2
        assert_eq!(app.selected, 2);
    }

    #[test]
    fn jump_top_and_bottom() {
        let mut app = make(5);
        app.jump_bottom();
        assert_eq!(app.selected, 4);
        app.jump_top();
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn jump_bottom_on_empty_is_noop() {
        let mut app = make(0);
        app.jump_bottom();
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn open_detail_requires_entries() {
        let mut app = make(0);
        app.open_detail();
        assert_eq!(app.view, View::List);
    }

    #[test]
    fn row_cells_for_default_columns() {
        let app = make(0);
        let entry = Entry::parse(r#"{"timestamp":"t1","level":"info","message":"hi"}"#);
        assert_eq!(app.row_cells(&entry), vec!["t1", "info", "hi"]);
    }
}
