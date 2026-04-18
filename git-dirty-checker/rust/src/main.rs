use clap::Parser;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use filetime::FileTime;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};
use rayon::prelude::*;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

/// Finds git repositories with uncommitted changes in subdirectories
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Run in interactive TUI mode
    #[clap(long)]
    interactive: bool,

    /// Directories to search for git repositories
    #[clap(required = true)]
    dirs: Vec<String>,
}

fn main() {
    let args = Args::parse();

    let repositories: Vec<PathBuf> = args
        .dirs
        .par_iter()
        .flat_map(|dir| find_subdirectories(dir))
        .collect();

    let mut dirty_repos: Vec<PathBuf> = repositories
        .par_iter()
        .filter(|repo| is_dirty_repository(repo))
        .filter_map(|repo| fs::canonicalize(repo).ok())
        .collect();

    dirty_repos.sort();
    dirty_repos.retain(|repo| !is_snoozed(repo));

    if args.interactive {
        if let Some(selected) = run_interactive(dirty_repos) {
            println!("{}", selected.display());
        }
    } else {
        for repo in dirty_repos {
            println!("{}", repo.display());
        }
    }
}

fn find_subdirectories(path: &str) -> Vec<PathBuf> {
    let dir_path = Path::new(path);

    if !dir_path.is_dir() {
        return Vec::new();
    }

    match fs::read_dir(dir_path) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_dir())
            .map(|entry| entry.path())
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn is_dirty_repository(path: &Path) -> bool {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("status")
        .arg("--porcelain")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();

    match output {
        Ok(output) => !output.stdout.is_empty(),
        Err(_) => false,
    }
}

// --- Snooze ---

fn snooze_dir() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    Some(
        home.join(".skagedal-tools")
            .join("git-dirty-checker")
            .join("snoozed"),
    )
}

fn repo_snooze_key(path: &Path) -> String {
    path.to_string_lossy().replace('/', "__")
}

fn is_snoozed(path: &Path) -> bool {
    let Some(dir) = snooze_dir() else {
        return false;
    };
    let snooze_file = dir.join(repo_snooze_key(path));
    match fs::metadata(&snooze_file) {
        Ok(metadata) => match metadata.modified() {
            Ok(mtime) => mtime > SystemTime::now(),
            Err(_) => false,
        },
        Err(_) => false,
    }
}

fn snooze_repo(path: &Path) {
    let Some(dir) = snooze_dir() else {
        return;
    };
    let _ = fs::create_dir_all(&dir);
    let snooze_file = dir.join(repo_snooze_key(path));
    let _ = fs::File::create(&snooze_file);
    let future_time = SystemTime::now()
        .checked_add(Duration::from_secs(3600))
        .unwrap();
    let _ = filetime::set_file_mtime(&snooze_file, FileTime::from_system_time(future_time));
}

// --- Interactive TUI ---

enum Mode {
    Normal,
    Search,
}

struct App {
    all_repos: Vec<PathBuf>,
    filtered_repos: Vec<PathBuf>,
    selected: usize,
    mode: Mode,
    query: String,
}

impl App {
    fn new(repos: Vec<PathBuf>) -> Self {
        let filtered = repos.clone();
        App {
            all_repos: repos,
            filtered_repos: filtered,
            selected: 0,
            mode: Mode::Normal,
            query: String::new(),
        }
    }

    fn update_filter(&mut self) {
        if self.query.is_empty() {
            self.filtered_repos = self.all_repos.clone();
        } else {
            let q = self.query.to_lowercase();
            self.filtered_repos = self
                .all_repos
                .iter()
                .filter(|p| p.to_string_lossy().to_lowercase().contains(&q))
                .cloned()
                .collect();
        }
        if self.selected >= self.filtered_repos.len() {
            self.selected = self.filtered_repos.len().saturating_sub(1);
        }
    }

    fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    fn move_down(&mut self) {
        if self.selected + 1 < self.filtered_repos.len() {
            self.selected += 1;
        }
    }

    fn selected_repo(&self) -> Option<&PathBuf> {
        self.filtered_repos.get(self.selected)
    }

    fn snooze_selected(&mut self) {
        if let Some(repo) = self.filtered_repos.get(self.selected).cloned() {
            snooze_repo(&repo);
            self.all_repos.retain(|r| r != &repo);
            self.update_filter();
        }
    }
}

fn run_interactive(repos: Vec<PathBuf>) -> Option<PathBuf> {
    if repos.is_empty() {
        return None;
    }

    enable_raw_mode().ok()?;
    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen).ok()?;
    let backend = CrosstermBackend::new(out);
    let mut terminal = Terminal::new(backend).ok()?;

    let mut app = App::new(repos);
    let result = run_app(&mut terminal, &mut app);

    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
    let _ = terminal.show_cursor();

    result.ok().flatten()
}

fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> io::Result<Option<PathBuf>> {
    loop {
        terminal.draw(|f| render(f, app))?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            match app.mode {
                Mode::Normal => match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => return Ok(None),
                    KeyCode::Enter => return Ok(app.selected_repo().cloned()),
                    KeyCode::Up | KeyCode::Char('k') => app.move_up(),
                    KeyCode::Down | KeyCode::Char('j') => app.move_down(),
                    KeyCode::Char('/') => app.mode = Mode::Search,
                    KeyCode::Char('s') => {
                        app.snooze_selected();
                        if app.filtered_repos.is_empty() {
                            return Ok(None);
                        }
                    }
                    _ => {}
                },
                Mode::Search => match key.code {
                    KeyCode::Esc => app.mode = Mode::Normal,
                    KeyCode::Enter => {
                        app.mode = Mode::Normal;
                        return Ok(app.selected_repo().cloned());
                    }
                    KeyCode::Backspace => {
                        app.query.pop();
                        app.update_filter();
                    }
                    KeyCode::Up => app.move_up(),
                    KeyCode::Down => app.move_down(),
                    KeyCode::Char(c) => {
                        app.query.push(c);
                        app.update_filter();
                    }
                    _ => {}
                },
            }
        }
    }
}

fn render(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(f.area());

    let header_text = match app.mode {
        Mode::Normal => "\u{2191}\u{2193}/jk: navigate  /: search  s: snooze 1h  Enter: select  q/Esc: quit",
        Mode::Search => "\u{2191}\u{2193}: navigate  Enter: select  Esc: back to normal",
    };
    f.render_widget(
        Paragraph::new(header_text).style(Style::default().fg(Color::DarkGray)),
        chunks[0],
    );

    let items: Vec<ListItem> = app
        .filtered_repos
        .iter()
        .map(|p| ListItem::new(p.to_string_lossy().to_string()))
        .collect();

    let list = List::new(items)
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    let mut list_state = ListState::default();
    if !app.filtered_repos.is_empty() {
        list_state.select(Some(app.selected));
    }
    f.render_stateful_widget(list, chunks[1], &mut list_state);

    let footer = match app.mode {
        Mode::Normal => Paragraph::new(format!("{} dirty repos", app.filtered_repos.len()))
            .style(Style::default().fg(Color::DarkGray)),
        Mode::Search => Paragraph::new(format!("/{}", app.query))
            .style(Style::default().fg(Color::White)),
    };
    f.render_widget(footer, chunks[2]);
}
