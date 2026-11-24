use clap::ValueEnum;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table, TableState, Wrap,
};

use super::app::{AgentChoice, App, ChatPurpose, Input, Mode};
use super::explorer::{Arrow, Explorer, Mark};
use super::menu::{General, ProjectItem};
use crate::agent::Agent;
use crate::chat::chats::Chats;
use crate::output::{printable, printable_line};
use crate::project::size::human;
use crate::snapshot::Snapshot;
use crate::upgrade;

const COMMAND_WIDTH: usize = 12;
const DESCRIPTION_WIDTH: usize = 36;
const HIGHLIGHT: &str = "› ";
const MAX_COMPLETIONS: usize = 30;
const INPUT_WIDTH: u16 = 90;
const LARGE_ITEM: u64 = 100 * 1024 * 1024;
const CLOSED: &str = if cfg!(windows) { "►" } else { "▸" };
const OPEN: &str = if cfg!(windows) { "▼" } else { "▾" };

pub fn render(frame: &mut Frame, app: &App) {
    let [header, projects, general, footer] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Fill(1),
        Constraint::Length(General::ALL.len() as u16 + 2),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    render_header(frame, app, header);
    render_projects(frame, app, projects);
    render_general(frame, app, general);
    frame.render_widget(Line::from(footer_keys(&app.mode)).dark_gray(), footer);

    match &app.mode {
        Mode::Browse => {}
        Mode::ProjectMenu { selected } => render_project_menu(frame, app, *selected),
        Mode::Input { purpose, value } => render_input(frame, app, purpose, value),
        Mode::ChooseAgent { selected, purpose } => render_agents(frame, *selected, purpose),
        Mode::LoadingChats { project, .. } => {
            let area = popup(frame.area(), 60, 3);
            frame.render_widget(Clear, area);
            frame.render_widget(
                Paragraph::new(format!("Loading the chats of {project}…"))
                    .block(Block::bordered().title(" chats ")),
                area,
            );
        }
        Mode::Chats {
            project,
            purpose,
            chats,
            selected,
            ..
        } => render_chats(frame, project, purpose, chats, *selected),
        Mode::LoadingChatView { title, .. } => {
            let area = popup(frame.area(), 70, 3);
            frame.render_widget(Clear, area);
            frame.render_widget(
                Paragraph::new(format!("Loading \"{}\"…", printable_line(title)))
                    .block(Block::bordered().title(" chat ")),
                area,
            );
        }
        Mode::ChatView {
            title,
            lines,
            scroll,
            ..
        } => render_chat_view(frame, title, lines, *scroll),
        Mode::LoadingReview { project, .. } => {
            let area = popup(frame.area(), 60, 3);
            frame.render_widget(Clear, area);
            frame.render_widget(
                Paragraph::new(format!("Checking what changed in {project}…"))
                    .block(Block::bordered().title(" review ")),
                area,
            );
        }
        Mode::Review { project, lines } => render_review(frame, project, lines),
        Mode::LoadingSnapshots { project, .. } => {
            let area = popup(frame.area(), 60, 3);
            frame.render_widget(Clear, area);
            frame.render_widget(
                Paragraph::new(format!("Loading the snapshots of {project}…"))
                    .block(Block::bordered().title(" snapshots ")),
                area,
            );
        }
        Mode::Snapshots {
            snapshots,
            selected,
            ..
        } => render_snapshots(frame, snapshots, *selected),
        Mode::Explorer { explorer, .. } => render_explorer(frame, explorer),
        Mode::Apply {
            explorer,
            question,
            asks_git,
            ..
        } => {
            render_explorer(frame, explorer);
            let title = if *asks_git {
                " [y] yes, with history   [n] without history   [Esc] back "
            } else {
                " [y] apply   [n] back "
            };
            render_question(frame, question, title);
        }
        Mode::Confirm { question, .. } => render_confirm(frame, question),
        Mode::Help => render_help(frame),
        Mode::Running { title, log, result } => render_running(frame, title, log, result.as_ref()),
    }
}

