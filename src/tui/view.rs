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

const COMMAND_WIDTH: usize = 12;
const DESCRIPTION_WIDTH: usize = 30;
const HIGHLIGHT: &str = "› ";

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
        Mode::ChooseAgent {
            selected, purpose, ..
        } => render_agents(frame, *selected, *purpose),
        Mode::Snapshots {
            stamps, selected, ..
        } => render_snapshots(frame, stamps, *selected),
        Mode::Confirm { question, .. } => render_confirm(frame, question),
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
    if app.projects().is_empty() {
        let hint = if app.loaded.is_some() {
            "No projects yet. Choose attach below to connect a folder"
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
        Row::new([
            Cell::from(project.name.as_str()),
            Cell::from(project.agent.as_str()),
            Cell::from(project.state.as_str()),
            Cell::from(project.sync.as_str()),
            Cell::from(running.map_or("", |usage| usage.cpu.as_str())),
            Cell::from(running.map_or("", |usage| usage.memory.as_str())),
            Cell::from(project.path.as_str()),
        ])
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(16),
            Constraint::Length(7),
            Constraint::Length(9),
            Constraint::Length(24),
            Constraint::Length(7),
            Constraint::Length(20),
            Constraint::Fill(1),
        ],
    )
    .header(
        Row::new(["NAME", "AGENT", "STATE", "SYNC", "CPU", "MEMORY", "PATH"])
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
    let area = popup(frame.area(), 52, ProjectItem::ALL.len() as u16 + 2);
    frame.render_widget(Clear, area);
    render_list(frame, area, &format!(" {name} "), items, Some(selected));
}

fn render_agents(frame: &mut Frame, selected: usize, purpose: AgentChoice) {
    let title = match purpose {
        AgentChoice::Attach => " agent for the folder ",
        AgentChoice::Change => " switch agent ",
    };
    let agents = Agent::value_variants();
    let items = agents.iter().map(|agent| ListItem::new(agent.name()));
    let area = popup(frame.area(), 30, agents.len() as u16 + 2);
    frame.render_widget(Clear, area);
    render_list(frame, area, title, items, Some(selected));
}

fn render_snapshots(frame: &mut Frame, stamps: &[String], selected: usize) {
    let height = (stamps.len().max(1) as u16 + 2).min(20);
    let area = popup(frame.area(), 40, height);
    frame.render_widget(Clear, area);
    if stamps.is_empty() {
        frame.render_widget(
            Paragraph::new("No snapshots yet").block(Block::bordered().title(" snapshots ")),
            area,
        );
        return;
    }
    let items = stamps.iter().map(|stamp| ListItem::new(stamp.as_str()));
    render_list(frame, area, " snapshots ", items, Some(selected));
}

fn render_input(frame: &mut Frame, app: &App, purpose: &Input, value: &str) {
    let title = match purpose {
        Input::Attach => " folder to attach ".to_owned(),
        Input::RestorePath { .. } => " path to restore, empty for the whole project ".to_owned(),
        Input::ResetConfirmation => {
            let ip = app.loaded.as_ref().map_or("", |loaded| loaded.ip.as_str());
            format!(" type {ip} to delete everything on the VPS ")
        }
    };
    let area = popup(frame.area(), 70, 3);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(format!("{value}_")).block(Block::bordered().title(title)),
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
        Mode::Browse => "[↑] [↓] move   [Enter] select   [Esc] quit",
        Mode::ProjectMenu { .. } | Mode::Snapshots { .. } => {
            "[↑] [↓] move   [Enter] select   [Esc] back"
        }
        Mode::Input { .. } => "[Enter] next   [Esc] cancel",
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
