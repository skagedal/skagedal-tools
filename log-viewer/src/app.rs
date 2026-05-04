use crate::config::{Config, FieldDef};
use crate::entry::{Entry, fuzzy_match, kill_word_left};

/// Concatenate the rendered values of every visible column for `entry`,
/// space-separated. Mirrors `App::row_cells(entry).join(" ")` but takes
/// `columns` directly so callers holding a `&mut App` borrow on another
/// field (e.g. `filtered`) can still compute it.
fn haystack_for(columns: &[ColumnState], entry: &Entry) -> String {
    let mut out = String::new();
    let mut first = true;
    for c in columns.iter().filter(|c| c.visible) {
        if !first {
            out.push(' ');
        }
        first = false;
        out.push_str(&entry.pick(&c.from));
    }
    out
}

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
    /// Fuzzy filter applied to the displayed list. Empty string = no filter.
    pub filter_text: String,
    /// Filter input is focused — typed characters go into `filter_text`
    /// rather than triggering navigation.
    pub filtering: bool,
    /// Optional entry-field name whose value picks a row color via a hash.
    pub color_by_field: Option<String>,
    /// Number of body rows in the list area on the most recent draw, used as
    /// the page size for PgUp/PgDown. The UI updates this before each frame.
    pub list_page_size: usize,
    /// Indices into `entries` that pass the current fuzzy filter, ascending.
    /// `None` when `filter_text` is empty so we don't materialize an
    /// identity vector for huge inputs. Maintained eagerly: rebuilt on
    /// filter / column-visibility / column-order changes, extended in
    /// place on append.
    filtered: Option<Vec<usize>>,
    /// Per-column max content width in chars seen so far, including the
    /// header. Parallel to `columns`. Updated incrementally as entries
    /// arrive so the renderer doesn't rescan every entry per frame.
    column_max_widths: Vec<usize>,
}

