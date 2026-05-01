//! iced-based GUI front-end. Compiled in only when the `gui` feature is on.

use std::time::Duration;

use anyhow::Result;
use iced::widget::{Column, Row, button, container, scrollable, text, text_input};
use iced::{Element, Font, Length, Subscription, Task, Theme, keyboard};

use crate::app::App;
use crate::source::EntryStream;
use crate::triggers::TriggerRuntime;

struct State {
    app: App,
    stream: EntryStream,
    triggers: TriggerRuntime,
}

#[derive(Debug, Clone)]
enum Message {
    Tick,
    SelectEntry(usize),
    KeyDown,
    KeyUp,
    JumpTop,
    JumpBottom,
    OpenDetail,
    CloseDetail,
    ToggleFollow,
    ToggleColumn(usize),
    FilterChanged(String),
    FilterFocus,
    FilterClear,
    Quit,
}

const FILTER_INPUT_ID: &str = "filter-input";

const MONO: Font = Font::MONOSPACE;

pub fn run(app: App, stream: EntryStream, triggers: TriggerRuntime) -> Result<()> {
    let title_label = format!("rust-log-viewer — {}", app.source_label);
    let initial = State {
        app,
        stream,
        triggers,
    };
    iced::application(move |_: &State| title_label.clone(), update, view)
        .subscription(subscription)
        .theme(|_| Theme::Dark)
        .run_with(|| (initial, Task::none()))
        .map_err(|e| anyhow::anyhow!("{e}"))
}

fn subscription(_: &State) -> Subscription<Message> {
    Subscription::batch([
        iced::time::every(Duration::from_millis(100)).map(|_| Message::Tick),
        keyboard::on_key_press(|key, _modifiers| match key {
            keyboard::Key::Character(c) => match c.as_str() {
                "j" => Some(Message::KeyDown),
                "k" => Some(Message::KeyUp),
                "g" => Some(Message::JumpTop),
                "G" => Some(Message::JumpBottom),
                "o" => Some(Message::OpenDetail),
                "f" => Some(Message::ToggleFollow),
                "q" => Some(Message::Quit),
                "/" => Some(Message::FilterFocus),
                _ => None,
            },
            keyboard::Key::Named(named) => match named {
                keyboard::key::Named::ArrowDown => Some(Message::KeyDown),
                keyboard::key::Named::ArrowUp => Some(Message::KeyUp),
                keyboard::key::Named::Enter => Some(Message::OpenDetail),
                keyboard::key::Named::Escape => Some(Message::CloseDetail),
                _ => None,
            },
            _ => None,
        }),
    ])
}

fn update(state: &mut State, msg: Message) -> Task<Message> {
    match msg {
        Message::Tick => drain_stream(state),
        Message::KeyDown => state.app.move_down(),
        Message::KeyUp => state.app.move_up(),
        Message::JumpTop => state.app.jump_top(),
        Message::JumpBottom => state.app.jump_bottom(),
        Message::OpenDetail => state.app.open_detail(),
        Message::CloseDetail => {
            if !state.app.filter_text.is_empty()
                && matches!(state.app.view, crate::app::View::List)
            {
                state.app.filter_clear();
            } else {
                state.app.close_detail();
            }
        }
        Message::ToggleFollow => state.app.toggle_follow(),
        Message::SelectEntry(i) => {
            state.app.follow = false;
            if i < state.app.entries.len() {
                state.app.selected = i;
            }
        }
        Message::ToggleColumn(i) => {
            if let Some(c) = state.app.columns.get_mut(i) {
                c.visible = !c.visible;
            }
        }
        Message::FilterChanged(text) => state.app.set_filter(text),
        Message::FilterFocus => {
            return text_input::focus(text_input::Id::new(FILTER_INPUT_ID));
        }
        Message::FilterClear => state.app.filter_clear(),
        Message::Quit => return iced::exit(),
    }
    Task::none()
}

fn drain_stream(state: &mut State) {
    let new = state.stream.drain();
    if new.is_empty() {
        return;
    }
    if !state.triggers.is_empty() {
        for e in &new {
            state.triggers.handle(e);
        }
    }
    state.app.append_entries(new);
}

