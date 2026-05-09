use std::io::Write;

use anyhow::Result;
use console::{Key, Term, measure_text_width, style};

use crate::git::{PrOptions, pr_create_args};

#[derive(Debug, Clone)]
pub struct PendingPr {
    pub repo_display: String,
    pub head: String,
    pub base: String,
}

pub enum PrOptionsOutcome {
    Confirmed(PrOptions),
    Cancelled,
}

struct State {
    options: PrOptions,
    cursor: usize,
    rendered_rows: usize,
}

const OPTIONS: &[&str] = &["Draft", "Open in browser"];

pub fn run(pending: &[PendingPr]) -> Result<PrOptionsOutcome> {
    if pending.is_empty() {
        return Ok(PrOptionsOutcome::Confirmed(PrOptions::default()));
    }

    let mut term = Term::stderr();

    let mut state = State {
        options: PrOptions {
            draft: true,
            web: false,
        },
        cursor: 0,
        rendered_rows: 0,
    };

    let _ = term.hide_cursor();

    let outcome = loop {
        render(&mut term, &mut state, pending)?;
        let key = match term.read_key() {
            Ok(key) => key,
            Err(_) => break PrOptionsOutcome::Cancelled,
        };
        match handle_key(&mut state, key) {
            KeyResult::Continue => {}
            KeyResult::Confirm => break PrOptionsOutcome::Confirmed(state.options),
            KeyResult::Cancel => break PrOptionsOutcome::Cancelled,
        }
    };

    clear_rendered(&mut term, state.rendered_rows)?;
    let _ = term.show_cursor();
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
            if state.cursor + 1 < OPTIONS.len() {
                state.cursor += 1;
            }
            KeyResult::Continue
        }
        Key::Char(' ') => {
            toggle(state);
            KeyResult::Continue
        }
        Key::Enter => KeyResult::Confirm,
        Key::Escape | Key::CtrlC | Key::Char('q') => KeyResult::Cancel,
        _ => KeyResult::Continue,
    }
}

fn toggle(state: &mut State) {
    // Draft and Web are mutually exclusive: turning one on turns the other off.
    match state.cursor {
        0 => {
            state.options.draft = !state.options.draft;
            if state.options.draft {
                state.options.web = false;
            }
        }
        1 => {
            state.options.web = !state.options.web;
            if state.options.web {
                state.options.draft = false;
            }
        }
        _ => {}
    }
}

fn render(term: &mut Term, state: &mut State, pending: &[PendingPr]) -> Result<()> {
    clear_rendered(term, state.rendered_rows)?;

    let term_width = term.size().1 as usize;
    let mut lines: Vec<String> = Vec::new();

    let plural = if pending.len() == 1 { "" } else { "s" };
    lines.push(format!(
        "{} {} pull request{} will be created:",
        style("Confirm:").bold(),
        pending.len(),
        plural
    ));

    for pp in pending {
        let args = pr_create_args(&pp.head, &pp.base, state.options);
        let cmd = std::iter::once("gh".to_string())
            .chain(args)
            .collect::<Vec<_>>()
            .join(" ");
        lines.push(format!(
            "  {}  {}",
            style(format!("[{}]", pp.repo_display)).dim(),
            cmd
        ));
    }

    lines.push(String::new());
    lines.push("Options:".to_string());

    let render_option = |idx: usize, on: bool, label: &str| -> String {
        let cursor_marker = if idx == state.cursor { ">" } else { " " };
        let check = if on { "[x]" } else { "[ ]" };
        let line = format!("{cursor_marker} {check} {label}");
        if idx == state.cursor {
            format!("{}", style(line).reverse())
        } else {
            line
        }
    };

    lines.push(render_option(0, state.options.draft, OPTIONS[0]));
    lines.push(render_option(1, state.options.web, OPTIONS[1]));

    lines.push(String::new());
    lines.push(
        "  \u{2191}/\u{2193} j/k navigate, space toggle, Enter confirm, Esc cancel".to_string(),
    );

    let mut total_rows = 0;
    for line in &lines {
        total_rows += rows_for_line(line, term_width);
        writeln!(term, "{line}")?;
    }

    state.rendered_rows = total_rows;
    Ok(())
}

fn rows_for_line(line: &str, term_width: usize) -> usize {
    if term_width == 0 {
        return 1;
    }
    let width = measure_text_width(line);
    if width == 0 {
        1
    } else {
        width.div_ceil(term_width)
    }
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

    fn fresh_state() -> State {
        State {
            options: PrOptions {
                draft: true,
                web: false,
            },
            cursor: 0,
            rendered_rows: 0,
        }
    }

    #[test]
    fn space_toggles_focused_option() {
        let mut s = fresh_state();
        let _ = handle_key(&mut s, Key::Char(' '));
        assert!(!s.options.draft);
        assert!(!s.options.web);

        let _ = handle_key(&mut s, Key::ArrowDown);
        let _ = handle_key(&mut s, Key::Char(' '));
        assert!(s.options.web);
        assert!(!s.options.draft);
    }

    #[test]
    fn enabling_web_clears_draft_and_vice_versa() {
        let mut s = fresh_state();
        // Default: draft on, web off. Move to web and turn it on.
        let _ = handle_key(&mut s, Key::ArrowDown);
        let _ = handle_key(&mut s, Key::Char(' '));
        assert!(s.options.web);
        assert!(!s.options.draft);

        // Move back to draft and turn it on; web should clear.
        let _ = handle_key(&mut s, Key::ArrowUp);
        let _ = handle_key(&mut s, Key::Char(' '));
        assert!(s.options.draft);
        assert!(!s.options.web);
    }

    #[test]
    fn cursor_clamps_to_options() {
        let mut s = fresh_state();
        let _ = handle_key(&mut s, Key::ArrowDown);
        let _ = handle_key(&mut s, Key::ArrowDown);
        assert_eq!(s.cursor, 1);
        let _ = handle_key(&mut s, Key::ArrowUp);
        let _ = handle_key(&mut s, Key::ArrowUp);
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn enter_confirms_and_escape_cancels() {
        let mut s = fresh_state();
        assert!(matches!(handle_key(&mut s, Key::Enter), KeyResult::Confirm));
        assert!(matches!(
            handle_key(&mut s, Key::Escape),
            KeyResult::Cancel
        ));
    }

    #[test]
    fn empty_pending_confirms_immediately() {
        match run(&[]).unwrap() {
            PrOptionsOutcome::Confirmed(opts) => {
                assert!(!opts.draft);
                assert!(!opts.web);
            }
            PrOptionsOutcome::Cancelled => panic!("expected confirmed"),
        }
    }
}
