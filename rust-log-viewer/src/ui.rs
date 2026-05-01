use std::io::{self, Stdout, Write};
use std::time::Duration;

use anyhow::Result;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Cell, List, ListItem, ListState, Paragraph, Row, Table, TableState};

use crate::app::{App, View};
use crate::source::EntryStream;
use crate::triggers::TriggerRuntime;

type Backend = CrosstermBackend<Stdout>;

pub fn run(app: &mut App, stream: EntryStream, mut triggers: TriggerRuntime) -> Result<()> {
    let mut terminal = setup_terminal()?;
    let result = event_loop(&mut terminal, app, stream, &mut triggers);
    restore_terminal(&mut terminal)?;
    result
}

fn setup_terminal() -> Result<Terminal<Backend>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    Ok(Terminal::new(CrosstermBackend::new(stdout))?)
}

fn restore_terminal(terminal: &mut Terminal<Backend>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn event_loop(
    terminal: &mut Terminal<Backend>,
    app: &mut App,
    stream: EntryStream,
    triggers: &mut TriggerRuntime,
) -> Result<()> {
    let mut table_state = TableState::default();
    let mut menu_state = ListState::default();
    while !app.should_quit {
        // Drain new entries from the producer thread, run triggers on each.
        let new = stream.drain();
        if !new.is_empty() {
            if !triggers.is_empty() {
                for e in &new {
                    triggers.handle(e);
                }
            }
            app.append_entries(new);
        }

        table_state.select(Some(app.cursor_in_filtered()));
        menu_state.select(Some(app.fields_menu.cursor));
        terminal.draw(|f| draw(f, app, &mut table_state, &mut menu_state))?;

        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            handle_key(app, key);
        }
    }
    Ok(())
}

fn handle_key(app: &mut App, key: KeyEvent) {
    if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
        app.quit();
        return;
    }
    app.status_msg = None;
    if app.filtering {
        handle_filter_key(app, key);
        return;
    }
    match app.view {
        View::List => handle_list_key(app, key),
        View::Detail => handle_detail_key(app, key),
        View::FieldsMenu => handle_fields_menu_key(app, key),
    }
}

fn handle_filter_key(app: &mut App, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match (key.code, ctrl) {
        (KeyCode::Esc, _) | (KeyCode::Enter, _) => app.exit_filter(),
        (KeyCode::Backspace, _) => app.filter_backspace(),
        (KeyCode::Char('w'), true) => app.filter_kill_word(),
        (KeyCode::Char('u'), true) => app.filter_clear(),
        (KeyCode::Char(c), false) if !c.is_control() => app.filter_push(c),
        _ => {}
    }
}

fn handle_list_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('q') => app.quit(),
        KeyCode::Char('j') | KeyCode::Down => app.move_down(),
        KeyCode::Char('k') | KeyCode::Up => app.move_up(),
        KeyCode::Char('u') => app.move_up(),
        KeyCode::Char('g') => app.jump_top(),
        KeyCode::Char('G') => app.jump_bottom(),
        KeyCode::Char('o') | KeyCode::Enter => app.open_detail(),
        KeyCode::Char('f') => app.toggle_follow(),
        KeyCode::Char('v') => app.open_fields_menu(),
        KeyCode::Char('/') => app.enter_filter(),
        KeyCode::Esc if !app.filter_text.is_empty() => app.filter_clear(),
        _ => {}
    }
}

fn handle_detail_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc | KeyCode::Char('u') => app.close_detail(),
        KeyCode::Char('j') | KeyCode::Down => app.detail_move_down(),
        KeyCode::Char('k') | KeyCode::Up => app.detail_move_up(),
        KeyCode::Char('n') => app.detail_next_entry(),
        KeyCode::Char('p') => app.detail_prev_entry(),
        KeyCode::Char('t') => app.detail_toggle_column(),
        KeyCode::Char('v') => app.open_fields_menu(),
        KeyCode::Char('c') => copy_detail(app),
        _ => {}
    }
}

fn handle_fields_menu_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc | KeyCode::Char('v') => app.close_fields_menu(),
        KeyCode::Char('j') | KeyCode::Down => app.fields_menu_move(1),
        KeyCode::Char('k') | KeyCode::Up | KeyCode::Char('u') => app.fields_menu_move(-1),
        KeyCode::Char(' ') | KeyCode::Char('t') => app.fields_menu_toggle(),
        KeyCode::Char('J') => app.fields_menu_swap(1),
        KeyCode::Char('K') => app.fields_menu_swap(-1),
        _ => {}
    }
}

fn copy_detail(app: &mut App) {
    let text = match app.detail_field() {
        Some((_, value)) => value,
        None => app
            .selected_entry()
            .map(|e| e.raw.clone())
            .unwrap_or_default(),
    };
    if text.is_empty() {
        return;
    }
    if osc52_copy(&text).is_ok() {
        app.set_status(format!("copied {} bytes", text.len()));
    } else {
        app.set_status("copy failed");
    }
}

fn osc52_copy(text: &str) -> io::Result<()> {
    let encoded = STANDARD.encode(text);
    let payload = format!("\x1b]52;c;{encoded}\x07");
    let mut out = io::stdout().lock();
    out.write_all(payload.as_bytes())?;
    out.flush()
}