fn view(state: &State) -> Element<'_, Message> {
    let header = header_row(state);
    let filter = filter_bar(state);
    let body: Element<'_, Message> = if matches!(state.app.view, crate::app::View::Detail) {
        detail_view(state)
    } else {
        list_view(state)
    };
    let status = status_bar(state);
    Column::new()
        .push(header)
        .push(filter)
        .push(body)
        .push(status)
        .spacing(4)
        .padding(8)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn filter_bar(state: &State) -> Element<'_, Message> {
    let input = text_input("filter (fuzzy) — press / to focus", &state.app.filter_text)
        .id(text_input::Id::new(FILTER_INPUT_ID))
        .on_input(Message::FilterChanged)
        .padding(4)
        .size(13);
    let mut row = Row::new().spacing(8).push(input);
    if !state.app.filter_text.is_empty() {
        row = row.push(button(text("Clear").size(12)).on_press(Message::FilterClear));
    }
    container(row).width(Length::Fill).into()
}

fn header_row(state: &State) -> Element<'_, Message> {
    let mut row = Row::new().spacing(8);
    for (i, col) in state.app.columns.iter().enumerate() {
        let label = format!(
            "{} {}",
            if col.visible { "[x]" } else { "[ ]" },
            col.name
        );
        row = row.push(button(text(label).size(12)).on_press(Message::ToggleColumn(i)));
    }
    container(row).width(Length::Fill).into()
}

fn list_view(state: &State) -> Element<'_, Message> {
    let cols = state.app.visible_columns();
    let header = Row::with_children(cols.iter().map(|c| {
        text(c.name.clone())
            .font(MONO)
            .size(13)
            .width(column_width(c))
            .into()
    }))
    .spacing(8);

    let displayed = state.app.filtered_indices();
    let rows = displayed.iter().map(|&i| {
        let entry = &state.app.entries[i];
        let cells = state.app.row_cells(entry);
        let row_widget = Row::with_children(cells.iter().enumerate().map(|(ci, cell)| {
            let col = cols[ci];
            text(truncate(cell, 200))
                .font(MONO)
                .size(12)
                .width(column_width(col))
                .into()
        }))
        .spacing(8);
        let style = if i == state.app.selected {
            container_selected
        } else {
            container_normal
        };
        let inner = container(row_widget)
            .padding([2, 4])
            .width(Length::Fill)
            .style(style);
        button(inner)
            .on_press(Message::SelectEntry(i))
            .style(button::text)
            .padding(0)
            .into()
    });

    let body = Column::with_children(rows).spacing(0);
    Column::new()
        .push(container(header).padding([2, 4]))
        .push(scrollable(body).height(Length::Fill).width(Length::Fill))
        .height(Length::Fill)
        .into()
}

fn detail_view(state: &State) -> Element<'_, Message> {
    let Some(entry) = state.app.selected_entry() else {
        return container(text("(no entry)")).into();
    };
    let mut col = Column::new().spacing(2);
    col = col.push(
        text(format!(
            "entry {} of {}",
            state.app.selected + 1,
            state.app.entries.len()
        ))
        .size(14),
    );
    for key in entry.keys() {
        let value = entry.get_str(&key).unwrap_or_default();
        col = col.push(
            text(format!("{key}: {value}"))
                .font(MONO)
                .size(12),
        );
    }
    col = col.push(text("").size(8));
    col = col.push(text("--- raw ---").size(12));
    if let Ok(pretty) = serde_json::to_string_pretty(&entry.value) {
        col = col.push(text(pretty).font(MONO).size(11));
    }
    let close = button(text("Close (Esc)").size(12)).on_press(Message::CloseDetail);
    Column::new()
        .push(close)
        .push(scrollable(col).height(Length::Fill).width(Length::Fill))
        .spacing(6)
        .height(Length::Fill)
        .into()
}

fn status_bar(state: &State) -> Element<'_, Message> {
    let total = state.app.entries.len();
    let displayed = state.app.displayed_count();
    let pos = if displayed == 0 {
        0
    } else {
        state.app.cursor_in_filtered() + 1
    };
    let filter_tag = if state.app.filter_text.is_empty() {
        String::new()
    } else {
        format!(" · {displayed}/{total} match")
    };
    let follow = if state.app.follow { " · FOLLOW" } else { "" };
    let label = format!(
        "{pos}/{displayed}{filter_tag}{follow} · {} · j/k move · / filter · o open · f follow · q quit",
        state.app.source_label
    );
    container(text(label).size(11))
        .width(Length::Fill)
        .into()
}

fn column_width(col: &crate::app::ColumnState) -> Length {
    if col.name == "message" || col.name == "msg" {
        Length::Fill
    } else {
        Length::Fixed(160.0)
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max).collect();
        out.push('…');
        out
    }
}

fn container_selected(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(palette.primary.weak.color.into()),
        text_color: Some(palette.primary.weak.text),
        ..Default::default()
    }
}

fn container_normal(_theme: &Theme) -> container::Style {
    container::Style::default()
}
