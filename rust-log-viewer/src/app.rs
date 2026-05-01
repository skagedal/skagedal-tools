use crate::config::{Config, FieldDef};
use crate::entry::Entry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    List,
    Detail,
    FieldsMenu,
}

#[derive(Debug, Clone)]
pub struct ColumnState {
    pub name: String,
    pub from: Vec<String>,
    pub visible: bool,
}

impl From<FieldDef> for ColumnState {
    fn from(f: FieldDef) -> Self {
        let from = if f.from.is_empty() {
            vec![f.name.clone()]
        } else {
            f.from
        };
        Self {
            name: f.name,
            from,
            visible: true,
        }
    }
}

/// Detail view selection: 0 = entry header (the full JSON line), 1.. = fields
/// of the current entry in their insertion order.
#[derive(Debug, Default, Clone, Copy)]
pub struct DetailState {
    pub row: usize,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct FieldsMenuState {
    pub cursor: usize,
}

pub struct App {
    pub entries: Vec<Entry>,
    pub source_label: String,
    pub columns: Vec<ColumnState>,
    pub selected: usize,
    pub view: View,
    pub follow: bool,
    pub detail: DetailState,
    pub fields_menu: FieldsMenuState,
    pub status_msg: Option<String>,
    pub should_quit: bool,
}

impl App {
    pub fn new(config: &Config, source_label: String) -> Self {
        let columns = config
            .fields
            .iter()
            .cloned()
            .map(ColumnState::from)
            .collect();
        Self {
            entries: Vec::new(),
            source_label,
            columns,
            selected: 0,
            view: View::List,
            follow: false,
            detail: DetailState::default(),
            fields_menu: FieldsMenuState::default(),
            status_msg: None,
            should_quit: false,
        }
    }

    pub fn append_entries<I: IntoIterator<Item = Entry>>(&mut self, new: I) {
        let was_empty = self.entries.is_empty();
        for entry in new {
            for key in entry.keys() {
                if !self.columns.iter().any(|c| c.name == key) {
                    self.columns.push(ColumnState {
                        name: key.clone(),
                        from: vec![key],
                        visible: false,
                    });
                }
            }
            self.entries.push(entry);
        }
        if self.follow && !self.entries.is_empty() {
            self.selected = self.entries.len() - 1;
        } else if was_empty && !self.entries.is_empty() {
            self.selected = 0;
        }
    }

    pub fn visible_columns(&self) -> Vec<&ColumnState> {
        self.columns.iter().filter(|c| c.visible).collect()
    }

    pub fn row_cells(&self, entry: &Entry) -> Vec<String> {
        self.visible_columns()
            .iter()
            .map(|c| entry.pick(&c.from))
            .collect()
    }

    pub fn selected_entry(&self) -> Option<&Entry> {
        self.entries.get(self.selected)
    }

    pub fn move_down(&mut self) {
        self.follow = false;
        if self.selected + 1 < self.entries.len() {
            self.selected += 1;
        }
    }

    pub fn move_up(&mut self) {
        self.follow = false;
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn jump_top(&mut self) {
        self.follow = false;
        self.selected = 0;
    }

    pub fn jump_bottom(&mut self) {
        self.follow = false;
        if !self.entries.is_empty() {
            self.selected = self.entries.len() - 1;
        }
    }

    pub fn toggle_follow(&mut self) {
        self.follow = !self.follow;
        if self.follow && !self.entries.is_empty() {
            self.selected = self.entries.len() - 1;
        }
    }

    pub fn open_detail(&mut self) {
        if !self.entries.is_empty() {
            self.view = View::Detail;
            self.detail = DetailState::default();
        }
    }

    pub fn close_detail(&mut self) {
        self.view = View::List;
    }

    pub fn open_fields_menu(&mut self) {
        self.view = View::FieldsMenu;
        if self.fields_menu.cursor >= self.columns.len() {
            self.fields_menu.cursor = self.columns.len().saturating_sub(1);
        }
    }

    pub fn close_fields_menu(&mut self) {
        self.view = if matches!(self.view, View::FieldsMenu) {
            View::List
        } else {
            self.view
        };
    }

    pub fn detail_rows(&self) -> usize {
        // header + one row per field of the current entry
        1 + self.selected_entry().map(|e| e.keys().len()).unwrap_or(0)
    }

    pub fn detail_move_down(&mut self) {
        let max = self.detail_rows().saturating_sub(1);
        if self.detail.row < max {
            self.detail.row += 1;
        }
    }

    pub fn detail_move_up(&mut self) {
        if self.detail.row > 0 {
            self.detail.row -= 1;
        }
    }

    pub fn detail_next_entry(&mut self) {
        if self.selected + 1 < self.entries.len() {
            self.selected += 1;
            self.detail.row = self.detail.row.min(self.detail_rows().saturating_sub(1));
        }
    }

    pub fn detail_prev_entry(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
            self.detail.row = self.detail.row.min(self.detail_rows().saturating_sub(1));
        }
    }

