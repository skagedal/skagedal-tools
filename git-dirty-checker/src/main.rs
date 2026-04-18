use clap::Parser;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use filetime::FileTime;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{List, ListItem, ListState, Paragraph},
    Frame, Terminal, TerminalOptions, Viewport,
};
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

/// Finds git repositories with uncommitted changes in subdirectories
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Run in interactive mode
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

    if args.interactive {
        if let Some(selected) = run_interactive(dirty_repos) {
            println!("{}", selected.display());
        }
    } else {
        for repo in &dirty_repos {
            if !is_snoozed(repo) {
                println!("{}", repo.display());
            }
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
    get_snooze_expiry(path).is_some()
}

fn get_snooze_expiry(path: &Path) -> Option<SystemTime> {
    let dir = snooze_dir()?;
    let snooze_file = dir.join(repo_snooze_key(path));
    let metadata = fs::metadata(&snooze_file).ok()?;
    let mtime = metadata.modified().ok()?;
    if mtime > SystemTime::now() {
        Some(mtime)
    } else {
        None
    }
}

fn snooze_repo(path: &Path) {
    let Some(dir) = snooze_dir() else {
        return;
    };
    let _ = fs::create_dir_all(&dir);
    let snooze_file = dir.join(repo_snooze_key(path));
    let _ = fs::File::create(&snooze_file);
    // Extend from current expiry if already snoozed, otherwise from now
    let base_time = get_snooze_expiry(path).unwrap_or_else(SystemTime::now);
    let future_time = base_time.checked_add(Duration::from_secs(3600)).unwrap();
    let _ = filetime::set_file_mtime(&snooze_file, FileTime::from_system_time(future_time));
}

fn unsnooze_repo(path: &Path) {
    let Some(dir) = snooze_dir() else {
        return;
    };
    let snooze_file = dir.join(repo_snooze_key(path));
    let _ = fs::remove_file(snooze_file);
}

// --- Interactive mode ---

enum Mode {
    Normal,
    Search,
}

struct App {
    all_repos: Vec<PathBuf>,
    filtered_repos: Vec<PathBuf>,
    snoozed: HashMap<PathBuf, SystemTime>,
    selected: usize,
    mode: Mode,
    query: String,
}

impl App {
    fn new(repos: Vec<PathBuf>) -> Self {
        // repos is already sorted alphabetically
        let mut snoozed: HashMap<PathBuf, SystemTime> = HashMap::new();
        for repo in &repos {
            if let Some(expiry) = get_snooze_expiry(repo) {
                snoozed.insert(repo.clone(), expiry);
            }
        }

        // Unsnoozed repos first, snoozed repos at bottom (preserving alphabetical order within each group)
        let mut ordered: Vec<PathBuf> = repos
            .iter()
            .filter(|r| !snoozed.contains_key(*r))
            .cloned()
            .collect();
        ordered.extend(repos.iter().filter(|r| snoozed.contains_key(*r)).cloned());

        let filtered = ordered.clone();
        App {
            all_repos: ordered,
            filtered_repos: filtered,
            snoozed,
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
            if let Some(expiry) = get_snooze_expiry(&repo) {
                self.snoozed.insert(repo, expiry);
            }
        }
    }

    fn unsnooze_selected(&mut self) {
        if let Some(repo) = self.filtered_repos.get(self.selected).cloned() {
            unsnooze_repo(&repo);
            self.snoozed.remove(&repo);
        }
    }
}

fn format_snooze_remaining(expiry: &SystemTime) -> String {
    let now = SystemTime::now();
    match expiry.duration_since(now) {
        Ok(remaining) => {
            let total_secs = remaining.as_secs();
            let hours = total_secs / 3600;
            let minutes = (total_secs % 3600) / 60;
            if hours > 0 {
                format!("{}h{}m", hours, minutes)
            } else {
                format!("{}m", minutes)
            }
        }
        Err(_) => "expired".to_string(),
    }
}

fn run_interactive(repos: Vec<PathBuf>) -> Option<PathBuf> {
    if repos.is_empty() {
        return None;
    }

    let (_, terminal_rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let max_height = (terminal_rows as usize).saturating_sub(2).max(3);
    let height = (repos.len() + 2).min(max_height) as u16;

    enable_raw_mode().ok()?;
    let stdout = io::stdout();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Inline(height),
        },
    )
    .ok()?;

    let mut app = App::new(repos);
    let result = run_app(&mut terminal, &mut app);

    let _ = disable_raw_mode();

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
                    KeyCode::Char('s') => app.snooze_selected(),
                    KeyCode::Char('u') => app.unsnooze_selected(),
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
        Mode::Normal => "↑↓/jk: navigate  /: search  s: snooze +1h  u: unsnooze  Enter: select  q/Esc: quit",
        Mode::Search => "↑↓: navigate  Enter: select  Esc: back to normal",
    };
    f.render_widget(
        Paragraph::new(header_text).style(Style::default().fg(Color::DarkGray)),
        chunks[0],
    );

    let items: Vec<ListItem> = app
        .filtered_repos
        .iter()
        .map(|p| {
            if let Some(expiry) = app.snoozed.get(p) {
                let remaining = format_snooze_remaining(expiry);
                ListItem::new(format!(
                    "{} (snoozed {})",
                    p.to_string_lossy(),
                    remaining
                ))
                .style(Style::default().fg(Color::DarkGray))
            } else {
                ListItem::new(p.to_string_lossy().to_string())
            }
        })
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
        Mode::Normal => {
            let snoozed_count = app
                .filtered_repos
                .iter()
                .filter(|r| app.snoozed.contains_key(*r))
                .count();
            let text = if snoozed_count > 0 {
                format!(
                    "{} dirty repos ({} snoozed)",
                    app.filtered_repos.len(),
                    snoozed_count
                )
            } else {
                format!("{} dirty repos", app.filtered_repos.len())
            };
            Paragraph::new(text).style(Style::default().fg(Color::DarkGray))
        }
        Mode::Search => Paragraph::new(format!("/{}", app.query))
            .style(Style::default().fg(Color::White)),
    };
    f.render_widget(footer, chunks[2]);
}