fn render_header(frame: &mut Frame, app: &App, area: Rect) {
    let latest = app
        .loaded
        .as_ref()
        .and_then(|loaded| loaded.latest.as_deref());
    let title = match (app.loading, latest, upgrade::installed()) {
        // Until sai restarts into the version it installed
        (_, _, Some(version)) => Line::from(format!(
            " sai · {version} is installed but not running yet, press U to restart "
        ))
        .fg(Color::Yellow),
        (true, _, None) => Line::from(" sai · loading… "),
        (false, Some(version), None) => {
            Line::from(format!(" sai · {version} is available, press U to update "))
                .fg(Color::Yellow)
        }
        (false, None, None) => Line::from(" sai "),
    };
    let line = match (&app.error, &app.loaded) {
        (Some(_), None) => Line::from("Cannot load the projects, see below").fg(Color::Red),
        (Some(error), _) => Line::from(printable_line(error)).fg(Color::Red),
        (None, Some(loaded)) => Line::from(format!(
            "{}   cpu {} core(s), load {}   memory {}   disk {}",
            loaded.ip, loaded.vps.cores, loaded.vps.load, loaded.vps.memory, loaded.vps.disk
        )),
        (None, None) => Line::from("Connecting to the VPS…"),
    };
    frame.render_widget(
        Paragraph::new(line).block(Block::bordered().title(title)),
        area,
    );
}

fn render_projects(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::bordered().title(" projects ");
    if let (Some(error), None) = (&app.error, &app.loaded) {
        let mut lines: Vec<Line> = error
            .lines()
            .map(|line| Line::from(printable_line(line)).fg(Color::Red))
            .collect();
        lines.push(Line::default());
        lines.push(Line::from(
            "Is the VPS on and set up? Set it up once with `sai setup` in a terminal.",
        ));
        lines.push(Line::from(
            "Choose doctor below to see what exactly is wrong. sai tries again every 15 seconds.",
        ));
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .block(block),
            area,
        );
        return;
    }
    if app.projects().is_empty() {
        let hint = if app.loaded.is_some() {
            "No projects yet. Choose attach below and pick a folder with your code, or press ? for help"
        } else {
            ""
        };
        frame.render_widget(Paragraph::new(hint).dark_gray().block(block), area);
        return;
    }

    let usage = app.loaded.as_ref().map(|loaded| &loaded.vps.agents);
    let rows = app.projects().iter().map(|project| {
        let running =
            usage.and_then(|agents| agents.iter().find(|usage| usage.name == project.name));
        let style = if project.only_on_vps {
            Style::new().fg(Color::Yellow)
        } else {
            Style::new()
        };
        Row::new([
            if project.review > 0 {
                Cell::from(format!("{} ! {}", project.name, project.review)).fg(Color::Yellow)
            } else {
                Cell::from(project.name.as_str())
            },
            Cell::from(project.agent.as_str()),
            Cell::from(printable_line(&project.state)),
            Cell::from(printable_line(&project.sync)),
            Cell::from(running.map_or("", |usage| usage.cpu.as_str())),
            Cell::from(running.map_or("", |usage| usage.memory.as_str())),
            Cell::from(folders(project)),
        ])
        .style(style)
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(16),
            Constraint::Length(7),
            Constraint::Length(11),
            Constraint::Length(24),
            Constraint::Length(7),
            Constraint::Length(20),
            Constraint::Fill(1),
        ],
    )
    .header(
        Row::new(["NAME", "AGENT", "STATE", "SYNC", "CPU", "MEMORY", "FOLDERS"])
            .style(Style::new().bold()),
    )
    .row_highlight_style(Style::new().reversed())
    .highlight_symbol(HIGHLIGHT)
    .block(block);
    let selected = app.selected_project().map(|_| app.selected);
    let mut state = TableState::default().with_selected(selected);
    frame.render_stateful_widget(table, area, &mut state);
}

fn render_general(frame: &mut Frame, app: &App, area: Rect) {
    let items = General::ALL
        .iter()
        .map(|item| menu_item(item.command(), item.description(), item.key()));
    render_list(frame, area, " menu ", items, app.selected_general());
}

