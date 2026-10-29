//! Interactive front end for the lab. The same Args are used by the headless CLI.
use super::{Args, BenchProfile, Mode, default_java};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::time::Duration;

const LABELS: [&str; 14] = [
    "◆ Scenario",
    "⚙ Chunkwake",
    "◈ Vanilla jar",
    "☕ Java",
    "▣ Reports",
    "◇ Data cache",
    "✓ Vanilla EULA",
    "▤ Keep worlds",
    "◉ Profile",
    "♟ Bots",
    "◷ Warmup (s)",
    "◴ Duration (s)",
    "↻ Repetitions",
    "▶ Start test",
];

struct Screen {
    args: Args,
    selected: usize,
    editing: Option<String>,
}

struct RestoreTerminal;
impl Drop for RestoreTerminal {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

pub fn run() -> Result<Args, String> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(
            "TUI needs an interactive terminal; use `chunkwake-lab --help` for headless commands"
                .into(),
        );
    }
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let chunkwake = default_chunkwake_path(&executable);
    let mut screen = Screen {
        args: Args {
            mode: Mode::Compare,
            chunkwake,
            jar: PathBuf::new(),
            java: default_java(),
            work_dir: PathBuf::from("target/chunkwake-lab"),
            generated_cache: None,
            accept_eula: false,
            keep_worlds: false,
            bots: 20,
            warmup: Duration::from_secs(10),
            duration: Duration::from_secs(30),
            repetitions: 3,
            profile: BenchProfile::Movement,
        },
        selected: 0,
        editing: None,
    };
    enable_raw_mode().map_err(|e| e.to_string())?;
    execute!(io::stdout(), EnterAlternateScreen).map_err(|e| e.to_string())?;
    let _restore = RestoreTerminal;
    let mut terminal =
        Terminal::new(CrosstermBackend::new(io::stdout())).map_err(|e| e.to_string())?;
    loop {
        terminal
            .draw(|f| draw(f, &screen))
            .map_err(|e| e.to_string())?;
        let Event::Key(key) = event::read().map_err(|e| e.to_string())? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if let Some(draft) = &mut screen.editing {
            match key.code {
                KeyCode::Enter => {
                    let value = std::mem::take(draft);
                    screen.set_path(value);
                    screen.editing = None;
                }
                KeyCode::Esc => screen.editing = None,
                KeyCode::Backspace => {
                    draft.pop();
                }
                KeyCode::Char(c) => draft.push(c),
                _ => {}
            }
            continue;
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return Err("cancelled".into()),
            KeyCode::Up => screen.select_move(-1),
            KeyCode::Down => screen.select_move(1),
            KeyCode::Tab => screen.selected = 13,
            KeyCode::Left => screen.change(-1),
            KeyCode::Right | KeyCode::Char(' ') => screen.change(1),
            KeyCode::Enter if screen.selected == 13 => return Ok(screen.args),
            KeyCode::Enter if (1..=5).contains(&screen.selected) => {
                screen.editing = Some(screen.path_value())
            }
            KeyCode::Enter => screen.change(1),
            _ => {}
        }
    }
}

fn default_chunkwake_path(executable: &Path) -> PathBuf {
    let binary_dir = executable
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let release_dir = if binary_dir.file_name().is_some_and(|name| name == "debug") {
        binary_dir.parent().unwrap_or(binary_dir).join("release")
    } else {
        binary_dir.to_path_buf()
    };
    release_dir.join(if cfg!(windows) {
        "chunkwake.exe"
    } else {
        "chunkwake"
    })
}

impl Screen {
    fn select_move(&mut self, direction: i32) {
        loop {
            self.selected =
                (self.selected as i32 + direction).rem_euclid(LABELS.len() as i32) as usize;
            if self.args.mode == Mode::Bench || !(8..=12).contains(&self.selected) {
                break;
            }
        }
    }

    fn path_value(&self) -> String {
        match self.selected {
            1 => self.args.chunkwake.display().to_string(),
            2 => self.args.jar.display().to_string(),
            3 => self.args.java.display().to_string(),
            4 => self.args.work_dir.display().to_string(),
            5 => self
                .args
                .generated_cache
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            _ => String::new(),
        }
    }

    fn set_path(&mut self, value: String) {
        match self.selected {
            1 => self.args.chunkwake = PathBuf::from(value),
            2 => self.args.jar = PathBuf::from(value),
            3 => self.args.java = PathBuf::from(value),
            4 => self.args.work_dir = PathBuf::from(value),
            5 => self.args.generated_cache = (!value.is_empty()).then(|| PathBuf::from(value)),
            _ => {}
        }
    }

    fn change(&mut self, direction: i32) {
        let step = direction as i64;
        match self.selected {
            0 => {
                self.args.mode = match (self.args.mode, direction) {
                    (Mode::Smoke, -1) => Mode::Bench,
                    (Mode::Smoke, _) => Mode::Compare,
                    (Mode::Compare, -1) => Mode::Smoke,
                    (Mode::Compare, _) => Mode::Bench,
                    (Mode::Bench, -1) => Mode::Compare,
                    (Mode::Bench, _) => Mode::Smoke,
                }
            }
            6 => self.args.accept_eula = !self.args.accept_eula,
            7 => self.args.keep_worlds = !self.args.keep_worlds,
            8 => {
                let profiles = [
                    BenchProfile::All,
                    BenchProfile::JoinBurst,
                    BenchProfile::Movement,
                    BenchProfile::Exploration,
                    BenchProfile::Redstone,
                    BenchProfile::Cows,
                ];
                let current = profiles
                    .iter()
                    .position(|p| *p == self.args.profile)
                    .unwrap_or(0);
                self.args.profile =
                    profiles[(current as i64 + step).rem_euclid(profiles.len() as i64) as usize];
            }
            9 => self.args.bots = (self.args.bots as i64 + step).clamp(1, 1000) as usize,
            10 => {
                self.args.warmup = Duration::from_secs(
                    (self.args.warmup.as_secs() as i64 + step).clamp(0, 3600) as u64,
                )
            }
            11 => {
                self.args.duration = Duration::from_secs(
                    (self.args.duration.as_secs() as i64 + step).clamp(1, 3600) as u64,
                )
            }
            12 => {
                self.args.repetitions = (self.args.repetitions as i64 + step).clamp(1, 20) as usize
            }
            _ => {}
        }
    }

