//! Полноэкранный мастер настройки сервера.

use crate::cli::{self, Diagnostic};
use crate::config::{self, Properties};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use std::io::{self, IsTerminal};
use std::net::IpAddr;
use std::path::{Path, PathBuf};

const BG: Color = Color::Rgb(23, 26, 32);
const PANEL: Color = Color::Rgb(31, 35, 42);
const SELECTED: Color = Color::Rgb(43, 53, 63);
const ACCENT: Color = Color::Rgb(122, 184, 218);
const MUTED: Color = Color::Rgb(154, 162, 172);
const TEXT: Color = Color::Rgb(235, 238, 242);
const WARNING: Color = Color::Rgb(235, 183, 106);

#[derive(Clone, Copy, PartialEq, Eq)]
enum FieldKind {
    Text,
    Number { min: i64, max: i64 },
    Toggle,
    Choice(&'static [&'static str]),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Section {
    Network,
    Server,
    Runtime,
    World,
    Gameplay,
}

const SECTIONS: [(Section, &str); 5] = [
    (Section::Network, "🌐 Net"),
    (Section::Server, "👥 Server"),
    (Section::Runtime, "⚙ Run"),
    (Section::World, "🌍 World*"),
    (Section::Gameplay, "✦ Play*"),
];

struct Field {
    key: &'static str,
    icon: &'static str,
    label: &'static str,
    help: &'static str,
    kind: FieldKind,
    section: Section,
}