fn render_project_menu(frame: &mut Frame, app: &App, selected: usize) {
    let name = app
        .selected_project()
        .map_or("", |project| project.name.as_str());
    let items = ProjectItem::ALL
        .iter()
        .map(|item| menu_item(item.command(), item.description(), item.key()));
    let area = popup(frame.area(), 58, ProjectItem::ALL.len() as u16 + 2);
    frame.render_widget(Clear, area);
    render_list(frame, area, &format!(" {name} "), items, Some(selected));
}

fn render_agents(frame: &mut Frame, selected: usize, purpose: &AgentChoice) {
    let title = match purpose {
        AgentChoice::Attach { .. } => " agent for the new project ",
        AgentChoice::Change { .. } => " switch agent ",
    };
    let agents = Agent::value_variants();
    let items = agents.iter().map(|agent| ListItem::new(agent.name()));
    let area = popup(frame.area(), 30, agents.len() as u16 + 2);
    frame.render_widget(Clear, area);
    render_list(frame, area, title, items, Some(selected));
}

fn render_chat_view(frame: &mut Frame, title: &str, lines: &[String], scroll: usize) {
    let full = frame.area();
    let area = popup(
        full,
        full.width.saturating_sub(4),
        full.height.saturating_sub(2),
    );
    frame.render_widget(Clear, area);
    let text: Vec<Line> = lines
        .iter()
        .skip(scroll)
        .map(|line| {
            let line = printable_line(line);
            if line.ends_with(':') && !line.starts_with(' ') {
                Line::from(line).bold()
            } else if line.trim_start().starts_with('[') {
                Line::from(line).dark_gray()
            } else {
                Line::from(line)
            }
        })
        .collect();
    frame.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .block(Block::bordered().title(format!(" {} (read only) ", printable_line(title)))),
        area,
    );
}

fn render_chats(
    frame: &mut Frame,
    project: &str,
    purpose: &ChatPurpose,
    chats: &Chats,
    selected: usize,
) {
    let title = match purpose {
        ChatPurpose::Open => format!(" chats in {project} "),
        ChatPurpose::Remove => format!(" delete a chat in {project} "),
    };
    let running = chats.running().map(|chat| chat.id.as_str());
    let new = purpose.offers_new().then(|| {
        let line = if chats.busy() {
            Line::from("+ new chat  ·  available when the agent finishes").dark_gray()
        } else {
            Line::from("+ new chat")
        };
        ListItem::new(line)
    });
    let items = new.into_iter().chain(chats.list.iter().map(|chat| {
        let mut spans = vec![Span::from(printable_line(&chat.label()))];
        if running == Some(chat.id.as_str()) {
            spans.push(Span::from(format!("  {}", chats.state)).fg(Color::Yellow));
        }
        ListItem::new(Line::from(spans))
    }));
    let rows = chats.list.len() + usize::from(purpose.offers_new());
    let height = (rows as u16 + 2).min(frame.area().height.saturating_sub(4));
    let area = popup(frame.area(), 90, height);
    frame.render_widget(Clear, area);
    render_list(frame, area, &title, items, Some(selected));
}

fn render_review(frame: &mut Frame, project: &str, lines: &[String]) {
    let mut text = vec![
        Line::from(format!(
            "Files in {project} that can run code on your computer changed since your last review."
        )),
        Line::from("The agent or you changed them. Look at them before you run, build or commit."),
        Line::default(),
    ];
    text.extend(
        lines
            .iter()
            .map(|line| Line::from(printable_line(line)).fg(Color::Yellow)),
    );
    let height = (text.len() as u16 + 2).min(frame.area().height);
    let area = popup(frame.area(), 90, height);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .block(Block::bordered().title(" review ")),
        area,
    );
}