    fn value(&self, index: usize) -> String {
        match index {
            0 => format!(
                "‹ {} ›",
                match self.args.mode {
                    Mode::Smoke => "Smoke · only Chunkwake",
                    Mode::Compare => "Compare · both servers",
                    Mode::Bench => "Bench · both servers",
                }
            ),
            1 => self.args.chunkwake.display().to_string(),
            2 => {
                if self.args.jar.as_os_str().is_empty() {
                    "Auto download · Minecraft 26.3".into()
                } else {
                    self.args.jar.display().to_string()
                }
            }
            3 => self.args.java.display().to_string(),
            4 => self.args.work_dir.display().to_string(),
            5 => self
                .args
                .generated_cache
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "Auto · shared lab cache".into()),
            6 => if self.args.accept_eula {
                "● Accepted"
            } else {
                "○ Not accepted"
            }
            .into(),
            7 => if self.args.keep_worlds {
                "● Yes"
            } else {
                "○ No"
            }
            .into(),
            8 => format!("‹ {} ›", self.args.profile.label()),
            9 => self.args.bots.to_string(),
            10 => self.args.warmup.as_secs().to_string(),
            11 => self.args.duration.as_secs().to_string(),
            12 => self.args.repetitions.to_string(),
            13 => "Press Enter to run".into(),
            _ => String::new(),
        }
    }
}

fn draw(f: &mut ratatui::Frame, screen: &Screen) {
    let area = f.area();
    if area.width < 48 || area.height < 12 {
        f.render_widget(
            Paragraph::new("Resize terminal to at least 48 × 12. Esc exits."),
            area,
        );
        return;
    }
    let panel = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width - 2,
        height: area.height - 2,
    };
    f.render_widget(
        Block::default()
            .title(" ◈ Chunkwake Lab  /  Test server ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
        panel,
    );
    let inner = Rect {
        x: panel.x + 2,
        y: panel.y + 1,
        width: panel.width - 4,
        height: panel.height - 2,
    };
    let header = Paragraph::new("Choose scenario and settings · missing jar downloads from Mojang")
        .style(Style::default().fg(Color::Gray));
    f.render_widget(
        header,
        Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: 1,
        },
    );
    let groups = [
        ("RUN", 0..1),
        ("FILES", 1..6),
        ("OPTIONS", 6..8),
        ("LOAD", 8..13),
        ("READY", 13..14),
    ];
    let mut rows = Vec::new();
    for (title, range) in groups {
        if title == "LOAD" && screen.args.mode != Mode::Bench {
            continue;
        }
        rows.push((None, title));
        for index in range {
            rows.push((Some(index), ""));
        }
    }
    let selected_row = rows
        .iter()
        .position(|(index, _)| *index == Some(screen.selected))
        .unwrap_or(0);
    let start_y = inner.y + 2;
    let visible = (panel.y + panel.height - 2).saturating_sub(start_y) as usize;
    let top = selected_row
        .saturating_sub(visible / 2)
        .min(rows.len().saturating_sub(visible));
    for (offset, (index, title)) in rows.iter().enumerate().skip(top).take(visible) {
        let y = start_y + (offset - top) as u16;
        if let Some(index) = index {
            let value = if screen.selected == *index {
                screen
                    .editing
                    .as_ref()
                    .cloned()
                    .unwrap_or_else(|| screen.value(*index))
            } else {
                screen.value(*index)
            };
            let line = Line::from(vec![
                Span::styled(
                    if screen.selected == *index {
                        "› "
                    } else {
                        "  "
                    },
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled(
                    format!("{:<19}", LABELS[*index]),
                    Style::default().fg(if screen.selected == *index {
                        Color::White
                    } else {
                        Color::Gray
                    }),
                ),
                Span::styled(
                    value,
                    Style::default().fg(if screen.selected == *index {
                        Color::White
                    } else {
                        Color::Cyan
                    }),
                ),
            ]);
            let style = if screen.selected == *index {
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            f.render_widget(
                Paragraph::new(line).style(style),
                Rect {
                    x: inner.x,
                    y,
                    width: inner.width,
                    height: 1,
                },
            );
        } else {
            f.render_widget(
                Paragraph::new(*title).style(
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Rect {
                    x: inner.x,
                    y,
                    width: inner.width,
                    height: 1,
                },
            );
        }
    }
    let footer = if screen.editing.is_some() {
        "ENTER Save path    ESC Cancel edit"
    } else {
        "↑↓ Field    ←→ Change    ENTER Edit/toggle    TAB Start    ESC Exit"
    };
    f.render_widget(
        Paragraph::new(footer).style(Style::default().fg(Color::Cyan)),
        Rect {
            x: inner.x,
            y: panel.y + panel.height - 2,
            width: inner.width,
            height: 1,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_lab_defaults_to_release_server() {
        let path = default_chunkwake_path(Path::new("target/debug/chunkwake-lab.exe"));
        assert_eq!(
            path,
            PathBuf::from("target/release").join(if cfg!(windows) {
                "chunkwake.exe"
            } else {
                "chunkwake"
            })
        );
    }
}