const FIELDS: [Field; 22] = [
    Field {
        key: "server-ip",
        icon: "🌐",
        label: "Listen address",
        help: "Use 127.0.0.1 for this computer only. Use 0.0.0.0 to accept connections from the network.",
        kind: FieldKind::Text,
        section: Section::Network,
    },
    Field {
        key: "server-port",
        icon: "🔌",
        label: "Server port",
        help: "TCP port used by Minecraft clients. Choose a number from 1 to 65535.",
        kind: FieldKind::Number { min: 1, max: 65535 },
        section: Section::Network,
    },
    Field {
        key: "motd",
        icon: "✎",
        label: "Server name",
        help: "Message shown in the Minecraft server list. This is the motd setting.",
        kind: FieldKind::Text,
        section: Section::Server,
    },
    Field {
        key: "max-players",
        icon: "👥",
        label: "Player limit",
        help: "Maximum number of connected players. Choose a positive integer.",
        kind: FieldKind::Number {
            min: 1,
            max: i64::MAX,
        },
        section: Section::Server,
    },
    Field {
        key: "online-mode",
        icon: "🔒",
        label: "Authentication",
        help: "On verifies Minecraft accounts. Off accepts unverified names. Press Enter or Space to switch.",
        kind: FieldKind::Toggle,
        section: Section::Server,
    },
    Field {
        key: "network-compression-threshold",
        icon: "⇄",
        label: "Compression",
        help: "Packet size threshold in bytes. Use -1 to disable compression.",
        kind: FieldKind::Number {
            min: -1,
            max: i32::MAX as i64,
        },
        section: Section::Network,
    },
    Field {
        key: "vanilla-jar",
        icon: "📦",
        label: "Vanilla server jar",
        help: "Path to the Minecraft 26.3 server.jar. Relative paths start in the server directory.",
        kind: FieldKind::Text,
        section: Section::Runtime,
    },
    Field {
        key: "enable-status",
        icon: "📡",
        label: "Server list",
        help: "Allow Minecraft clients to see this server in their server list.",
        kind: FieldKind::Toggle,
        section: Section::Network,
    },
    Field {
        key: "java-path",
        icon: "☕",
        label: "Java executable",
        help: "Java command or path. Relative paths with a directory start in the server directory.",
        kind: FieldKind::Text,
        section: Section::Runtime,
    },
    Field {
        key: "generated-dir",
        icon: "📁",
        label: "Generated data",
        help: "Where generated vanilla data is stored. Relative paths start in the server directory.",
        kind: FieldKind::Text,
        section: Section::Runtime,
    },
    Field {
        key: "log-level",
        icon: "▤",
        label: "Log detail",
        help: "Press Enter or Space to choose how much the server logs.",
        kind: FieldKind::Choice(&["error", "warn", "info", "debug", "trace"]),
        section: Section::Runtime,
    },
    Field {
        key: "level-name",
        icon: "🌍",
        label: "World name",
        help: "Saved for a future world implementation; currently has no effect.",
        kind: FieldKind::Text,
        section: Section::World,
    },
    Field {
        key: "level-seed",
        icon: "✧",
        label: "World seed",
        help: "Press R for a random seed or Enter to type one. Saved for a future world implementation.",
        kind: FieldKind::Text,
        section: Section::World,
    },
    Field {
        key: "view-distance",
        icon: "◉",
        label: "View distance",
        help: "2–32 chunks. Saved for a future world implementation; currently has no effect.",
        kind: FieldKind::Number { min: 2, max: 32 },
        section: Section::World,
    },
    Field {
        key: "simulation-distance",
        icon: "◌",
        label: "Simulation distance",
        help: "2–32 chunks. Saved for a future world implementation; currently has no effect.",
        kind: FieldKind::Number { min: 2, max: 32 },
        section: Section::World,
    },
    Field {
        key: "gamemode",
        icon: "🎮",
        label: "Game mode",
        help: "Saved for a future gameplay implementation; currently has no effect.",
        kind: FieldKind::Choice(&["survival", "creative", "adventure", "spectator"]),
        section: Section::Gameplay,
    },
    Field {
        key: "difficulty",
        icon: "🔥",
        label: "Difficulty",
        help: "Saved for a future gameplay implementation; currently has no effect.",
        kind: FieldKind::Choice(&["peaceful", "easy", "normal", "hard"]),
        section: Section::Gameplay,
    },
    Field {
        key: "hardcore",
        icon: "💀",
        label: "Hardcore",
        help: "Saved for a future gameplay implementation; currently has no effect.",
        kind: FieldKind::Toggle,
        section: Section::Gameplay,
    },
    Field {
        key: "pvp",
        icon: "⚔",
        label: "PvP",
        help: "Saved for a future gameplay implementation; currently has no effect.",
        kind: FieldKind::Toggle,
        section: Section::Gameplay,
    },
    Field {
        key: "white-list",
        icon: "📋",
        label: "Allowlist",
        help: "Saved for a future gameplay implementation; currently has no effect.",
        kind: FieldKind::Toggle,
        section: Section::Gameplay,
    },
    Field {
        key: "allow-flight",
        icon: "✈",
        label: "Allow flight",
        help: "Saved for a future gameplay implementation; currently has no effect.",
        kind: FieldKind::Toggle,
        section: Section::Gameplay,
    },
    Field {
        key: "spawn-protection",
        icon: "🛡",
        label: "Spawn protection",
        help: "Non-negative radius. Saved for a future gameplay implementation; currently has no effect.",
        kind: FieldKind::Number {
            min: 0,
            max: u32::MAX as i64,
        },
        section: Section::Gameplay,
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Browse,
    Edit,
    Review,
    ConfirmExit,
    ConfirmExposure,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Continue,
    Save,
    Quit,
}

struct App {
    path: PathBuf,
    original: String,
    initial: Vec<String>,
    values: Vec<String>,
    is_new: bool,
    section: usize,
    selected: usize,
    edit_value: String,
    cursor: usize,
    mode: Mode,
    message: Option<String>,
    issues: Vec<Diagnostic>,
    doctor_checked: bool,
    review_offset: usize,
}

impl App {
    fn open(dir: &Path) -> Result<Self, String> {
        if !dir.is_dir() {
            return Err(format!(
                "--dir {} does not exist; create it first",
                dir.display()
            ));
        }
        let path = dir.join("server.properties");
        let is_new = !path.exists();
        let original = if !is_new {
            config::read(&path)?
        } else {
            config::DEFAULT_PROPERTIES.to_owned()
        };
        let mut app = Self::from_text(path, original)?;
        app.is_new = is_new;
        Ok(app)
    }

    fn from_text(path: PathBuf, original: String) -> Result<Self, String> {
        let mut values = FIELDS
            .iter()
            .map(|field| config::effective_value(&original, field.key))
            .collect::<Vec<_>>();
        if values[0].is_empty() {
            values[0] = "0.0.0.0".into();
        }
        let message = Properties::parse(&original)
            .and_then(|props| props.check_supported())
            .err();
        Ok(Self {
            path,
            original,
            initial: values.clone(),
            values,
            is_new: false,
            section: 0,
            selected: 0,
            edit_value: String::new(),
            cursor: 0,
            mode: Mode::Browse,
            message,
            issues: Vec::new(),
            doctor_checked: false,
            review_offset: 0,
        })
    }

    fn dirty(&self) -> bool {
        self.is_new || self.initial != self.values
    }

    fn field_dirty(&self, index: usize) -> bool {
        self.initial[index] != self.values[index]
    }

    fn section_dirty(&self, section: Section) -> bool {
        FIELDS
            .iter()
            .enumerate()
            .any(|(index, field)| field.section == section && self.field_dirty(index))
    }

    fn changed_fields(&self) -> Vec<usize> {
        (0..FIELDS.len())
            .filter(|&index| self.field_dirty(index))
            .collect()
    }

    fn invalidate_doctor(&mut self) {
        self.doctor_checked = false;
        self.issues.clear();
    }

    fn run_doctor(&mut self) {
        self.issues.clear();
        self.doctor_checked = true;
        match self.validate() {
            Ok(props) => {
                let dir = self.path.parent().unwrap_or(Path::new("."));
                self.issues = cli::diagnose(&props, dir);
            }
            Err(error) => {
                let key = FIELDS
                    .iter()
                    .find(|field| error.starts_with(field.key))
                    .map_or("server-ip", |field| field.key);
                self.issues.push(Diagnostic {
                    key,
                    message: error,
                });
            }
        }
        self.message = if self.issues.is_empty() {
            Some("✓ Ready to start with these settings".into())
        } else {
            Some(format!(
                "Doctor found {} issue(s); select a marked field",
                self.issues.len()
            ))
        };
    }

    fn draft(&self) -> Result<String, String> {
        let mut text = self.original.clone();
        for (index, field) in FIELDS.iter().enumerate() {
            if self.initial[index] != self.values[index] {
                text = config::set(&text, field.key, &self.values[index])?;
            }
        }
        Ok(text)
    }

    fn validate(&self) -> Result<Properties, String> {
        let props = Properties::parse(&self.draft()?)?;
        props.check_supported()?;
        Ok(props)
    }

    fn save(&self) -> Result<(), String> {
        let text = self.draft()?;
        self.validate()?;
        config::save(&self.path, &text)
    }

    fn handle_key(&mut self, key: KeyEvent) -> Action {
        if key.kind != KeyEventKind::Press {
            return Action::Continue;
        }
        match self.mode {
            Mode::Browse => match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    self.move_row(-1);
                    self.message = None;
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.move_row(1);
                    self.message = None;
                }
                KeyCode::Left | KeyCode::BackTab => self.switch_section(-1),
                KeyCode::Right | KeyCode::Tab => self.switch_section(1),
                KeyCode::Enter => match FIELDS[self.selected].kind {
                    FieldKind::Toggle => self.toggle(),
                    FieldKind::Choice(options) => self.cycle_choice(options),
                    FieldKind::Text | FieldKind::Number { .. } => {
                        self.edit_value = self.values[self.selected].clone();
                        self.cursor = self.edit_value.chars().count();
                        self.mode = Mode::Edit;
                        self.message = None;
                    }
                },
                KeyCode::Char(' ') => match FIELDS[self.selected].kind {
                    FieldKind::Toggle => self.toggle(),
                    FieldKind::Choice(options) => self.cycle_choice(options),
                    FieldKind::Text | FieldKind::Number { .. } => {}
                },
                KeyCode::Char('r' | 'R') if FIELDS[self.selected].key == "level-seed" => {
                    self.values[self.selected] = rand::random::<i64>().to_string();
                    self.message = None;
                    self.invalidate_doctor();
                }
                KeyCode::Char('d' | 'D') => self.run_doctor(),
                KeyCode::Char('s') | KeyCode::F(2) => {
                    if !self.dirty() {
                        self.message = Some("No changes to save".into());
                    } else if let Err(error) = self.validate() {
                        self.message = Some(error);
                    } else {
                        self.message = None;
                        self.review_offset = 0;
                        self.mode = Mode::Review;
                    }
                }
                KeyCode::Esc | KeyCode::Char('q') => {
                    if self.dirty() {
                        self.mode = Mode::ConfirmExit;
                    } else {
                        return Action::Quit;
                    }
                }
                _ => {}
            },
            Mode::Edit => match key.code {
                KeyCode::Enter => {
                    let field = &FIELDS[self.selected];
                    let candidate =
                        config::set(config::DEFAULT_PROPERTIES, field.key, &self.edit_value)
                            .and_then(|text| Properties::parse(&text));
                    match candidate {
                        Ok(_) => {
                            self.values[self.selected] = self.edit_value.clone();
                            self.mode = Mode::Browse;
                            self.message = None;
                            self.invalidate_doctor();
                        }
                        Err(error) => {
                            self.message = Some(error);
                        }
                    }
                }
                KeyCode::Esc => {
                    self.mode = Mode::Browse;
                    self.message = None;
                }
                KeyCode::Backspace => {
                    if self.cursor > 0 {
                        let start = byte_index(&self.edit_value, self.cursor - 1);
                        let end = byte_index(&self.edit_value, self.cursor);
                        self.edit_value.replace_range(start..end, "");
                        self.cursor -= 1;
                    }
                    self.message = None;
                }
                KeyCode::Delete => {
                    if self.cursor < self.edit_value.chars().count() {
                        let start = byte_index(&self.edit_value, self.cursor);
                        let end = byte_index(&self.edit_value, self.cursor + 1);
                        self.edit_value.replace_range(start..end, "");
                    }
                    self.message = None;
                }
                KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
                KeyCode::Right => {
                    self.cursor = (self.cursor + 1).min(self.edit_value.chars().count());
                }
                KeyCode::Up | KeyCode::Down | KeyCode::PageUp | KeyCode::PageDown
                    if matches!(FIELDS[self.selected].kind, FieldKind::Number { .. }) =>
                {
                    let step = if key.modifiers.contains(KeyModifiers::SHIFT)
                        || matches!(key.code, KeyCode::PageUp | KeyCode::PageDown)
                    {
                        10
                    } else {
                        1
                    };
                    let delta = if matches!(key.code, KeyCode::Up | KeyCode::PageUp) {
                        step
                    } else {
                        -step
                    };
                    self.step_number(delta);
                }
                KeyCode::Home => self.cursor = 0,
                KeyCode::End => self.cursor = self.edit_value.chars().count(),
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    let index = byte_index(&self.edit_value, self.cursor);
                    self.edit_value.insert(index, c);
                    self.cursor += 1;
                    self.message = None;
                }
                _ => {}
            },
            Mode::Review => match key.code {
                KeyCode::Up => self.review_offset = self.review_offset.saturating_sub(1),
                KeyCode::Down => {
                    self.review_offset =
                        (self.review_offset + 1).min(self.changed_fields().len().saturating_sub(1));
                }
                KeyCode::Enter | KeyCode::Char('s') => match self.validate() {
                    Ok(props) if self.needs_exposure_confirmation(&props) => {
                        self.mode = Mode::ConfirmExposure;
                    }
                    Ok(_) => return Action::Save,
                    Err(error) => {
                        self.message = Some(error);
                        self.mode = Mode::Browse;
                    }
                },
                KeyCode::Esc => self.mode = Mode::Browse,
                _ => {}
            },
            Mode::ConfirmExit => match key.code {
                KeyCode::Char('s') => {
                    self.review_offset = 0;
                    self.mode = Mode::Review;
                }
                KeyCode::Char('d') => return Action::Quit,
                KeyCode::Esc => self.mode = Mode::Browse,
                _ => {}
            },
            Mode::ConfirmExposure => match key.code {
                KeyCode::Char('y') | KeyCode::Enter => return Action::Save,
                KeyCode::Char('n') | KeyCode::Esc => self.mode = Mode::Review,
                _ => {}
            },
        }
        Action::Continue
    }

    fn toggle(&mut self) {
        self.values[self.selected] = if self.values[self.selected] == "true" {
            "false".into()
        } else {
            "true".into()
        };
        self.message = None;
        self.invalidate_doctor();
    }

    fn cycle_choice(&mut self, options: &'static [&'static str]) {
        let current = options
            .iter()
            .position(|&option| option == self.values[self.selected]);
        self.values[self.selected] =
            options[current.map_or(0, |index| (index + 1) % options.len())].into();
        self.message = None;
        self.invalidate_doctor();
    }

    fn step_number(&mut self, delta: i64) {
        let FieldKind::Number { min, max } = FIELDS[self.selected].kind else {
            return;
        };
        match self.edit_value.parse::<i64>() {
            Ok(value) => {
                self.edit_value = value.saturating_add(delta).clamp(min, max).to_string();
                self.cursor = self.edit_value.chars().count();
                self.message = None;
            }
            Err(_) => {
                self.message = Some(format!(
                    "{}: enter a valid number before using arrows",
                    FIELDS[self.selected].key
                ));
            }
        }
    }

    fn move_row(&mut self, direction: isize) {
        let indices = self.section_indices();
        let current = indices
            .iter()
            .position(|&index| index == self.selected)
            .unwrap_or(0);
        let next = (current as isize + direction).rem_euclid(indices.len() as isize) as usize;
        self.selected = indices[next];
    }

    fn switch_section(&mut self, direction: isize) {
        self.section =
            (self.section as isize + direction).rem_euclid(SECTIONS.len() as isize) as usize;
        self.selected = self.section_indices()[0];
        self.message = None;
    }

    fn section_indices(&self) -> Vec<usize> {
        let section = SECTIONS[self.section].0;
        FIELDS
            .iter()
            .enumerate()
            .filter_map(|(index, field)| (field.section == section).then_some(index))
            .collect()
    }

    fn needs_exposure_confirmation(&self, props: &Properties) -> bool {
        is_public(&props.server_ip)
            && !props.online_mode
            && (self.initial[0] != self.values[0] || self.initial[4] != self.values[4])
    }
}

