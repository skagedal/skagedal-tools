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

        table_state.select(Some(app.selected));
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
    match app.view {
        View::List => handle_list_key(app, key),
        View::Detail => handle_detail_key(app, key),
        View::FieldsMenu => handle_fields_menu_key(app, key),
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
    let chunks = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(f.area());

    match app.view {
        View::List => draw_list(f, app, chunks[0], table_state),
        View::Detail => draw_detail(f, app, chunks[0]),
        View::FieldsMenu => draw_fields_menu(f, app, chunks[0], menu_state),
    }
    draw_status(f, app, chunks[1]);
}

fn draw_list(f: &mut ratatui::Frame, app: &App, area: Rect, state: &mut TableState) {
    let cols = app.visible_columns();
    let header = Row::new(cols.iter().map(|c| Cell::from(c.name.clone())))
        .style(Style::default().add_modifier(Modifier::BOLD));
    let rows = app
        .entries
        .iter()
        .map(|e| Row::new(app.row_cells(e).into_iter().map(Cell::from)));
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
    let pos = if total == 0 {
        0
    } else {
        app.selected.saturating_add(1).min(total)
    };
    let follow = if app.follow { " · FOLLOW" } else { "" };
    let extra = app
        .status_msg
        .as_deref()
        .map(|m| format!(" · {m}"))
        .unwrap_or_default();
    let bindings = match app.view {
        View::List => " j/k move · g/G top/bottom · o open · f follow · v fields · q quit ",
        View::Detail => " j/k field · n/p entry · t toggle col · c copy · u/Esc back ",
        View::FieldsMenu => " j/k cursor · space toggle · J/K reorder · v close ",
    };
    let text = format!("{pos}/{total}{follow}{extra} ·{bindings}");
    f.render_widget(Paragraph::new(text), area);
}