impl App {
    pub fn new(config: &Config, source_label: String) -> Self {
        let columns: Vec<ColumnState> = config
            .fields
            .iter()
            .cloned()
            .map(ColumnState::from)
            .collect();
        let column_max_widths = columns.iter().map(|c| c.name.chars().count()).collect();
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
            filter_text: String::new(),
            filtering: false,
            color_by_field: config.color_by_field.clone(),
            list_page_size: 0,
            filtered: None,
            column_max_widths,
        }
    }

    /// Resolve the entry's level using the column named "level"
    /// (case-insensitive) if defined, else the canonical fallback list. Used
    /// for level-based row coloring.
    pub fn level_for(&self, entry: &Entry) -> String {
        if let Some(col) = self
            .columns
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case("level"))
        {
            return entry.pick(&col.from);
        }
        entry.pick(&[
            "level".to_string(),
            "severity".to_string(),
            "lvl".to_string(),
        ])
    }

    /// Value of the configured `color_by_field`, if any, for `entry`.
    pub fn color_key_for(&self, entry: &Entry) -> Option<String> {
        let key = self.color_by_field.as_ref()?;
        let v = entry.get_str(key)?;
        if v.is_empty() { None } else { Some(v) }
    }

    pub fn append_entries<I: IntoIterator<Item = Entry>>(&mut self, new: I) {
        let was_empty = self.entries.is_empty();
        for entry in new {
            // Discover new keys and add them as hidden columns.
            for key in entry.keys() {
                if !self.columns.iter().any(|c| c.name == key) {
                    let header_w = key.chars().count();
                    self.columns.push(ColumnState {
                        name: key.clone(),
                        from: vec![key],
                        visible: false,
                    });
                    self.column_max_widths.push(header_w);
                }
            }
            // Update per-column max widths from this entry.
            for i in 0..self.columns.len() {
                let v = entry.pick(&self.columns[i].from);
                let w = v.chars().count();
                if w > self.column_max_widths[i] {
                    self.column_max_widths[i] = w;
                }
            }
            // If a filter is active, test the new entry incrementally.
            let new_idx = self.entries.len();
            if let Some(filtered) = self.filtered.as_mut() {
                let hay = haystack_for(&self.columns, &entry);
                if fuzzy_match(&self.filter_text, &hay) {
                    filtered.push(new_idx);
                }
            }
            self.entries.push(entry);
        }
        if self.follow {
            if let Some(last) = self.last_displayed() {
                self.selected = last;
            }
        } else if was_empty && !self.entries.is_empty() {
            self.selected = 0;
        }
    }

    /// Rebuild the filter cache from scratch. Called whenever `filter_text`
    /// or anything affecting `haystack_for` (column visibility / order)
    /// changes. O(N) but only on user input — not per frame.
    fn refresh_filter(&mut self) {
        if self.filter_text.is_empty() {
            self.filtered = None;
        } else {
            let mut v = Vec::new();
            for (i, e) in self.entries.iter().enumerate() {
                let hay = haystack_for(&self.columns, e);
                if fuzzy_match(&self.filter_text, &hay) {
                    v.push(i);
                }
            }
            self.filtered = Some(v);
        }
        self.clamp_selection_to_displayed();
    }

    /// Number of currently-displayed entries (filter applied if active).
    pub fn displayed_count(&self) -> usize {
        match &self.filtered {
            Some(v) => v.len(),
            None => self.entries.len(),
        }
    }

    /// Entry index at displayed position `pos`, or `None` if out of range.
    pub fn displayed_at(&self, pos: usize) -> Option<usize> {
        match &self.filtered {
            Some(v) => v.get(pos).copied(),
            None => (pos < self.entries.len()).then_some(pos),
        }
    }

    /// Position of `entry_idx` in the displayed list, or `None` if hidden.
    pub fn position_in_displayed(&self, entry_idx: usize) -> Option<usize> {
        match &self.filtered {
            Some(v) => v.binary_search(&entry_idx).ok(),
            None => (entry_idx < self.entries.len()).then_some(entry_idx),
        }
    }

    pub fn first_displayed(&self) -> Option<usize> {
        self.displayed_at(0)
    }

    pub fn last_displayed(&self) -> Option<usize> {
        let n = self.displayed_count();
        if n == 0 { None } else { self.displayed_at(n - 1) }
    }

    /// Cursor's position in the filtered list (0 if the selected entry isn't
    /// currently displayed).
    pub fn cursor_in_filtered(&self) -> usize {
        self.position_in_displayed(self.selected).unwrap_or(0)
    }

    /// Materialized view of `displayed_at(0..displayed_count)`. Allocates;
    /// tests only.
    #[cfg(test)]
    pub fn filtered_indices(&self) -> Vec<usize> {
        match &self.filtered {
            Some(v) => v.clone(),
            None => (0..self.entries.len()).collect(),
        }
    }

    fn move_in_filtered(&mut self, delta: isize) {
        self.follow = false;
        let n = self.displayed_count();
        if n == 0 {
            return;
        }
        let pos = self.position_in_displayed(self.selected).unwrap_or(0);
        let next = (pos as isize + delta).clamp(0, (n - 1) as isize) as usize;
        if let Some(idx) = self.displayed_at(next) {
            self.selected = idx;
        }
    }

    pub fn enter_filter(&mut self) {
        self.filtering = true;
    }

    pub fn exit_filter(&mut self) {
        self.filtering = false;
    }

    pub fn filter_push(&mut self, c: char) {
        self.filter_text.push(c);
        self.refresh_filter();
    }

    pub fn filter_backspace(&mut self) {
        self.filter_text.pop();
        self.refresh_filter();
    }

    pub fn filter_clear(&mut self) {
        self.filter_text.clear();
        self.filtering = false;
        self.refresh_filter();
    }

    pub fn filter_kill_word(&mut self) {
        let len = self.filter_text.len();
        let (next, _) = kill_word_left(&self.filter_text, len);
        self.filter_text = next;
        self.refresh_filter();
    }

    #[allow(dead_code)] // tests only — TUI uses filter_push/backspace/clear
    pub fn set_filter(&mut self, text: String) {
        self.filter_text = text;
        self.refresh_filter();
    }

    fn clamp_selection_to_displayed(&mut self) {
        if self.displayed_count() == 0 {
            return;
        }
        if self.position_in_displayed(self.selected).is_none()
            && let Some(first) = self.first_displayed()
        {
            self.selected = first;
        }
    }

    pub fn visible_columns(&self) -> Vec<&ColumnState> {
        self.columns.iter().filter(|c| c.visible).collect()
    }

    pub fn row_cells(&self, entry: &Entry) -> Vec<String> {
        self.columns
            .iter()
            .filter(|c| c.visible)
            .map(|c| entry.pick(&c.from))
            .collect()
    }

    /// Per-visible-column max content width (chars), header included.
    /// Sliced from the running `column_max_widths` — no per-frame scan.
    pub fn visible_widths(&self) -> Vec<usize> {
        self.columns
            .iter()
            .enumerate()
            .filter(|(_, c)| c.visible)
            .map(|(i, _)| self.column_max_widths[i])
            .collect()
    }

    pub fn selected_entry(&self) -> Option<&Entry> {
        self.entries.get(self.selected)
    }

    pub fn move_down(&mut self) {
        self.move_in_filtered(1);
    }

    pub fn move_up(&mut self) {
        self.move_in_filtered(-1);
    }

    /// Move by a page (PgUp / PgDown). The page size is the list area's
    /// last-drawn body height; falls back to a small default when nothing
    /// has been drawn yet.
    pub fn move_page(&mut self, direction: isize) {
        let page = self.list_page_size.max(1) as isize;
        self.move_in_filtered(direction * page);
    }

    pub fn jump_top(&mut self) {
        self.follow = false;
        if let Some(first) = self.first_displayed() {
            self.selected = first;
        }
    }

    pub fn jump_bottom(&mut self) {
        self.follow = false;
        if let Some(last) = self.last_displayed() {
            self.selected = last;
        }
    }

    pub fn toggle_follow(&mut self) {
        self.follow = !self.follow;
        if self.follow
            && let Some(last) = self.last_displayed()
        {
            self.selected = last;
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
        if let Some(pos) = self.position_in_displayed(self.selected)
            && let Some(next) = self.displayed_at(pos + 1)
        {
            self.selected = next;
            self.detail.row = self.detail.row.min(self.detail_rows().saturating_sub(1));
        }
    }

    pub fn detail_prev_entry(&mut self) {
        if let Some(pos) = self.position_in_displayed(self.selected)
            && pos > 0
            && let Some(prev) = self.displayed_at(pos - 1)
        {
            self.selected = prev;
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
            let header_w = key.chars().count();
            self.columns.push(ColumnState {
                name: key.clone(),
                from: vec![key],
                visible: true,
            });
            self.column_max_widths.push(header_w);
        }
        self.refresh_filter();
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
            self.refresh_filter();
        }
    }

    pub fn fields_menu_swap(&mut self, delta: isize) {
        let i = self.fields_menu.cursor;
        let j = i as isize + delta;
        if j < 0 || j as usize >= self.columns.len() {
            return;
        }
        let j = j as usize;
        self.columns.swap(i, j);
        self.column_max_widths.swap(i, j);
        self.fields_menu.cursor = j;
        self.refresh_filter();
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
    fn filter_narrows_displayed_count() {
        let mut app = make(0);
        app.append_entries([
            Entry::parse(r#"{"level":"info","message":"hello"}"#, "message"),
            Entry::parse(r#"{"level":"error","message":"oops"}"#, "message"),
            Entry::parse(r#"{"level":"info","message":"yo"}"#, "message"),
        ]);
        assert_eq!(app.displayed_count(), 3);
        app.set_filter("err".into());
        assert_eq!(app.displayed_count(), 1);
        assert_eq!(app.filtered_indices(), vec![1]);
    }

    #[test]
    fn move_down_skips_non_matching_entries() {
        let mut app = make(0);
        app.append_entries([
            Entry::parse(r#"{"message":"alpha"}"#, "message"),
            Entry::parse(r#"{"message":"beta"}"#, "message"),
            Entry::parse(r#"{"message":"alphabet"}"#, "message"),
        ]);
        app.set_filter("alph".into());
        // displayed = entries 0 and 2
        assert_eq!(app.selected, 0);
        app.move_down();
        assert_eq!(app.selected, 2);
        app.move_down();
        assert_eq!(app.selected, 2); // clamped
        app.move_up();
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn follow_pins_to_filtered_tail() {
        let mut app = make(0);
        app.set_filter("info".into());
        app.toggle_follow();
        app.append_entries([Entry::parse(r#"{"level":"error"}"#, "message")]);
        // No matching entry yet: selection unchanged.
        app.append_entries([Entry::parse(r#"{"level":"info"}"#, "message")]);
        // Now the latest matching entry is index 1.
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn filter_clears_with_filter_clear() {
        let mut app = make(0);
        app.append_entries([Entry::parse(r#"{"level":"info"}"#, "message")]);
        app.set_filter("zzz".into());
        assert_eq!(app.displayed_count(), 0);
        app.filter_clear();
        assert_eq!(app.displayed_count(), 1);
    }

    #[test]
    fn filter_kill_word_removes_last_word() {
        let mut app = make(0);
        app.set_filter("foo bar".into());
        app.filter_kill_word();
        assert_eq!(app.filter_text, "foo ");
    }

    #[test]
    fn level_for_uses_level_column_when_present() {
        let mut cfg = cfg();
        cfg.fields = vec![
            crate::config::FieldDef {
                name: "level".into(),
                from: vec!["severity".into(), "level".into()],
            },
        ];
        let app = App::new(&cfg, "test".into());
        let e = Entry::parse(r#"{"severity":"WARN"}"#, "message");
        assert_eq!(app.level_for(&e), "WARN");
    }

    #[test]
    fn level_for_falls_back_to_canonical_keys() {
        let mut cfg = cfg();
        cfg.fields.clear();
        let app = App::new(&cfg, "test".into());
        let e = Entry::parse(r#"{"lvl":"ERROR"}"#, "message");
        assert_eq!(app.level_for(&e), "ERROR");
    }

    #[test]
    fn color_key_for_returns_value_when_field_set() {
        let mut cfg = cfg();
        cfg.color_by_field = Some("pod".into());
        let app = App::new(&cfg, "test".into());
        let e = Entry::parse(r#"{"pod":"p1"}"#, "message");
        assert_eq!(app.color_key_for(&e).as_deref(), Some("p1"));
    }

    #[test]
    fn color_key_for_returns_none_when_unset_or_empty() {
        let app = App::new(&cfg(), "test".into());
        let e = Entry::parse(r#"{"pod":"p1"}"#, "message");
        assert!(app.color_key_for(&e).is_none()); // no color_by_field configured
        let mut cfg = cfg();
        cfg.color_by_field = Some("pod".into());
        let app = App::new(&cfg, "test".into());
        let e = Entry::parse(r#"{"other":"v"}"#, "message");
        assert!(app.color_key_for(&e).is_none()); // field absent
    }

    #[test]
    fn move_page_uses_list_page_size() {
        let mut app = make(50);
        app.list_page_size = 10;
        app.move_page(1);
        assert_eq!(app.selected, 10);
        app.move_page(1);
        assert_eq!(app.selected, 20);
        app.move_page(-1);
        assert_eq!(app.selected, 10);
        // Going past the end clamps.
        app.move_page(99);
        assert_eq!(app.selected, 49);
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

    #[test]
    fn column_max_widths_grow_with_appended_entries() {
        let mut app = make(0);
        // Add a column "msg" so we can observe its width grow.
        app.columns.push(ColumnState {
            name: "msg".into(),
            from: vec!["msg".into()],
            visible: true,
        });
        app.column_max_widths.push("msg".chars().count());
        let initial = *app.column_max_widths.last().unwrap();
        app.append_entries([Entry::parse(r#"{"msg":"hello world!"}"#, "message")]);
        assert!(*app.column_max_widths.last().unwrap() > initial);
        assert_eq!(*app.column_max_widths.last().unwrap(), "hello world!".len());
    }

    #[test]
    fn column_max_widths_track_column_swaps() {
        let mut app = make(0);
        app.columns.clear();
        app.column_max_widths.clear();
        app.columns.push(ColumnState { name: "a".into(), from: vec!["a".into()], visible: true });
        app.column_max_widths.push(1);
        app.columns.push(ColumnState { name: "b".into(), from: vec!["b".into()], visible: true });
        app.column_max_widths.push(2);
        app.open_fields_menu();
        app.fields_menu_swap(1);
        // After swap, widths follow the columns.
        assert_eq!(app.columns[0].name, "b");
        assert_eq!(app.column_max_widths[0], 2);
        assert_eq!(app.columns[1].name, "a");
        assert_eq!(app.column_max_widths[1], 1);
    }

    #[test]
    fn filter_cache_extends_on_append() {
        let mut app = make(0);
        app.append_entries([Entry::parse(r#"{"message":"alpha"}"#, "message")]);
        app.set_filter("bet".into());
        assert_eq!(app.displayed_count(), 0);
        app.append_entries([Entry::parse(r#"{"message":"beta"}"#, "message")]);
        assert_eq!(app.displayed_count(), 1);
        assert_eq!(app.displayed_at(0), Some(1));
    }

    #[test]
    fn position_in_displayed_uses_binary_search() {
        let mut app = make(0);
        for i in 0..10 {
            app.append_entries([Entry::parse(
                &format!(r#"{{"message":"x{i}"}}"#),
                "message",
            )]);
        }
        // Filter that matches every entry.
        app.set_filter("x".into());
        assert_eq!(app.position_in_displayed(7), Some(7));
        assert_eq!(app.position_in_displayed(99), None);
    }
}
