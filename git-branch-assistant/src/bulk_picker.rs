use std::io::Write;
use std::sync::mpsc;
use std::thread;

use anyhow::Result;
use console::{Key, Term, style};

pub fn stderr_is_terminal() -> bool {
    Term::stderr().is_term()
}

pub enum BulkOutcome {
    Confirmed {
        selected_indices: Vec<usize>,
        action_index: usize,
    },
    Cancelled,
}

struct State {
    title: String,
    entries: Vec<String>,
    actions: Vec<String>,
    checked: Vec<bool>,
    cursor: usize,
    action_index: usize,
    rendered_rows: usize,
}

pub fn run(title: &str, entries: &[String], actions: &[String]) -> Result<BulkOutcome> {
    if entries.is_empty() || actions.is_empty() {
        return Ok(BulkOutcome::Cancelled);
    }

    let mut term = Term::stderr();
    let (tx, rx) = mpsc::channel::<Key>();
    let key_term = Term::stderr();
    let handle = thread::spawn(move || {
        while let Ok(key) = key_term.read_key() {
            if tx.send(key).is_err() {
                break;
            }
        }
    });

    let mut state = State {
        title: title.to_string(),
        entries: entries.to_vec(),
        actions: actions.to_vec(),
        checked: vec![true; entries.len()],
        cursor: 0,
        action_index: 0,
        rendered_rows: 0,
    };

    let _ = term.hide_cursor();

    let outcome = loop {
        render(&mut term, &mut state)?;
        let key = match rx.recv() {
            Ok(key) => key,
            Err(_) => break BulkOutcome::Cancelled,
        };
        match handle_key(&mut state, key) {
            KeyResult::Continue => {}
            KeyResult::Confirm => {
                let selected_indices: Vec<usize> = state
                    .checked
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, &checked)| if checked { Some(idx) } else { None })
                    .collect();
                break BulkOutcome::Confirmed {
                    selected_indices,
                    action_index: state.action_index,
                };
            }
            KeyResult::Cancel => break BulkOutcome::Cancelled,
        }
    };

    clear_rendered(&mut term, state.rendered_rows)?;
    let _ = term.show_cursor();
    drop(rx);
    let _ = handle.join();
    Ok(outcome)
}

enum KeyResult {
    Continue,
    Confirm,
    Cancel,
}

fn handle_key(state: &mut State, key: Key) -> KeyResult {
    match key {
        Key::ArrowUp | Key::Char('k') => {
            if state.cursor > 0 {
                state.cursor -= 1;
            }
            KeyResult::Continue
        }
        Key::ArrowDown | Key::Char('j') => {
            if state.cursor + 1 < state.entries.len() {
                state.cursor += 1;
            }
            KeyResult::Continue
        }
        Key::ArrowLeft | Key::Char('h') => {
            if state.action_index > 0 {
                state.action_index -= 1;
            }
            KeyResult::Continue
        }
        Key::ArrowRight | Key::Char('l') => {
            if state.action_index + 1 < state.actions.len() {
                state.action_index += 1;
            }
            KeyResult::Continue
        }
        Key::Char(' ') => {
            if let Some(slot) = state.checked.get_mut(state.cursor) {
                *slot = !*slot;
            }
            KeyResult::Continue
        }
        Key::Char('a') => {
            let all_selected = state.checked.iter().all(|c| *c);
            for c in state.checked.iter_mut() {
                *c = !all_selected;
            }
            KeyResult::Continue
        }
        Key::Enter => KeyResult::Confirm,
        Key::Escape | Key::CtrlC | Key::Char('q') => KeyResult::Cancel,
        _ => KeyResult::Continue,
    }
}

