//! Полноэкранный мастер настройки сервера.

use crate::config::{self, Properties};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Paragraph, Wrap};
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
        kind: FieldKind::Text,
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
        kind: FieldKind::Text,
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
        kind: FieldKind::Text,
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
        help: "Saved for a future world implementation; currently has no effect.",
        kind: FieldKind::Text,
        section: Section::World,
    },
    Field {
        key: "view-distance",
        icon: "◉",
        label: "View distance",
        help: "2–32 chunks. Saved for a future world implementation; currently has no effect.",
        kind: FieldKind::Text,
        section: Section::World,
    },
    Field {
        key: "simulation-distance",
        icon: "◌",
        label: "Simulation distance",
        help: "2–32 chunks. Saved for a future world implementation; currently has no effect.",
        kind: FieldKind::Text,
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
        kind: FieldKind::Text,
        section: Section::Gameplay,
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Browse,
    Edit,
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
    section: usize,
    selected: usize,
    edit_value: String,
    cursor: usize,
    mode: Mode,
    message: Option<String>,
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
        let original = if path.exists() {
            config::read(&path)?
        } else {
            config::DEFAULT_PROPERTIES.to_owned()
        };
        Self::from_text(path, original)
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
            section: 0,
            selected: 0,
            edit_value: String::new(),
            cursor: 0,
            mode: Mode::Browse,
            message,
        })
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
                    FieldKind::Text => {
                        self.edit_value = self.values[self.selected].clone();
                        self.cursor = self.edit_value.chars().count();
                        self.mode = Mode::Edit;
                        self.message = None;
                    }
                },
                KeyCode::Char(' ') => match FIELDS[self.selected].kind {
                    FieldKind::Toggle => self.toggle(),
                    FieldKind::Choice(options) => self.cycle_choice(options),
                    FieldKind::Text => {}
                },
                KeyCode::Char('s') | KeyCode::F(2) => match self.validate() {
                    Ok(props) => {
                        self.message = None;
                        if self.needs_exposure_confirmation(&props) {
                            self.mode = Mode::ConfirmExposure;
                        } else {
                            return Action::Save;
                        }
                    }
                    Err(error) => self.message = Some(error),
                },
                KeyCode::Esc | KeyCode::Char('q') => return Action::Quit,
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
            Mode::ConfirmExposure => match key.code {
                KeyCode::Char('y') | KeyCode::Enter => return Action::Save,
                KeyCode::Char('n') | KeyCode::Esc => self.mode = Mode::Browse,
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
    }

    fn cycle_choice(&mut self, options: &'static [&'static str]) {
        let current = options
            .iter()
            .position(|&option| option == self.values[self.selected]);
        self.values[self.selected] =
            options[current.map_or(0, |index| (index + 1) % options.len())].into();
        self.message = None;
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
    Line::from(Span::styled(
        app.values[index].clone(),
        Style::default().fg(if app.selected == index { TEXT } else { MUTED }),
    ))
}

fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    frame.render_widget(Block::default().style(Style::default().bg(BG)), area);
    if area.width < 55 || area.height < 16 {
        frame.render_widget(
            Paragraph::new("Resize the terminal to at least 55 × 16.\nPress Esc to cancel.")
                .style(Style::default().fg(TEXT)),
            area,
        );
        return;
    }

    let content = Rect::new(area.x + 2, area.y, area.width - 4, area.height);
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(9),
            Constraint::Length(4),
        ])
        .split(content);
    let tabs = SECTIONS
        .iter()
        .enumerate()
        .map(|(index, (_, label))| {
            Span::styled(
                format!("  {label}  "),
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
        ])),
        sections[0],
    );

    let planned = matches!(SECTIONS[app.section].0, Section::World | Section::Gameplay);
    frame.render_widget(
        Paragraph::new(if planned {
            "Settings  ·  * Saved for future versions; no effect yet"
        } else {
            "Settings"
        })
        .style(
            Style::default()
                .fg(if planned { WARNING } else { MUTED })
                .add_modifier(Modifier::BOLD),
        ),
        Rect::new(sections[1].x, sections[1].y, sections[1].width, 1),
    );
    for (row_index, index) in app.section_indices().into_iter().enumerate() {
        let field = &FIELDS[index];
        let row = Rect::new(
            sections[1].x,
            sections[1].y + 1 + row_index as u16,
            sections[1].width,
            1,
        );
        if row.y >= sections[1].bottom() {
            break;
        }
        let active = app.selected == index;
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
            Paragraph::new(field.label).style(
                Style::default()
                    .fg(if active { TEXT } else { MUTED })
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

    let (hint, keys, important) = match app.mode {
        Mode::Browse => (
            app.message.as_deref().unwrap_or(FIELDS[app.selected].help),
            "←→ / Tab Sections    ↑↓ Move    Enter/Space Change    S Save    Esc Quit",
            app.message.is_some(),
        ),
        Mode::Edit => (
            app.message.as_deref().unwrap_or(FIELDS[app.selected].help),
            "Enter Apply    Esc Cancel    ←→ Cursor",
            app.message.is_some(),
        ),
        Mode::ConfirmExposure => (
            "⚠  This change exposes offline mode to the network. Player names are not verified.",
            "Y / Enter Save    N / Esc Back",
            true,
        ),
    };
    frame.render_widget(
        Paragraph::new(Text::from(vec![
            Line::from(Span::styled(
                hint.to_owned(),
                Style::default().fg(if important { WARNING } else { MUTED }),
            )),
            Line::from(""),
            Line::from(Span::styled(keys, Style::default().fg(ACCENT))),
        ]))
        .style(Style::default().bg(PANEL))
        .wrap(Wrap { trim: true }),
        sections[2],
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
        assert_eq!(app.mode, Mode::ConfirmExposure);
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.mode, Mode::Browse);
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
        let app = app(config::DEFAULT_PROPERTIES);
        for (width, height) in [(60, 17), (80, 24), (120, 30), (40, 12)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|frame| draw(frame, &app)).unwrap();
        }
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
    fn toggle_and_save_need_no_text_input_or_extra_dialog() {
        let mut app = app(config::DEFAULT_PROPERTIES);
        app.selected = 4;
        app.handle_key(key(KeyCode::Char(' ')));
        assert_eq!(app.values[4], "true");
        assert_eq!(app.mode, Mode::Browse);
        assert_eq!(app.handle_key(key(KeyCode::Char('s'))), Action::Save);
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
}