fn render_snapshots(frame: &mut Frame, snapshots: &[Snapshot], selected: usize) {
    let height = (snapshots.len().max(1) as u16 + 2).min(24);
    let area = popup(frame.area(), 80, height);
    frame.render_widget(Clear, area);
    if snapshots.is_empty() {
        frame.render_widget(
            Paragraph::new("No snapshots yet").block(Block::bordered().title(" snapshots ")),
            area,
        );
        return;
    }
    let items = snapshots.iter().map(|snapshot| {
        let (ago, date) = snapshot.when();
        let summary = Span::from(printable_line(&snapshot.summary));
        let summary = if snapshot.summary.contains("deleted") {
            summary.fg(Color::Yellow)
        } else {
            summary.dark_gray()
        };
        ListItem::new(Line::from(vec![
            Span::from(format!("{ago:<12} ")).bold(),
            Span::from(format!("{date:<17} ")).dark_gray(),
            summary,
        ]))
    });
    render_list(
        frame,
        area,
        " snapshots, newest first: what changed since the one below ",
        items,
        Some(selected),
    );
}

fn render_explorer(frame: &mut Frame, explorer: &Explorer) {
    let full = frame.area();
    let area = popup(
        full,
        full.width.saturating_sub(4),
        full.height.saturating_sub(2),
    );
    frame.render_widget(Clear, area);
    let [list, bottom] = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(area);
    let title = format!(" {}: folders on this computer ", explorer.project);
    let uploads = format!(" uploads {} ", human(explorer.uploaded()));
    let items = explorer.rows().into_iter().map(|row| {
        let mark = match row.mark {
            Mark::On => "[x]",
            Mark::Partial => "[~]",
            Mark::Off => "[ ]",
            Mark::Ignored => "[-]",
        };
        let arrow = match row.arrow {
            Arrow::None => " ",
            Arrow::Closed => CLOSED,
            Arrow::Open => OPEN,
        };
        let mut spans = vec![
            Span::from(format!("{}{arrow} {mark} ", "  ".repeat(row.depth))),
            Span::from(printable_line(&row.name)),
        ];
        if !row.note.is_empty() {
            spans.push(Span::from(format!("  {}", printable_line(&row.note))).dark_gray());
        }
        if row.measuring {
            spans.push(Span::from("  measuring…").dark_gray());
        } else if let Some(size) = row.size {
            let text = match row.total {
                Some(total) if total != size => format!("  {} of {}", human(size), human(total)),
                _ => format!("  {}", human(size)),
            };
            let span = Span::from(text);
            spans.push(if size >= LARGE_ITEM {
                span.fg(Color::Yellow)
            } else {
                span.dark_gray()
            });
        }
        let line = Line::from(spans);
        if matches!(row.mark, Mark::Off | Mark::Ignored) {
            ListItem::new(line.dark_gray())
        } else {
            ListItem::new(line)
        }
    });
    let block = Block::bordered()
        .title(title)
        .title(Line::from(uploads).right_aligned());
    let list_widget = List::new(items)
        .block(block)
        .highlight_style(Style::new().reversed())
        .highlight_symbol(HIGHLIGHT);
    let mut state = ListState::default()
        .with_offset(explorer.offset.get())
        .with_selected(Some(explorer.cursor));
    frame.render_stateful_widget(list_widget, list, &mut state);
    explorer.offset.set(state.offset());
    explorer.area.set(list);
    let line = match (&explorer.typing, &explorer.message) {
        (Some(value), _) => {
            let mut text = format!(" path: {value}_");
            if !explorer.completions.is_empty() {
                text.push_str(&format!("   {}", explorer.completions.join("   ")));
            }
            Line::from(text)
        }
        (None, Some(message)) => Line::from(format!(" {}", printable_line(message))).fg(Color::Red),
        (None, None) => Line::default(),
    };
    frame.render_widget(Clear, bottom);
    frame.render_widget(line, bottom);
}