fn render(term: &mut Term, state: &mut State) -> Result<()> {
    clear_rendered(term, state.rendered_rows)?;

    let selected_count = state.checked.iter().filter(|c| **c).count();
    writeln!(
        term,
        "{} ({}/{} selected)",
        style(&state.title).bold(),
        selected_count,
        state.entries.len()
    )?;
    writeln!(
        term,
        "  \u{2191}/\u{2193} j/k navigate, space toggle, a toggle all, \u{2190}/\u{2192} h/l action, Enter confirm, Esc cancel"
    )?;

    let height = term.size().0 as usize;
    let max_visible = height.saturating_sub(5).max(1);
    let (top, bottom) = visible_window(state.cursor, state.entries.len(), max_visible);

    for idx in top..bottom {
        let cursor_marker = if idx == state.cursor { ">" } else { " " };
        let check = if state.checked[idx] { "[x]" } else { "[ ]" };
        let line = format!("{cursor_marker} {check} {}", state.entries[idx]);
        if idx == state.cursor {
            writeln!(term, "{}", style(line).reverse())?;
        } else {
            writeln!(term, "{line}")?;
        }
    }

    let mut action_line = String::new();
    for (idx, action) in state.actions.iter().enumerate() {
        if idx > 0 {
            action_line.push_str("  ");
        }
        if idx == state.action_index {
            action_line.push_str(&format!(
                "{}",
                style(format!("[{action}]")).reverse().bold()
            ));
        } else {
            action_line.push_str(&format!(" {action} "));
        }
    }
    writeln!(term, "{action_line}")?;

    state.rendered_rows = 3 + (bottom - top);
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

    fn state(entries: usize, actions: usize) -> State {
        State {
            title: "test".into(),
            entries: (0..entries).map(|i| format!("entry-{i}")).collect(),
            actions: (0..actions).map(|i| format!("action-{i}")).collect(),
            checked: vec![true; entries],
            cursor: 0,
            action_index: 0,
            rendered_rows: 0,
        }
    }

    #[test]
    fn space_toggles_current_entry() {
        let mut s = state(3, 2);
        assert!(s.checked[0]);
        let _ = handle_key(&mut s, Key::Char(' '));
        assert!(!s.checked[0]);
        let _ = handle_key(&mut s, Key::Char(' '));
        assert!(s.checked[0]);
    }

    #[test]
    fn arrow_keys_move_cursor_within_bounds() {
        let mut s = state(3, 2);
        let _ = handle_key(&mut s, Key::ArrowDown);
        assert_eq!(s.cursor, 1);
        let _ = handle_key(&mut s, Key::ArrowDown);
        assert_eq!(s.cursor, 2);
        let _ = handle_key(&mut s, Key::ArrowDown);
        assert_eq!(s.cursor, 2);
        let _ = handle_key(&mut s, Key::ArrowUp);
        assert_eq!(s.cursor, 1);
    }

    #[test]
    fn horizontal_keys_move_action_within_bounds() {
        let mut s = state(2, 3);
        let _ = handle_key(&mut s, Key::ArrowRight);
        assert_eq!(s.action_index, 1);
        let _ = handle_key(&mut s, Key::Char('l'));
        assert_eq!(s.action_index, 2);
        let _ = handle_key(&mut s, Key::ArrowRight);
        assert_eq!(s.action_index, 2);
        let _ = handle_key(&mut s, Key::Char('h'));
        assert_eq!(s.action_index, 1);
    }

    #[test]
    fn jk_navigates() {
        let mut s = state(3, 2);
        let _ = handle_key(&mut s, Key::Char('j'));
        assert_eq!(s.cursor, 1);
        let _ = handle_key(&mut s, Key::Char('k'));
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn toggle_all_inverts_when_all_selected() {
        let mut s = state(3, 2);
        let _ = handle_key(&mut s, Key::Char('a'));
        assert!(s.checked.iter().all(|c| !c));
        let _ = handle_key(&mut s, Key::Char('a'));
        assert!(s.checked.iter().all(|c| *c));
    }

    #[test]
    fn enter_yields_confirm() {
        let mut s = state(2, 2);
        assert!(matches!(handle_key(&mut s, Key::Enter), KeyResult::Confirm));
    }

    #[test]
    fn escape_yields_cancel() {
        let mut s = state(2, 2);
        assert!(matches!(handle_key(&mut s, Key::Escape), KeyResult::Cancel));
    }

    #[test]
    fn visible_window_centers_on_cursor() {
        assert_eq!(visible_window(0, 10, 5), (0, 5));
        assert_eq!(visible_window(5, 10, 5), (3, 8));
        assert_eq!(visible_window(9, 10, 5), (5, 10));
    }
}