fn is_public(ip: &str) -> bool {
    !ip.parse::<IpAddr>()
        .expect("validated server-ip")
        .is_loopback()
}

fn byte_index(text: &str, character: usize) -> usize {
    text.char_indices()
        .nth(character)
        .map_or(text.len(), |(index, _)| index)
}

fn display_value(app: &App, index: usize) -> Line<'static> {
    if app.mode == Mode::Edit && app.selected == index {
        let cursor_byte = byte_index(&app.edit_value, app.cursor);
        let before = app.edit_value[..cursor_byte].to_owned();
        let after = app.edit_value[cursor_byte..].to_owned();
        return Line::from(vec![
            Span::styled(before, Style::default().fg(TEXT)),
            Span::styled(" ", Style::default().bg(ACCENT)),
            Span::styled(after, Style::default().fg(TEXT)),
        ]);
    }
    if FIELDS[index].kind == FieldKind::Toggle {
        let (label, color) = match app.values[index].as_str() {
            "true" => ("●  ON".to_owned(), ACCENT),
            "false" => ("○  OFF".to_owned(), MUTED),
            value => (format!("?  {value}  ·  press Enter to fix"), WARNING),
        };
        return Line::from(Span::styled(label, Style::default().fg(color)));
    }
    if matches!(FIELDS[index].kind, FieldKind::Choice(_)) {
        return Line::from(Span::styled(
            format!("‹ {} ›", app.values[index]),
            Style::default().fg(ACCENT),
        ));
    }
    if FIELDS[index].key == "level-seed" && app.selected == index {
        let value = if app.values[index].is_empty() {
            "—".to_owned()
        } else {
            app.values[index].clone()
        };
        return Line::from(vec![
            Span::styled(value, Style::default().fg(TEXT)),
            Span::styled("   R: random", Style::default().fg(ACCENT)),
        ]);
    }
    Line::from(Span::styled(
        app.values[index].clone(),
        Style::default().fg(if app.selected == index { TEXT } else { MUTED }),
    ))
}