fn folders(project: &crate::project::list::Row) -> String {
    project
        .folders
        .iter()
        .map(|folder| printable_line(&folder.label()))
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_input(frame: &mut Frame, app: &App, purpose: &Input, value: &str) {
    let title = match purpose {
        Input::Project => " project name, new or existing ".to_owned(),
        Input::Rename { project } => format!(" new name for {project} "),
        Input::RestorePath { .. } => {
            " path like front/src/app.ts, empty for the whole project ".to_owned()
        }
        Input::PurgeConfirmation { project } => {
            format!(" type {project} to delete it with its files and snapshots on the VPS ")
        }
        Input::ResetConfirmation => {
            let ip = app.loaded.as_ref().map_or("", |loaded| loaded.ip.as_str());
            format!(" type {ip} to delete everything on the VPS ")
        }
    };
    let mut lines = vec![Line::from(format!("{value}_"))];
    if !app.completions.is_empty() {
        let shown: Vec<&str> = app
            .completions
            .iter()
            .take(MAX_COMPLETIONS)
            .map(String::as_str)
            .collect();
        let more = app.completions.len().saturating_sub(MAX_COMPLETIONS);
        let mut text = shown.join("   ");
        if more > 0 {
            text.push_str(&format!("   … and {more} more"));
        }
        lines.push(Line::from(text).dark_gray());
    }
    if let Some(error) = &app.input_error {
        lines.push(Line::from(printable_line(error)).fg(Color::Red));
    }
    let width = INPUT_WIDTH.min(frame.area().width);
    let inner = usize::from(width.saturating_sub(2)).max(1);
    let rows: usize = lines
        .iter()
        .map(|line| {
            let text: String = line
                .spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect();
            wrapped_lines(&text, inner)
        })
        .sum();
    let area = popup(frame.area(), width, rows as u16 + 2);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(Block::bordered().title(title)),
        area,
    );
}

fn render_help(frame: &mut Frame) {
    let key = |key: &str, text: &str| {
        Line::from(vec![
            Span::from(format!("  {key:<14}")).fg(Color::Cyan),
            Span::from(text.to_owned()),
        ])
    };
    let lines = vec![
        Line::from("In sai").bold(),
        key("↑ ↓", "choose a project or a menu item"),
        key("Enter", "open the project menu or run the menu item"),
        key("letters", "shortcuts shown in [brackets]"),
        key("Esc", "go back, or quit on the main screen"),
        Line::default(),
        Line::from("In the agent").bold(),
        key("mouse wheel", "scroll back through the conversation"),
        key("drag", "select text, it is copied when you let go"),
        key("Ctrl+B, D", "back to sai, the agent keeps working"),
        key("/exit", "stop the agent"),
        Line::from("  If copying does not work in your terminal, hold Shift (Windows, Linux)")
            .dark_gray(),
        Line::from("  or Option (Mac) while selecting.").dark_gray(),
        Line::default(),
        Line::from("Your files").bold(),
        Line::from("  sai never deletes your local folders. The VPS copy of every project is"),
        Line::from("  saved every 3 minutes, restore it with snapshots in the project menu."),
    ];
    let area = popup(frame.area(), 80, lines.len() as u16 + 2);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(" help ")),
        area,
    );
}

fn render_confirm(frame: &mut Frame, question: &str) {
    render_question(frame, question, " [y] yes   [n] no ");
}

fn render_question(frame: &mut Frame, question: &str, title: &str) {
    let width = INPUT_WIDTH.min(frame.area().width);
    let inner = usize::from(width.saturating_sub(2)).max(1);
    let height = u16::try_from(wrapped_lines(question, inner)).unwrap_or(u16::MAX);
    let area = popup(frame.area(), width, height.saturating_add(2));
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(printable(question))
            .wrap(Wrap { trim: true })
            .block(Block::bordered().title(title.to_owned())),
        area,
    );
}

fn wrapped_lines(text: &str, width: usize) -> usize {
    text.lines()
        .map(|line| {
            let mut lines = 1;
            let mut used = 0;
            for word in line.split_whitespace() {
                let length = word.chars().count();
                let needed = if used == 0 { length } else { used + 1 + length };
                if needed <= width {
                    used = needed;
                } else {
                    lines += length.saturating_sub(1) / width + usize::from(used > 0);
                    used = (length - 1) % width + 1;
                }
            }
            lines
        })
        .sum::<usize>()
        .max(1)
}

