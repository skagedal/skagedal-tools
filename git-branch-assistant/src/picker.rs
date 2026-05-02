use std::io::Write;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, TryRecvError};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use console::{Key, Term};

use crate::services::git_repos_list_service::{BranchListEntry, format_entry_lines};

pub fn stderr_is_terminal() -> bool {
    Term::stderr().is_term()
}

pub enum PickerOutcome {
    Picked(BranchListEntry),
    Cancelled,
}

enum Event {
    Key(Key),
    KeyEnded,
    Refresh(Vec<BranchListEntry>),
    RefreshEnded,
}

struct State {
    entries: Vec<BranchListEntry>,
    selected: usize,
    refreshing: bool,
    rendered_rows: usize,
}

pub fn run(
    initial: Vec<BranchListEntry>,
    refresh: Option<Receiver<Vec<BranchListEntry>>>,
) -> Result<PickerOutcome> {
    let mut term = Term::stderr();
    let (event_tx, event_rx) = mpsc::channel::<Event>();

    let key_term = Term::stderr();
    let key_tx = event_tx.clone();
    thread::spawn(move || {
        loop {
            match key_term.read_key() {
                Ok(key) => {
                    if key_tx.send(Event::Key(key)).is_err() {
                        break;
                    }
                }
                Err(_) => {
                    let _ = key_tx.send(Event::KeyEnded);
                    break;
                }
            }
        }
    });

    let refreshing = refresh.is_some();
    if let Some(refresh_rx) = refresh {
        let refresh_tx = event_tx.clone();
        thread::spawn(move || match refresh_rx.recv() {
            Ok(entries) => {
                let _ = refresh_tx.send(Event::Refresh(entries));
            }
            Err(_) => {
                let _ = refresh_tx.send(Event::RefreshEnded);
            }
        });
    }
    drop(event_tx);

    let _ = term.hide_cursor();

    let mut state = State {
        entries: initial,
        selected: 0,
        refreshing,
        rendered_rows: 0,
    };

    let outcome = loop {
        render(&mut term, &mut state)?;
        let event = match event_rx.recv_timeout(Duration::from_secs(60)) {
            Ok(event) => event,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break PickerOutcome::Cancelled,
        };
        match handle_event(&mut state, event) {
            EventResult::Continue => {}
            EventResult::Pick => {
                if let Some(entry) = state.entries.get(state.selected).cloned() {
                    break PickerOutcome::Picked(entry);
                }
            }
            EventResult::Cancel => break PickerOutcome::Cancelled,
        }
        // Drain any other events that arrived while we were rendering.
        loop {
            match event_rx.try_recv() {
                Ok(event) => match handle_event(&mut state, event) {
                    EventResult::Continue => {}
                    EventResult::Pick => {
                        if let Some(entry) = state.entries.get(state.selected).cloned() {
                            return finish(
                                &mut term,
                                state.rendered_rows,
                                PickerOutcome::Picked(entry),
                            );
                        }
                    }
                    EventResult::Cancel => {
                        return finish(&mut term, state.rendered_rows, PickerOutcome::Cancelled);
                    }
                },
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }
    };

    finish(&mut term, state.rendered_rows, outcome)
}

fn finish(term: &mut Term, rendered_rows: usize, outcome: PickerOutcome) -> Result<PickerOutcome> {
    clear_rendered(term, rendered_rows)?;
    let _ = term.show_cursor();
    Ok(outcome)
}

enum EventResult {
    Continue,
    Pick,
    Cancel,
}

fn handle_event(state: &mut State, event: Event) -> EventResult {
    match event {
        Event::Key(key) => match key {
            Key::ArrowUp | Key::Char('k') => {
                if state.selected > 0 {
                    state.selected -= 1;
                }
                EventResult::Continue
            }
            Key::ArrowDown | Key::Char('j') => {
                if !state.entries.is_empty() && state.selected + 1 < state.entries.len() {
                    state.selected += 1;
                }
                EventResult::Continue
            }
            Key::Home => {
                state.selected = 0;
                EventResult::Continue
            }
            Key::End => {
                if !state.entries.is_empty() {
                    state.selected = state.entries.len() - 1;
                }
                EventResult::Continue
            }
            Key::Enter => EventResult::Pick,
            Key::Escape | Key::CtrlC | Key::Char('q') => EventResult::Cancel,
            _ => EventResult::Continue,
        },
        Event::KeyEnded => EventResult::Cancel,
        Event::Refresh(new_entries) => {
            apply_refresh(state, new_entries);
            EventResult::Continue
        }
        Event::RefreshEnded => {
            state.refreshing = false;
            EventResult::Continue
        }
    }
}

fn apply_refresh(state: &mut State, new_entries: Vec<BranchListEntry>) {
    let current_key = state
        .entries
        .get(state.selected)
        .map(|entry| (entry.repo_path.clone(), entry.refname.clone()));
    let new_selected = current_key
        .and_then(|(repo_path, refname)| {
            new_entries
                .iter()
                .position(|entry| entry.repo_path == repo_path && entry.refname == refname)
        })
        .unwrap_or(0);
    state.entries = new_entries;
    state.selected = new_selected.min(state.entries.len().saturating_sub(1));
    state.refreshing = false;
}

fn render(term: &mut Term, state: &mut State) -> Result<()> {
    clear_rendered(term, state.rendered_rows)?;

    let header = if state.refreshing {
        "Refreshing... (\u{2191}/\u{2193} navigate, Enter select, Esc cancel)"
    } else {
        "(\u{2191}/\u{2193} navigate, Enter select, Esc cancel)"
    };
    writeln!(term, "{header}")?;

    let lines = format_entry_lines(&state.entries);
    let height = term.size().0 as usize;
    let max_visible = height.saturating_sub(3).max(1);
    let (top, bottom) = visible_window(state.selected, lines.len(), max_visible);

    if lines.is_empty() {
        writeln!(term, "  (no branches)")?;
        state.rendered_rows = 2;
        return Ok(());
    }

    for (idx, line) in lines.iter().enumerate().take(bottom).skip(top) {
        if idx == state.selected {
            writeln!(term, "> {line}")?;
        } else {
            writeln!(term, "  {line}")?;
        }
    }

    state.rendered_rows = 1 + (bottom - top);
    Ok(())
}

fn visible_window(selected: usize, total: usize, max_visible: usize) -> (usize, usize) {
    if total == 0 {
        return (0, 0);
    }
    if total <= max_visible {
        return (0, total);
    }
    let half = max_visible / 2;
    let top = selected.saturating_sub(half);
    let bottom = (top + max_visible).min(total);
    let top = bottom.saturating_sub(max_visible);
    (top, bottom)
}

fn clear_rendered(term: &mut Term, rows: usize) -> Result<()> {
    if rows == 0 {
        return Ok(());
    }
    term.clear_last_lines(rows)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::git_repos_list_service::BranchStatus;
    use std::path::PathBuf;

    fn make_entry(repo: &str, refname: &str) -> BranchListEntry {
        BranchListEntry {
            repo_name: repo.to_string(),
            repo_path: PathBuf::from("/tmp").join(repo),
            refname: refname.to_string(),
            status: BranchStatus::Identical,
            commit_timestamp: 1000,
            commit_date: "2024-01-01".to_string(),
            committer: "alice".to_string(),
            worktree_path: None,
        }
    }

    fn state(entries: Vec<BranchListEntry>, selected: usize) -> State {
        State {
            entries,
            selected,
            refreshing: true,
            rendered_rows: 0,
        }
    }

    #[test]
    fn refresh_keeps_selection_on_same_branch() {
        let mut s = state(
            vec![
                make_entry("a", "main"),
                make_entry("b", "main"),
                make_entry("c", "feature"),
            ],
            1,
        );
        apply_refresh(
            &mut s,
            vec![
                make_entry("c", "feature"),
                make_entry("b", "main"),
                make_entry("a", "main"),
            ],
        );
        assert_eq!(s.selected, 1);
        assert_eq!(s.entries[s.selected].repo_name, "b");
        assert!(!s.refreshing);
    }

    #[test]
    fn refresh_falls_back_to_first_when_branch_missing() {
        let mut s = state(vec![make_entry("a", "old"), make_entry("b", "main")], 0);
        apply_refresh(&mut s, vec![make_entry("b", "main"), make_entry("c", "x")]);
        assert_eq!(s.selected, 0);
        assert_eq!(s.entries[s.selected].repo_name, "b");
    }

    #[test]
    fn refresh_to_empty_clamps_selection() {
        let mut s = state(vec![make_entry("a", "main")], 0);
        apply_refresh(&mut s, vec![]);
        assert_eq!(s.entries.len(), 0);
        assert_eq!(s.selected, 0);
    }

    #[test]
    fn visible_window_centers_on_selection() {
        assert_eq!(visible_window(0, 10, 5), (0, 5));
        assert_eq!(visible_window(5, 10, 5), (3, 8));
        assert_eq!(visible_window(9, 10, 5), (5, 10));
        assert_eq!(visible_window(0, 3, 5), (0, 3));
    }
}