fn draw_compact(frame: &mut Frame, app: &App, area: Rect) {
    let inner = Rect::new(
        area.x + 1,
        area.y + (area.height - 11) / 2,
        area.width - 2,
        11,
    );
    let status = if app.doctor_checked {
        if app.issues.is_empty() {
            "✓ Ready".to_owned()
        } else {
            format!("⚠ {} issue(s)", app.issues.len())
        }
    } else if app.dirty() {
        "● Unsaved".to_owned()
    } else {
        String::new()
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "⚙ Chunkwake  /  Setup",
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("    {status}"), Style::default().fg(WARNING)),
        ])),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    let tabs = SECTIONS
        .iter()
        .enumerate()
        .map(|(index, (section, label))| {
            let mark = if app.section_dirty(*section) {
                "●"
            } else {
                ""
            };
            Span::styled(
                format!(" {label}{mark} "),
                Style::default()
                    .fg(if index == app.section { TEXT } else { MUTED })
                    .bg(if index == app.section {
                        SELECTED
                    } else {
                        PANEL
                    }),
            )
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(Line::from(tabs)),
        Rect::new(inner.x, inner.y + 1, inner.width, 1),
    );
    let body = Rect::new(inner.x, inner.y + 2, inner.width, inner.height - 4);
    if app.mode == Mode::Review {
        let changed = app.changed_fields();
        frame.render_widget(
            Paragraph::new(format!("Review changes  ·  {} setting(s)", changed.len()))
                .style(Style::default().fg(ACCENT)),
            Rect::new(body.x, body.y, body.width, 1),
        );
        if changed.is_empty() && app.is_new {
            frame.render_widget(
                Paragraph::new("Create server.properties with defaults")
                    .style(Style::default().fg(TEXT)),
                Rect::new(body.x + 1, body.y + 2, body.width - 1, 1),
            );
        }
        for (row, index) in changed.iter().skip(app.review_offset).enumerate() {
            let y = body.y + 1 + row as u16;
            if y >= body.bottom() {
                break;
            }
            frame.render_widget(
                Paragraph::new(format!(
                    "{}  {}: {} → {}",
                    FIELDS[*index].icon,
                    FIELDS[*index].label,
                    if app.initial[*index].is_empty() {
                        "—"
                    } else {
                        &app.initial[*index]
                    },
                    if app.values[*index].is_empty() {
                        "—"
                    } else {
                        &app.values[*index]
                    }
                ))
                .style(Style::default().fg(TEXT)),
                Rect::new(body.x + 1, y, body.width - 1, 1),
            );
        }
    } else if matches!(app.mode, Mode::ConfirmExit | Mode::ConfirmExposure) {
        let (title, detail) = if app.mode == Mode::ConfirmExit {
            (
                "Unsaved changes",
                "S Review & save    D Discard    Esc Back",
            )
        } else {
            ("Offline access from the network", "Y Save    N / Esc Back")
        };
        frame.render_widget(
            Paragraph::new(Text::from(vec![
                Line::from(Span::styled(
                    title,
                    Style::default().fg(WARNING).add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(detail, Style::default().fg(TEXT))),
            ])),
            body,
        );
    } else {
        let (fields, detail) = if body.width >= 90 {
            let left_width = body.width * 57 / 100;
            (
                Rect::new(body.x, body.y, left_width, body.height),
                Some(Rect::new(
                    body.x + left_width + 1,
                    body.y,
                    body.width - left_width - 1,
                    body.height,
                )),
            )
        } else {
            (body, None)
        };
        for (row_index, index) in app.section_indices().into_iter().enumerate() {
            let y = fields.y + row_index as u16;
            if y >= fields.bottom() {
                break;
            }
            let field = &FIELDS[index];
            let selected = index == app.selected;
            let issue = app.issues.iter().any(|issue| issue.key == field.key);
            let mark = if issue {
                "!"
            } else if app.field_dirty(index) {
                "●"
            } else {
                " "
            };
            let row = Rect::new(fields.x, y, fields.width, 1);
            frame.render_widget(
                Block::default().style(Style::default().bg(if selected {
                    SELECTED
                } else {
                    PANEL
                })),
                row,
            );
            frame.render_widget(
                Paragraph::new(if selected { "›" } else { " " }).style(Style::default().fg(ACCENT)),
                Rect::new(row.x, y, 1, 1),
            );
            frame.render_widget(Paragraph::new(field.icon), Rect::new(row.x + 2, y, 3, 1));
            frame.render_widget(
                Paragraph::new(format!("{mark} {}", field.label)).style(Style::default().fg(
                    if issue {
                        WARNING
                    } else if selected {
                        TEXT
                    } else {
                        MUTED
                    },
                )),
                Rect::new(row.x + 6, y, 22, 1),
            );
            frame.render_widget(
                Paragraph::new(display_value(app, index)),
                Rect::new(row.x + 29, y, row.width - 29, 1),
            );
        }
        if let Some(detail) = detail {
            let field = &FIELDS[app.selected];
            let issue = app.issues.iter().find(|issue| issue.key == field.key);
            frame.render_widget(
                Block::default()
                    .borders(Borders::LEFT)
                    .border_style(Style::default().fg(MUTED)),
                detail,
            );
            frame.render_widget(
                Paragraph::new(Text::from(vec![
                    Line::from(Span::styled(
                        field.label,
                        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
                    )),
                    Line::from(Span::styled(field.key, Style::default().fg(MUTED))),
                    Line::from(""),
                    Line::from(Span::styled(
                        issue.map_or(field.help, |issue| &issue.message),
                        Style::default().fg(if issue.is_some() { WARNING } else { MUTED }),
                    )),
                ]))
                .wrap(Wrap { trim: true }),
                Rect::new(detail.x + 2, detail.y, detail.width - 2, detail.height),
            );
        }
    }
    let hint = app
        .issues
        .iter()
        .find(|issue| issue.key == FIELDS[app.selected].key)
        .map_or(FIELDS[app.selected].help, |issue| issue.message.as_str());
    let footer_hint = if body.width >= 90 && matches!(app.mode, Mode::Browse | Mode::Edit) {
        app.path.display().to_string()
    } else {
        hint.to_owned()
    };
    let keys = match app.mode {
        Mode::Browse => "←→ Tabs  ↑↓ Fields  Enter Change  D Check  S Review  Esc Exit",
        Mode::Edit => "Enter Apply  Esc Cancel  ↑↓ Number  ←→ Cursor",
        Mode::Review => "↑↓ Scroll  Enter Save  Esc Back",
        Mode::ConfirmExit => "S Review & save  D Discard  Esc Back",
        Mode::ConfirmExposure => "Y Save  N / Esc Back",
    };
    frame.render_widget(
        Paragraph::new(footer_hint).style(Style::default().fg(MUTED)),
        Rect::new(inner.x, inner.bottom() - 2, inner.width, 1),
    );
    frame.render_widget(
        Paragraph::new(keys).style(Style::default().fg(ACCENT)),
        Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
    );
}

fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    frame.render_widget(Block::default().style(Style::default().bg(BG)), area);
    if area.width < 50 || area.height < 11 {
        frame.render_widget(
            Paragraph::new("Resize the terminal to at least 50 × 11.\nPress Esc to cancel.")
                .style(Style::default().fg(TEXT)),
            area,
        );
        return;
    }
    if area.width < 60 || area.height < 22 {
        frame.render_widget(Block::default().style(Style::default().bg(PANEL)), area);
        draw_compact(frame, app, area);
        return;
    }

    let shell = Rect::new(
        area.x + (area.width - area.width.saturating_sub(2).min(104)) / 2,
        area.y + (area.height - area.height.saturating_sub(2).min(23)) / 2,
        area.width.saturating_sub(2).min(104),
        area.height.saturating_sub(2).min(23),
    );
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(MUTED))
            .style(Style::default().bg(PANEL)),
        shell,
    );
    let content = Rect::new(shell.x + 2, shell.y + 1, shell.width - 4, shell.height - 2);
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(8),
            Constraint::Length(4),
        ])
        .split(content);
    let wide = content.width >= 88;
    let tabs = SECTIONS
        .iter()
        .enumerate()
        .map(|(index, (section, label))| {
            let marker = if app.section_dirty(*section) {
                "●"
            } else {
                ""
            };
            Span::styled(
                format!(" {label}{marker} "),
                Style::default()
                    .fg(if index == app.section { TEXT } else { MUTED })
                    .bg(if index == app.section { SELECTED } else { BG })
                    .add_modifier(if index == app.section {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            )
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(Text::from(vec![
            Line::from(vec![
                Span::styled(
                    "⚙  Chunkwake",
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
                Span::styled("   /   Setup", Style::default().fg(TEXT)),
            ]),
            Line::from(Span::styled(
                app.path.display().to_string(),
                Style::default().fg(MUTED),
            )),
            Line::from(tabs),
            Line::from(Span::styled(
                if app.doctor_checked {
                    if app.issues.is_empty() {
                        "✓ Doctor: ready".to_owned()
                    } else {
                        format!("⚠ Doctor: {} issue(s)", app.issues.len())
                    }
                } else if app.dirty() {
                    if app.is_new && app.changed_fields().is_empty() {
                        "● New configuration · D: check readiness".to_owned()
                    } else {
                        "● Unsaved changes · D: check readiness".to_owned()
                    }
                } else {
                    "D: check readiness".to_owned()
                },
                Style::default().fg(if app.doctor_checked && !app.issues.is_empty() {
                    WARNING
                } else {
                    ACCENT
                }),
            )),
        ])),
        sections[0],
    );

    if matches!(app.mode, Mode::ConfirmExit | Mode::ConfirmExposure) {
        let (title, body) = if app.mode == Mode::ConfirmExit {
            if app.is_new && app.changed_fields().is_empty() {
                (
                    "Exit setup?",
                    "No server.properties has been created. Save the defaults or leave setup.",
                )
            } else {
                (
                    "Unsaved changes",
                    "Review and save the configuration, discard changes, or return to editing.",
                )
            }
        } else {
            (
                "Network exposure",
                "This address exposes offline mode to the network. Player names are not verified.",
            )
        };
        frame.render_widget(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {title} "))
                .border_style(Style::default().fg(WARNING)),
            sections[1],
        );
        frame.render_widget(
            Paragraph::new(body)
                .style(Style::default().fg(TEXT))
                .wrap(Wrap { trim: true }),
            Rect::new(
                sections[1].x + 3,
                sections[1].y + 3,
                sections[1].width - 6,
                sections[1].height - 5,
            ),
        );
    } else if app.mode == Mode::Review {
        let changed = app.changed_fields();
        frame.render_widget(
            Block::default()
                .borders(Borders::ALL)
                .title(" REVIEW CHANGES ")
                .border_style(Style::default().fg(ACCENT)),
            sections[1],
        );
        frame.render_widget(
            Paragraph::new(format!("Review changes  ·  {} setting(s)", changed.len()))
                .style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
            Rect::new(
                sections[1].x + 2,
                sections[1].y + 2,
                sections[1].width - 4,
                1,
            ),
        );
        if changed.is_empty() && app.is_new {
            frame.render_widget(
                Paragraph::new("Create server.properties with the displayed defaults")
                    .style(Style::default().fg(TEXT)),
                Rect::new(
                    sections[1].x + 2,
                    sections[1].y + 4,
                    sections[1].width - 4,
                    1,
                ),
            );
        }
        for (row, index) in changed.iter().skip(app.review_offset).enumerate() {
            let y = sections[1].y + 4 + row as u16;
            if y >= sections[1].bottom() - 1 {
                break;
            }
            frame.render_widget(
                Paragraph::new(format!(
                    "{}  {}: {}  →  {}",
                    FIELDS[*index].icon,
                    FIELDS[*index].label,
                    if app.initial[*index].is_empty() {
                        "—"
                    } else {
                        &app.initial[*index]
                    },
                    if app.values[*index].is_empty() {
                        "—"
                    } else {
                        &app.values[*index]
                    }
                ))
                .style(Style::default().fg(TEXT)),
                Rect::new(sections[1].x + 2, y, sections[1].width - 4, 1),
            );
        }
    } else {
        let (left, right) = if wide {
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(57),
                    Constraint::Length(1),
                    Constraint::Min(0),
                ])
                .split(sections[1]);
            (columns[0], Some(columns[2]))
        } else {
            (sections[1], None)
        };
        frame.render_widget(
            Block::default()
                .borders(Borders::ALL)
                .title(" SETTINGS ")
                .border_style(Style::default().fg(MUTED)),
            left,
        );
        let field_area = Rect::new(left.x + 1, left.y + 1, left.width - 2, left.height - 2);
        let planned = matches!(SECTIONS[app.section].0, Section::World | Section::Gameplay);
        let row_step = if app.section_indices().len() <= 4 && field_area.height >= 10 {
            2
        } else {
            1
        };
        for (row_index, index) in app.section_indices().into_iter().enumerate() {
            let field = &FIELDS[index];
            let row = Rect::new(
                field_area.x,
                field_area.y + 1 + row_index as u16 * row_step,
                field_area.width,
                1,
            );
            if row.y >= field_area.bottom() {
                break;
            }
            let active = app.selected == index;
            let issue = app.issues.iter().any(|issue| issue.key == field.key);
            let marker = if issue {
                "! "
            } else if app.field_dirty(index) {
                "● "
            } else {
                "  "
            };
            frame.render_widget(
                Block::default().style(Style::default().bg(if active { SELECTED } else { BG })),
                row,
            );
            frame.render_widget(
                Paragraph::new(if active { "›" } else { " " }).style(
                    Style::default()
                        .fg(ACCENT)
                        .bg(if active { SELECTED } else { BG }),
                ),
                Rect::new(row.x + 1, row.y, 2, 1),
            );
            frame.render_widget(
                Paragraph::new(field.icon).style(Style::default().fg(TEXT).bg(if active {
                    SELECTED
                } else {
                    BG
                })),
                Rect::new(row.x + 3, row.y, 3, 1),
            );
            frame.render_widget(
                Paragraph::new(format!("{marker}{}", field.label)).style(
                    Style::default()
                        .fg(if issue {
                            WARNING
                        } else if active {
                            TEXT
                        } else {
                            MUTED
                        })
                        .bg(if active { SELECTED } else { BG }),
                ),
                Rect::new(row.x + 7, row.y, 21, 1),
            );
            frame.render_widget(
                Paragraph::new(display_value(app, index)).style(Style::default().bg(if active {
                    SELECTED
                } else {
                    BG
                })),
                Rect::new(row.x + 28, row.y, row.width - 29, 1),
            );
        }
        if let Some(right) = right {
            let field = &FIELDS[app.selected];
            let issue = app.issues.iter().find(|issue| issue.key == field.key);
            frame.render_widget(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" DETAILS ")
                    .border_style(Style::default().fg(if issue.is_some() {
                        WARNING
                    } else {
                        MUTED
                    })),
                right,
            );
            let status = if issue.is_some() {
                "! Needs attention"
            } else if app.field_dirty(app.selected) {
                "● Changed"
            } else if app.doctor_checked {
                "✓ Checked"
            } else {
                "Current value"
            };
            let detail = issue.map_or(field.help, |issue| issue.message.as_str());
            frame.render_widget(
                Paragraph::new(Text::from(vec![
                    Line::from(Span::styled(
                        field.label,
                        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
                    )),
                    Line::from(Span::styled(field.key, Style::default().fg(MUTED))),
                    Line::from(""),
                    Line::from(Span::styled(
                        status,
                        Style::default().fg(if issue.is_some() { WARNING } else { ACCENT }),
                    )),
                    Line::from(""),
                    Line::from(Span::styled(
                        detail,
                        Style::default().fg(if issue.is_some() { WARNING } else { MUTED }),
                    )),
                    Line::from(""),
                    Line::from(Span::styled(
                        if planned {
                            "* Saved for a future version"
                        } else {
                            ""
                        },
                        Style::default().fg(WARNING),
                    )),
                ]))
                .wrap(Wrap { trim: true }),
                Rect::new(right.x + 2, right.y + 2, right.width - 4, right.height - 4),
            );
        }
    }

    let (hint, keys, important) = match app.mode {
        Mode::Browse => (
            app.issues
                .iter()
                .find(|issue| issue.key == FIELDS[app.selected].key)
                .map(|issue| issue.message.as_str())
                .or(app.message.as_deref())
                .unwrap_or(FIELDS[app.selected].help),
            if FIELDS[app.selected].key == "level-seed" {
                "R Random  Enter Type  D Check  S Review  Esc Exit"
            } else if matches!(FIELDS[app.selected].kind, FieldKind::Number { .. }) {
                "←→ Tabs  ↑↓ Fields  Enter Edit  D Check  S Review  Esc Exit"
            } else {
                "←→ Tabs  ↑↓ Fields  Enter/Space Change  D Check  S Review  Esc Exit"
            },
            app.issues
                .iter()
                .any(|issue| issue.key == FIELDS[app.selected].key)
                || (app.message.is_some() && !app.doctor_checked),
        ),
        Mode::Edit => (
            app.message.as_deref().unwrap_or(FIELDS[app.selected].help),
            if matches!(FIELDS[app.selected].kind, FieldKind::Number { .. }) {
                "↑↓ ±1    Shift+↑↓ / PgUp/PgDn ±10    Enter Apply    Esc Cancel"
            } else {
                "Enter Apply    Esc Cancel    ←→ Cursor"
            },
            app.message.is_some(),
        ),
        Mode::Review => (
            "Check these values before writing server.properties.",
            "↑↓ Scroll    Enter / S Save    Esc Back",
            false,
        ),
        Mode::ConfirmExit => (
            if app.is_new && app.changed_fields().is_empty() {
                "The new configuration has not been created yet."
            } else {
                "● Unsaved changes. Choose what to do with them."
            },
            "S Review & save    D Discard    Esc Back",
            true,
        ),
        Mode::ConfirmExposure => (
            "⚠  This change exposes offline mode to the network. Player names are not verified.",
            "Y / Enter Save    N / Esc Back",
            true,
        ),
    };
    let footer = sections[2];
    frame.render_widget(
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(MUTED)),
        footer,
    );
    let footer_lines = if wide && matches!(app.mode, Mode::Browse | Mode::Edit) {
        let navigation = if app.mode == Mode::Edit {
            "EDIT  ←→ cursor   ↑↓ numbers   Shift+↑↓ / PgUp/PgDn ±10"
        } else {
            "NAVIGATE  ←→ / Tab sections   ↑↓ fields"
        };
        let actions = if app.mode == Mode::Edit {
            "Enter apply   Esc cancel"
        } else if FIELDS[app.selected].key == "level-seed" {
            "Enter type   R random   D check   S review   Esc exit"
        } else {
            "Enter/Space change   D check   S review   Esc exit"
        };
        vec![
            Line::from(Span::styled(navigation, Style::default().fg(MUTED))),
            Line::from(Span::styled(actions, Style::default().fg(ACCENT))),
        ]
    } else {
        vec![
            Line::from(Span::styled(
                hint.to_owned(),
                Style::default().fg(if important { WARNING } else { MUTED }),
            )),
            Line::from(Span::styled(keys, Style::default().fg(ACCENT))),
        ]
    };
    frame.render_widget(
        Paragraph::new(Text::from(footer_lines))
            .style(Style::default().bg(PANEL))
            .wrap(Wrap { trim: true }),
        Rect::new(
            footer.x + 1,
            footer.y + 1,
            footer.width - 2,
            footer.height - 1,
        ),
    );
}