fn render_running(
    frame: &mut Frame,
    title: &str,
    log: &[String],
    result: Option<&Result<(), String>>,
) {
    let full = frame.area();
    let area = popup(
        full,
        full.width.saturating_sub(8),
        full.height.saturating_sub(4),
    );
    let (status, color) = match result {
        None => ("working…", Color::Yellow),
        Some(Ok(())) => ("done", Color::Green),
        Some(Err(_)) => ("failed", Color::Red),
    };
    let block = Block::bordered()
        .title(format!(" {title} "))
        .title(Line::from(format!(" {status} ")).fg(color).right_aligned());
    let mut lines: Vec<Line> = log.iter().map(|line| log_line(line)).collect();
    if let Some(Err(error)) = result {
        lines.push(Line::default());
        lines.extend(
            error
                .lines()
                .map(|line| Line::from(printable_line(line)).fg(Color::Red)),
        );
    }
    let visible = area.height.saturating_sub(2) as usize;
    let scroll = lines.len().saturating_sub(visible) as u16;
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        area,
    );
}

fn log_line(line: &str) -> Line<'static> {
    let text = Line::from(line.to_owned());
    let trimmed = line.trim_start();
    if line.starts_with("==>") {
        text.bold()
    } else if trimmed.starts_with("FAIL") || line.starts_with("Error") {
        text.fg(Color::Red)
    } else if trimmed.starts_with("warn") {
        text.fg(Color::Yellow)
    } else if trimmed.starts_with("ok ") {
        text.fg(Color::Green)
    } else {
        text
    }
}

fn render_list<'a>(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    items: impl Iterator<Item = ListItem<'a>>,
    selected: Option<usize>,
) {
    let list = List::new(items)
        .block(Block::bordered().title(title.to_owned()))
        .highlight_style(Style::new().reversed())
        .highlight_symbol(HIGHLIGHT);
    let visible = usize::from(area.height.saturating_sub(2)).max(1);
    let offset = selected.map_or(0, |selected| selected / visible * visible);
    let mut state = ListState::default()
        .with_offset(offset)
        .with_selected(selected);
    frame.render_stateful_widget(list, area, &mut state);
}

fn menu_item(command: &str, description: &str, key: char) -> ListItem<'static> {
    ListItem::new(Line::from(vec![
        Span::from(format!("{command:<COMMAND_WIDTH$}")).bold(),
        Span::from(format!("{description:<DESCRIPTION_WIDTH$}")).dark_gray(),
        Span::from(format!("[{key}]")).fg(Color::Cyan),
    ]))
}

fn footer_keys(mode: &Mode) -> &'static str {
    match mode {
        Mode::Browse => "[↑] [↓] move   [Enter] select   [?] help   [Esc] quit",
        Mode::Help => "any key: back",
        Mode::ChatView { .. } => "[↑] [↓] [PgUp] [PgDn] scroll   [Esc] back to the list",
        Mode::LoadingSnapshots { .. }
        | Mode::LoadingReview { .. }
        | Mode::LoadingChats { .. }
        | Mode::LoadingChatView { .. } => "[Esc] cancel",
        Mode::Chats {
            purpose: ChatPurpose::Open,
            ..
        } => "[↑] [↓] move   [Enter] open   [n] new chat   [v] read   [Esc] back",
        Mode::Chats { .. } => "[↑] [↓] move   [Enter] delete   [v] read   [Esc] back",
        Mode::Review { .. } => "[Enter] I checked them   [Esc] later",
        Mode::ProjectMenu { .. } | Mode::Snapshots { .. } => {
            "[↑] [↓] move   [Enter] select   [Esc] back"
        }
        Mode::Input { .. } => "[Tab] complete   [Enter] next   [Esc] cancel",
        Mode::Explorer { explorer, .. } if explorer.typing.is_some() => {
            "[Tab] complete   [Enter] go there   [Esc] back to the list"
        }
        Mode::Explorer { .. } => {
            "[Space] check   [→] [←] open, close   [/] type a path   [Enter] apply   [Esc] cancel"
        }
        Mode::Apply { .. } => "",
        Mode::ChooseAgent { .. } => "[↑] [↓] move   [Enter] choose   [Esc] cancel",
        Mode::Confirm { .. } => "[y] confirm   [n] cancel",
        Mode::Running { result: None, .. } => "working, please wait…",
        Mode::Running { .. } => "[Enter] back",
    }
}

