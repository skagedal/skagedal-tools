use std::io::{self, Stdout};
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};

use crate::app::{App, COLUMNS, View};

type Backend = CrosstermBackend<Stdout>;

pub fn run(app: &mut App) -> Result<()> {
    let mut terminal = setup_terminal()?;
    let result = event_loop(&mut terminal, app);
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

fn event_loop(terminal: &mut Terminal<Backend>, app: &mut App) -> Result<()> {
    let mut table_state = TableState::default().with_selected(Some(app.selected));
    while !app.should_quit {
        table_state.select(Some(app.selected));
        terminal.draw(|f| draw(f, app, &mut table_state))?;
        if event::poll(Duration::from_millis(250))?
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
    match app.view {
        View::List => match key.code {
            KeyCode::Char('q') => app.quit(),
            KeyCode::Char('j') | KeyCode::Down => app.move_down(),
            KeyCode::Char('k') | KeyCode::Up => app.move_up(),
            KeyCode::Char('g') => app.jump_top(),
            KeyCode::Char('G') => app.jump_bottom(),
            KeyCode::Char('o') | KeyCode::Enter => app.open_detail(),
            _ => {}
        },
        View::Detail => match key.code {
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Char('u') => app.close_detail(),
            KeyCode::Char('j') | KeyCode::Down | KeyCode::Char('n') => {
                app.move_down();
            }
            KeyCode::Char('k') | KeyCode::Up | KeyCode::Char('p') => {
                app.move_up();
            }
            _ => {}
        },
    }
}

fn draw(f: &mut ratatui::Frame, app: &App, table_state: &mut TableState) {
    let chunks =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(f.area());

    match app.view {
        View::List => draw_list(f, app, chunks[0], table_state),
        View::Detail => draw_detail(f, app, chunks[0]),
    }

    let status = match app.view {
        View::List => format!(
            " {}  {}/{}   j/k move · g/G top/bottom · o open · q quit ",
            app.source_label,
            app.selected.saturating_add(1).min(app.entries.len().max(1)),
            app.entries.len()
        ),
        View::Detail => format!(
            " {}  entry {}/{}   j/k or n/p navigate · u/Esc/q back ",
            app.source_label,
            app.selected + 1,
            app.entries.len()
        ),
    };
    f.render_widget(Paragraph::new(status), chunks[1]);
}

fn draw_list(
    f: &mut ratatui::Frame,
    app: &App,
    area: ratatui::layout::Rect,
    table_state: &mut TableState,
) {
    let header = Row::new(COLUMNS.iter().map(|c| Cell::from(c.name)))
        .style(Style::default().add_modifier(Modifier::BOLD));

    let rows = app
        .entries
        .iter()
        .map(|e| Row::new(app.row_cells(e).into_iter().map(Cell::from)));

    let widths = [
        Constraint::Length(20),
        Constraint::Length(8),
        Constraint::Min(10),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" rust-log-viewer "),
        );

    f.render_stateful_widget(table, area, table_state);
}

fn draw_detail(f: &mut ratatui::Frame, app: &App, area: ratatui::layout::Rect) {
    let body = match app.selected_entry() {
        Some(e) => serde_json::to_string_pretty(&e.value).unwrap_or_else(|_| e.raw.clone()),
        None => String::new(),
    };
    let lines: Vec<Line> = body.lines().map(Line::raw).collect();
    let para = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" entry detail "),
    );
    f.render_widget(para, area);
}