fn draw(
    f: &mut ratatui::Frame,
    app: &App,
    table_state: &mut TableState,
    menu_state: &mut ListState,
) {
    let show_filter_bar = app.filtering || !app.filter_text.is_empty();
    let constraints = if show_filter_bar {
        vec![
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(1),
        ]
    } else {
        vec![Constraint::Min(0), Constraint::Length(1)]
    };
    let chunks = Layout::vertical(constraints).split(f.area());

    match app.view {
        View::List => draw_list(f, app, chunks[0], table_state),
        View::Detail => draw_detail(f, app, chunks[0]),
        View::FieldsMenu => draw_fields_menu(f, app, chunks[0], menu_state),
    }
    if show_filter_bar {
        draw_filter_bar(f, app, chunks[1]);
        draw_status(f, app, chunks[2]);
    } else {
        draw_status(f, app, chunks[1]);
    }
}

fn draw_filter_bar(f: &mut ratatui::Frame, app: &App, area: Rect) {
    let cursor = if app.filtering { "│" } else { "" };
    let label = if app.filtering {
        format!("/{}{cursor}", app.filter_text)
    } else {
        format!("/{} (Esc to clear, / to edit)", app.filter_text)
    };
    let style = if app.filtering {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default().add_modifier(Modifier::DIM)
    };
    f.render_widget(Paragraph::new(label).style(style), area);
}

fn draw_list(f: &mut ratatui::Frame, app: &App, area: Rect, state: &mut TableState) {
    let cols = app.visible_columns();
    let header = Row::new(cols.iter().map(|c| Cell::from(c.name.clone())))
        .style(Style::default().add_modifier(Modifier::BOLD));
    let displayed = app.filtered_indices();
    let rows = displayed
        .iter()
        .map(|&i| Row::new(app.row_cells(&app.entries[i]).into_iter().map(Cell::from)));
    let widths = column_widths(&cols);
    let table = Table::new(rows, widths)
        .header(header)
        .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" rust-log-viewer — {} ", app.source_label)),
        );
    f.render_stateful_widget(table, area, state);
}

fn column_widths(cols: &[&crate::app::ColumnState]) -> Vec<Constraint> {
    if cols.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(cols.len());
    for (i, _) in cols.iter().enumerate() {
        if i + 1 == cols.len() {
            out.push(Constraint::Min(10));
        } else {
            out.push(Constraint::Length(20));
        }
    }
    out
}

fn draw_detail(f: &mut ratatui::Frame, app: &App, area: Rect) {
    let entry = match app.selected_entry() {
        Some(e) => e,
        None => return,
    };
    let mut lines: Vec<Line> = Vec::new();
    let header_style = if app.detail.row == 0 {
        Style::default().add_modifier(Modifier::REVERSED)
    } else {
        Style::default().add_modifier(Modifier::BOLD)
    };
    lines.push(Line::styled(
        format!("entry {} of {}", app.selected + 1, app.entries.len()),
        header_style,
    ));
    for (i, key) in entry.keys().iter().enumerate() {
        let value = entry.get_str(key).unwrap_or_default();
        let style = if app.detail.row == i + 1 {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        lines.push(Line::styled(format!("  {key}: {value}"), style));
    }
    lines.push(Line::raw(""));
    lines.push(Line::raw("--- raw ---"));
    if let Ok(pretty) = serde_json::to_string_pretty(&entry.value) {
        for line in pretty.lines() {
            lines.push(Line::raw(line.to_string()));
        }
    }
    let para = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" entry detail "),
    );
    f.render_widget(para, area);
}

fn draw_fields_menu(
    f: &mut ratatui::Frame,
    app: &App,
    area: Rect,
    state: &mut ListState,
) {
    let items: Vec<ListItem> = app
        .columns
        .iter()
        .map(|c| {
            let mark = if c.visible { "[x]" } else { "[ ]" };
            ListItem::new(format!("{mark} {}", c.name))
        })
        .collect();
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" fields — space/t toggle · J/K reorder · v/q close "),
        )
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    f.render_stateful_widget(list, area, state);
}

fn draw_status(f: &mut ratatui::Frame, app: &App, area: Rect) {
    let total = app.entries.len();
    let displayed = app.displayed_count();
    let pos = if displayed == 0 {
        0
    } else {
        app.cursor_in_filtered() + 1
    };
    let filter_tag = if app.filter_text.is_empty() {
        String::new()
    } else {
        format!(" · {displayed}/{total} match")
    };
    let follow = if app.follow { " · FOLLOW" } else { "" };
    let extra = app
        .status_msg
        .as_deref()
        .map(|m| format!(" · {m}"))
        .unwrap_or_default();
    let bindings = match app.view {
        View::List => " j/k move · g/G top/bottom · o open · / filter · f follow · v fields · q quit ",
        View::Detail => " j/k field · n/p entry · t toggle col · c copy · u/Esc back ",
        View::FieldsMenu => " j/k cursor · space toggle · J/K reorder · v close ",
    };
    let text = format!("{pos}/{displayed}{filter_tag}{follow}{extra} ·{bindings}");
    f.render_widget(Paragraph::new(text), area);
}
