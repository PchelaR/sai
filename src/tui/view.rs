use clap::ValueEnum;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table, TableState, Wrap,
};

use super::app::{AgentChoice, App, Input, Mode};
use super::menu::{General, ProjectItem};
use crate::agent::Agent;
use crate::project::list::FolderRow;
use crate::snapshot::Snapshot;

const COMMAND_WIDTH: usize = 12;
const DESCRIPTION_WIDTH: usize = 36;
const HIGHLIGHT: &str = "› ";
const MAX_COMPLETIONS: usize = 30;
const INPUT_WIDTH: u16 = 90;

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
        Mode::Snapshots {
            snapshots,
            selected,
            ..
        } => render_snapshots(frame, snapshots, *selected),
        Mode::Folders {
            project,
            folders,
            selected,
        } => render_folders(frame, project, folders, *selected),
        Mode::Confirm { question, .. } => render_confirm(frame, question),
        Mode::Help => render_help(frame),
        Mode::AskGit { path, .. } => render_ask_git(frame, path),
        Mode::Running { title, log, result } => render_running(frame, title, log, result.as_ref()),
    }
}

fn render_header(frame: &mut Frame, app: &App, area: Rect) {
    let title = if app.loading {
        " sai · loading… "
    } else {
        " sai "
    };
    let line = match (&app.error, &app.loaded) {
        (Some(_), None) => Line::from("Cannot load the projects, see below").fg(Color::Red),
        (Some(error), _) => Line::from(error.as_str()).fg(Color::Red),
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
            .map(|line| Line::from(line.to_owned()).fg(Color::Red))
            .collect();
        lines.push(Line::default());
        lines.push(Line::from(
            "Is the VPS on and set up? Set it up once with `sai setup` in a terminal.",
        ));
        lines.push(Line::from(
            "Choose doctor below to see what exactly is wrong, then refresh.",
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
            Cell::from(project.name.as_str()),
            Cell::from(project.agent.as_str()),
            Cell::from(project.state.as_str()),
            Cell::from(project.sync.as_str()),
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
        let summary = Span::from(snapshot.summary.clone());
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

fn render_folders(frame: &mut Frame, project: &str, folders: &[FolderRow], selected: usize) {
    let height = (folders.len().max(1) as u16 + 2).min(20);
    let area = popup(frame.area(), 80, height);
    frame.render_widget(Clear, area);
    let title = format!(" detach a folder from {project} ");
    if folders.is_empty() {
        frame.render_widget(
            Paragraph::new("No folders attached").block(Block::bordered().title(title)),
            area,
        );
        return;
    }
    let items = folders.iter().map(|folder| {
        ListItem::new(Line::from(vec![
            Span::from(format!("{:<16}", folder.name)).bold(),
            Span::from(folder.path.clone()).dark_gray(),
        ]))
    });
    render_list(frame, area, &title, items, Some(selected));
}

fn folders(project: &crate::project::list::Row) -> String {
    project
        .folders
        .iter()
        .map(|folder| folder.label())
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_input(frame: &mut Frame, app: &App, purpose: &Input, value: &str) {
    let title = match purpose {
        Input::Project => " project name, new or existing ".to_owned(),
        Input::Folder { project } => format!(" folder to attach to {project} "),
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
        lines.push(Line::from(error.as_str()).fg(Color::Red));
    }
    let width = INPUT_WIDTH.min(frame.area().width);
    let inner = usize::from(width.saturating_sub(2)).max(1);
    let rows: usize = lines
        .iter()
        .map(|line| line.width().max(1).div_ceil(inner))
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

fn render_ask_git(frame: &mut Frame, path: &str) {
    let question = crate::attach::git_question(std::path::Path::new(path));
    let width = INPUT_WIDTH.min(frame.area().width);
    let inner = usize::from(width.saturating_sub(2)).max(1);
    let area = popup(
        frame.area(),
        width,
        question.chars().count().div_ceil(inner) as u16 + 2,
    );
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(question)
            .wrap(Wrap { trim: false })
            .block(Block::bordered().title(" [y] yes   [n] no   [Esc] cancel ")),
        area,
    );
}

fn render_confirm(frame: &mut Frame, question: &str) {
    let area = popup(frame.area(), 70, 3);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(question).block(Block::bordered().title(" [y] yes   [n] no ")),
        area,
    );
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
                .map(|line| Line::from(line.to_owned()).fg(Color::Red)),
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
    let mut state = ListState::default().with_selected(selected);
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
        Mode::AskGit { .. } => "[y] give the history   [n] attach without it   [Esc] cancel",
        Mode::ProjectMenu { .. } | Mode::Snapshots { .. } | Mode::Folders { .. } => {
            "[↑] [↓] move   [Enter] select   [Esc] back"
        }
        Mode::Input { .. } => "[Tab] complete   [Enter] next   [Esc] cancel",
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