fn popup(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::{render, wrapped_lines};
    use crate::agent::Agent;
    use crate::chat::chats::{Chat, Chats};
    use crate::project::list::Row;
    use crate::snapshot::Snapshot;
    use crate::status::Vps;
    use crate::tui::app::{AgentChoice, App, ChatPurpose, Input, Mode};
    use crate::tui::loader::Loaded;

    fn row(name: &str, state: &str, only_on_vps: bool) -> Row {
        Row {
            only_on_vps,
            name: name.to_owned(),
            agent: "claude".to_owned(),
            state: state.to_owned(),
            sync: "synced".to_owned(),
            review: 0,
            folders: Vec::new(),
        }
    }

    fn app(latest: Option<&str>) -> App {
        let mut app = App::default();
        app.update(Ok(Loaded {
            ip: "203.0.113.7".to_owned(),
            projects: vec![
                row("shop", "your turn", false),
                row("remote", "only on VPS", true),
            ],
            vps: Vps::default(),
            latest: latest.map(str::to_owned),
        }));
        app
    }

    /// What the terminal shows, one line per row
    fn screen(app: &App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| render(frame, app)).unwrap();
        let buffer = terminal.backend().buffer();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn chats(state: &str) -> Chats {
        Chats {
            state: state.to_owned(),
            clients: 0,
            list: vec![Chat {
                id: "c0".to_owned(),
                modified: 0,
                title: "Refactor the scheduler".to_owned(),
            }],
        }
    }

    /// Every screen of the menu, as the user can reach it
    fn modes() -> Vec<Mode> {
        let (_, chats_loading) = mpsc::channel();
        let (_, view_loading) = mpsc::channel();
        let (_, review_loading) = mpsc::channel();
        let (_, snapshots_loading) = mpsc::channel();
        vec![
            Mode::Browse,
            Mode::ProjectMenu { selected: 3 },
            Mode::Input {
                purpose: Input::Project,
                value: "new shop".to_owned(),
            },
            Mode::Input {
                purpose: Input::Rename {
                    project: "shop".to_owned(),
                },
                value: "store".to_owned(),
            },
            Mode::Input {
                purpose: Input::RestorePath {
                    project: "shop".to_owned(),
                    snapshot: "20261120-100000".to_owned(),
                },
                value: String::new(),
            },
            Mode::Input {
                purpose: Input::ResetConfirmation,
                value: String::new(),
            },
            Mode::Input {
                purpose: Input::PurgeConfirmation {
                    project: "shop".to_owned(),
                },
                value: "sh".to_owned(),
            },
            Mode::ChooseAgent {
                selected: 1,
                purpose: AgentChoice::Attach {
                    project: "shop".to_owned(),
                },
            },
            Mode::ChooseAgent {
                selected: 0,
                purpose: AgentChoice::Change {
                    project: "shop".to_owned(),
                },
            },
            Mode::LoadingChats {
                project: "shop".to_owned(),
                agent: Agent::Claude,
                purpose: ChatPurpose::Open,
                receiver: chats_loading,
            },
            Mode::Chats {
                project: "shop".to_owned(),
                agent: Agent::Claude,
                purpose: ChatPurpose::Open,
                chats: chats("working"),
                selected: 1,
            },
            Mode::Chats {
                project: "shop".to_owned(),
                agent: Agent::Claude,
                purpose: ChatPurpose::Remove,
                chats: chats("idle"),
                selected: 0,
            },
            Mode::LoadingChatView {
                title: "Refactor".to_owned(),
                receiver: view_loading,
                back: Box::new(Mode::Browse),
            },
            Mode::ChatView {
                title: "Refactor".to_owned(),
                lines: (0..200)
                    .map(|line| format!("line {line} ").repeat(30))
                    .collect(),
                scroll: 150,
                back: Box::new(Mode::Browse),
            },
            Mode::LoadingReview {
                project: "shop".to_owned(),
                receiver: review_loading,
            },
            Mode::Review {
                project: "shop".to_owned(),
                lines: vec!["front/build.rs".to_owned(); 60],
            },
            Mode::LoadingSnapshots {
                project: "shop".to_owned(),
                receiver: snapshots_loading,
            },
            Mode::Snapshots {
                project: "shop".to_owned(),
                snapshots: (0..40)
                    .map(|day| Snapshot {
                        stamp: format!("202611{:02}-100000", day % 28 + 1),
                        summary: "changed 3, added 1".to_owned(),
                    })
                    .collect(),
                selected: 39,
            },
            Mode::Confirm {
                question: "Delete the chat? ".repeat(20),
                action: crate::tui::actions::Action::Restart,
            },
            Mode::Help,
            Mode::Running {
                title: "doctor".to_owned(),
                log: (0..100).map(|line| format!("==> step {line}")).collect(),
                result: Some(Err("docker is missing ".repeat(10))),
            },
        ]
    }

    #[test]
    fn every_screen_draws_in_any_window() {
        for size in [(120, 40), (80, 24), (40, 12), (20, 6), (1, 1)] {
            for mode in modes() {
                let mut app = app(Some("9.9.9"));
                app.error = Some("cannot reach the VPS ".repeat(10));
                app.mode = mode;
                screen(&app, size.0, size.1);
            }
        }
    }

    #[test]
    fn the_list_shows_projects_their_state_and_the_keys() {
        let shown = screen(&app(None), 120, 40);
        for text in [
            "shop",
            "your turn",
            "remote",
            "203.0.113.7",
            "[U]",
            "[q]",
            "quit",
        ] {
            assert!(shown.contains(text), "{text} in\n{shown}");
        }
        assert!(!shown.contains("is available"));
        // Another test may have pretended an upgrade, then the header asks to restart instead
        if crate::upgrade::installed().is_none() {
            let newer = screen(&app(Some("9.9.9")), 120, 40);
            assert!(
                newer.contains("9.9.9 is available, press U to update"),
                "{newer}"
            );
        }
        assert!(screen(&App::default(), 120, 40).contains("Connecting to the VPS"));
    }

    #[test]
    fn the_chat_list_offers_a_new_chat_unless_the_agent_works() {
        let mut app = app(None);
        app.mode = Mode::Chats {
            project: "shop".to_owned(),
            agent: Agent::Claude,
            purpose: ChatPurpose::Open,
            chats: chats("idle"),
            selected: 1,
        };
        let idle = screen(&app, 120, 40);
        assert!(idle.contains("+ new chat") && idle.contains("Refactor the scheduler"));
        assert!(idle.contains("[n] new chat"), "the footer tells the key");
        app.mode = Mode::Chats {
            project: "shop".to_owned(),
            agent: Agent::Claude,
            purpose: ChatPurpose::Open,
            chats: chats("working"),
            selected: 1,
        };
        assert!(screen(&app, 120, 40).contains("available when the agent finishes"));
        app.mode = Mode::Chats {
            project: "shop".to_owned(),
            agent: Agent::Claude,
            purpose: ChatPurpose::Remove,
            chats: chats("idle"),
            selected: 0,
        };
        assert!(!screen(&app, 120, 40).contains("+ new chat"));
    }

    #[test]
    fn counts_wrapped_lines() {
        assert_eq!(wrapped_lines("", 10), 1);
        assert_eq!(wrapped_lines("short", 10), 1);
        assert_eq!(wrapped_lines("one two three", 9), 2);
        assert_eq!(wrapped_lines("aaaa bbbb cccc", 9), 2);
        assert_eq!(wrapped_lines("abcdefghijklmnopqrstuvwxy", 10), 3);
        assert_eq!(wrapped_lines("first\nsecond line here", 10), 3);
    }
}