pub fn run(dir: &Path) -> Result<(), String> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err("setup requires an interactive terminal; run it in a Windows/Linux terminal before starting the service".into());
    }
    let mut app = App::open(dir)?;
    let mut terminal = ratatui::try_init().map_err(|e| format!("initialize terminal: {e}"))?;
    let result = (|| -> Result<Action, String> {
        loop {
            terminal
                .draw(|frame| draw(frame, &app))
                .map_err(|e| format!("draw setup screen: {e}"))?;
            let event = event::read().map_err(|e| format!("read keyboard: {e}"))?;
            if let Event::Key(key) = event {
                match app.handle_key(key) {
                    Action::Continue => {}
                    action => return Ok(action),
                }
            }
        }
    })();
    ratatui::try_restore().map_err(|e| format!("restore terminal: {e}"))?;
    match result? {
        Action::Save => {
            app.save()?;
            println!("✓ Configuration saved to {}", app.path.display());
            println!(
                "  Run `chunkwake doctor --dir {}` to check the jar.",
                dir.display()
            );
        }
        Action::Quit => println!("Setup cancelled. No changes saved."),
        Action::Continue => unreachable!(),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn app(text: &str) -> App {
        App::from_text(PathBuf::from("server.properties"), text.to_owned()).unwrap()
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn editing_preserves_other_lines() {
        let mut app = app("# note\nserver-port=25565\nunknown=keep\n");
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Enter));
        app.edit_value = "25570".into();
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.mode, Mode::Browse);
        assert!(
            app.draft()
                .unwrap()
                .contains("# note\nserver-port=25570\nunknown=keep")
        );
    }

    #[test]
    fn invalid_edit_stays_open() {
        let mut app = app(config::DEFAULT_PROPERTIES);
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Enter));
        app.edit_value = "99999".into();
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.mode, Mode::Edit);
        assert!(app.message.as_deref().unwrap().contains("server-port"));
        assert_eq!(app.values[1], "25565");
    }

    #[test]
    fn public_address_requires_extra_confirmation() {
        let mut app = app(config::DEFAULT_PROPERTIES);
        app.handle_key(key(KeyCode::Enter));
        app.edit_value = "0.0.0.0".into();
        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Char('s')));
        assert_eq!(app.mode, Mode::Review);
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.mode, Mode::ConfirmExposure);
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.mode, Mode::Review);
    }

    #[test]
    fn editing_unicode_at_cursor() {
        let mut app = app(config::DEFAULT_PROPERTIES);
        app.selected = 2;
        app.handle_key(key(KeyCode::Enter));
        app.edit_value = "Привет".into();
        app.cursor = 3;
        app.handle_key(key(KeyCode::Char('!')));
        assert_eq!(app.edit_value, "При!вет");
        app.handle_key(key(KeyCode::Backspace));
        assert_eq!(app.edit_value, "Привет");
    }

    #[test]
    fn renders_in_normal_and_small_terminals() {
        let base = app(config::DEFAULT_PROPERTIES);
        for (width, height) in [(50, 11), (60, 17), (120, 17), (80, 24), (120, 30), (40, 12)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|frame| draw(frame, &base)).unwrap();
            let rendered = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();
            if width >= 50 {
                assert!(rendered.contains("Listen address"), "{width}x{height}");
                assert!(
                    !rendered.contains("Resize the terminal"),
                    "{width}x{height}"
                );
            }
        }
        let mut gameplay = app(config::DEFAULT_PROPERTIES);
        gameplay.section = 4;
        gameplay.selected = *gameplay.section_indices().last().unwrap();
        let mut terminal = Terminal::new(TestBackend::new(50, 11)).unwrap();
        terminal.draw(|frame| draw(frame, &gameplay)).unwrap();
        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("Spawn protection"));
    }

    #[test]
    fn saving_existing_config_keeps_unrelated_lines() {
        let path =
            std::env::temp_dir().join(format!("chunkwake-tui-{}.properties", std::process::id()));
        let mut app = App::from_text(
            path.clone(),
            "# note\ncustom=keep\nserver-port=25565\n".into(),
        )
        .unwrap();
        app.values[1] = "25570".into();
        app.save().unwrap();
        let saved = config::read(&path).unwrap();
        assert!(saved.contains("# note\ncustom=keep\nserver-port=25570"));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn invalid_existing_value_can_be_repaired_in_tui() {
        let mut app = app("server-port=bad\nonline-mode=yes\n");
        assert!(app.message.is_some());
        app.selected = 1;
        app.handle_key(key(KeyCode::Enter));
        app.edit_value = "25565".into();
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.mode, Mode::Browse);
        assert!(app.validate().is_err());
        app.selected = 4;
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.values[4], "true");
        app.handle_key(key(KeyCode::Char(' ')));
        assert_eq!(app.values[4], "false");
        assert!(app.validate().is_ok());
    }

    #[test]
    fn toggle_and_save_goes_through_review() {
        let mut app = app(config::DEFAULT_PROPERTIES);
        app.selected = 4;
        app.handle_key(key(KeyCode::Char(' ')));
        assert_eq!(app.values[4], "true");
        assert_eq!(app.mode, Mode::Browse);
        assert_eq!(app.handle_key(key(KeyCode::Char('s'))), Action::Continue);
        assert_eq!(app.mode, Mode::Review);
        assert_eq!(app.handle_key(key(KeyCode::Enter)), Action::Save);
        assert!(app.draft().unwrap().contains("online-mode=true"));
    }

    #[test]
    fn sections_and_planned_settings_are_keyboard_accessible() {
        let mut app = app(config::DEFAULT_PROPERTIES);
        app.handle_key(key(KeyCode::Right));
        assert_eq!(SECTIONS[app.section].0, Section::Server);
        app.handle_key(key(KeyCode::Tab));
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(SECTIONS[app.section].0, Section::World);
        assert_eq!(FIELDS[app.selected].key, "level-name");
        app.handle_key(key(KeyCode::Right));
        assert_eq!(FIELDS[app.selected].key, "gamemode");
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.values[app.selected], "creative");
        app.handle_key(key(KeyCode::Down));
        assert_eq!(FIELDS[app.selected].key, "difficulty");
        app.handle_key(key(KeyCode::Char(' ')));
        assert_eq!(app.values[app.selected], "normal");
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(FIELDS[app.selected].key, "hardcore");
        assert_eq!(app.values[app.selected], "true");
        let draft = app.draft().unwrap();
        assert!(draft.contains("gamemode=creative"));
        assert!(draft.contains("difficulty=normal"));
        assert!(draft.contains("hardcore=true"));
        app.validate().unwrap();
    }

    #[test]
    fn vertical_navigation_wraps_within_section() {
        let mut app = app(config::DEFAULT_PROPERTIES);
        let network = app.section_indices();
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.selected, *network.last().unwrap());
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.selected, network[0]);

        app.handle_key(key(KeyCode::Right));
        let server = app.section_indices();
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.selected, *server.last().unwrap());
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.selected, server[0]);
    }

    #[test]
    fn numeric_arrows_adjust_and_respect_limits() {
        let mut app = app(config::DEFAULT_PROPERTIES);
        app.selected = 3; // max-players
        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.edit_value, "21");
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::SHIFT));
        assert_eq!(app.edit_value, "11");
        app.handle_key(key(KeyCode::PageUp));
        assert_eq!(app.edit_value, "21");
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.values[3], "21");

        app.selected = 1; // server-port
        app.handle_key(key(KeyCode::Enter));
        app.edit_value = "65535".into();
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.edit_value, "65535");
        app.edit_value = "1".into();
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.edit_value, "1");
        app.edit_value = "bad".into();
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.edit_value, "bad");
        assert!(app.message.as_deref().unwrap().contains("server-port"));
    }

    #[test]
    fn random_seed_is_generated_only_on_seed_row_and_saved() {
        let mut app = app(config::DEFAULT_PROPERTIES);
        app.section = 3;
        app.selected = app.section_indices()[1];
        assert_eq!(FIELDS[app.selected].key, "level-seed");
        app.handle_key(key(KeyCode::Char('r')));
        let seed = app.values[app.selected].clone();
        assert!(seed.parse::<i64>().is_ok());
        assert!(app.draft().unwrap().contains(&format!("level-seed={seed}")));
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Char('r')));
        assert_eq!(app.values[app.section_indices()[1]], seed);
    }

    #[test]
    fn unsaved_exit_requires_a_choice() {
        let mut app = app(config::DEFAULT_PROPERTIES);
        app.selected = 3;
        app.handle_key(key(KeyCode::Enter));
        app.edit_value = "21".into();
        app.handle_key(key(KeyCode::Enter));
        assert!(app.field_dirty(3));
        assert!(app.section_dirty(Section::Server));
        assert_eq!(app.handle_key(key(KeyCode::Esc)), Action::Continue);
        assert_eq!(app.mode, Mode::ConfirmExit);
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.mode, Mode::Browse);
        app.handle_key(key(KeyCode::Esc));
        app.handle_key(key(KeyCode::Char('s')));
        assert_eq!(app.mode, Mode::Review);
        assert_eq!(app.changed_fields(), vec![3]);
        app.handle_key(key(KeyCode::Esc));
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.handle_key(key(KeyCode::Char('d'))), Action::Quit);
    }

    #[test]
    fn doctor_marks_missing_jar_and_clears_after_change() {
        let path = std::env::temp_dir().join(format!(
            "chunkwake-doctor-ui-{}-missing/server.properties",
            std::process::id()
        ));
        let mut app = App::from_text(path, config::DEFAULT_PROPERTIES.into()).unwrap();
        app.handle_key(key(KeyCode::Char('d')));
        assert!(app.doctor_checked);
        assert!(app.issues.iter().any(|issue| issue.key == "vanilla-jar"));
        app.selected = 6;
        app.handle_key(key(KeyCode::Enter));
        app.edit_value = "other.jar".into();
        app.handle_key(key(KeyCode::Enter));
        assert!(!app.doctor_checked);
        assert!(app.issues.is_empty());
    }
}