    /// Returns the (field name, value) the detail cursor is on, or None if
    /// the cursor is on the header row.
    pub fn detail_field(&self) -> Option<(String, String)> {
        if self.detail.row == 0 {
            return None;
        }
        let entry = self.selected_entry()?;
        let key = entry.keys().get(self.detail.row - 1)?.clone();
        let value = entry.get_str(&key).unwrap_or_default();
        Some((key, value))
    }

    /// Toggle visibility of the column matching the detail cursor's field.
    /// If the field isn't a known column, add it (visible).
    pub fn detail_toggle_column(&mut self) {
        let Some((key, _)) = self.detail_field() else {
            return;
        };
        if let Some(col) = self.columns.iter_mut().find(|c| c.name == key) {
            col.visible = !col.visible;
        } else {
            self.columns.push(ColumnState {
                name: key.clone(),
                from: vec![key],
                visible: true,
            });
        }
    }

    pub fn fields_menu_move(&mut self, delta: isize) {
        let len = self.columns.len();
        if len == 0 {
            return;
        }
        let next = (self.fields_menu.cursor as isize + delta).clamp(0, (len - 1) as isize);
        self.fields_menu.cursor = next as usize;
    }

    pub fn fields_menu_toggle(&mut self) {
        if let Some(c) = self.columns.get_mut(self.fields_menu.cursor) {
            c.visible = !c.visible;
        }
    }

    pub fn fields_menu_swap(&mut self, delta: isize) {
        let i = self.fields_menu.cursor;
        let j = i as isize + delta;
        if j < 0 || j as usize >= self.columns.len() {
            return;
        }
        self.columns.swap(i, j as usize);
        self.fields_menu.cursor = j as usize;
    }

    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status_msg = Some(msg.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn cfg() -> Config {
        Config::default()
    }

    fn make(n: usize) -> App {
        let mut app = App::new(&cfg(), "test".into());
        for i in 0..n {
            let e = Entry::parse(&format!(r#"{{"i":{i}}}"#), "message");
            app.append_entries([e]);
        }
        app
    }

    #[test]
    fn navigation_clamps_at_bounds() {
        let mut app = make(3);
        app.move_up();
        assert_eq!(app.selected, 0);
        app.move_down();
        app.move_down();
        app.move_down();
        assert_eq!(app.selected, 2);
    }

    #[test]
    fn follow_pins_to_latest_entry() {
        let mut app = make(0);
        app.toggle_follow();
        app.append_entries([Entry::parse(r#"{"i":0}"#, "message")]);
        app.append_entries([Entry::parse(r#"{"i":1}"#, "message")]);
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn follow_breaks_on_navigation() {
        let mut app = make(3);
        app.toggle_follow();
        assert_eq!(app.selected, 2);
        app.move_up();
        assert!(!app.follow);
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn unknown_keys_become_hidden_columns() {
        let mut app = make(0);
        let initial = app.columns.len();
        app.append_entries([Entry::parse(r#"{"newkey":"v"}"#, "message")]);
        assert_eq!(app.columns.len(), initial + 1);
        let added = app.columns.last().unwrap();
        assert_eq!(added.name, "newkey");
        assert!(!added.visible);
    }

    #[test]
    fn fields_menu_swap_moves_column_and_cursor() {
        let mut app = make(0);
        let names_before: Vec<_> = app.columns.iter().map(|c| c.name.clone()).collect();
        app.open_fields_menu();
        app.fields_menu_move(1);
        app.fields_menu_swap(-1);
        let names_after: Vec<_> = app.columns.iter().map(|c| c.name.clone()).collect();
        assert_eq!(names_after[0], names_before[1]);
        assert_eq!(names_after[1], names_before[0]);
        assert_eq!(app.fields_menu.cursor, 0);
    }

    #[test]
    fn detail_toggle_for_existing_column() {
        let mut app = make(1);
        // Detail row 1 = first key of entry "i". "i" isn't a column yet.
        app.open_detail();
        app.detail_move_down();
        app.detail_toggle_column();
        let i_col = app.columns.iter().find(|c| c.name == "i").unwrap();
        assert!(i_col.visible);
        app.detail_toggle_column();
        let i_col = app.columns.iter().find(|c| c.name == "i").unwrap();
        assert!(!i_col.visible);
    }

    #[test]
    fn detail_field_is_none_on_header_row() {
        let mut app = make(1);
        app.open_detail();
        assert!(app.detail_field().is_none());
        app.detail_move_down();
        assert!(app.detail_field().is_some());
    }

    #[test]
    fn row_cells_uses_only_visible_columns() {
        let mut app = make(0);
        // Hide all columns
        for c in &mut app.columns {
            c.visible = false;
        }
        let e = Entry::parse(r#"{"a":1}"#, "message");
        assert!(app.row_cells(&e).is_empty());
    }
}
