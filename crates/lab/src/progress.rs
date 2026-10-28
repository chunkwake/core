//! Live terminal feedback shared by headless runs and the interactive dashboard.

use super::Mode;
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use std::io::{self, IsTerminal, Write};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

enum Update {
    Run {
        server: String,
        iteration: usize,
        total: usize,
    },
    Message {
        server: Option<String>,
        text: String,
    },
}

pub struct Progress {
    tx: Option<Sender<Update>>,
    worker: Option<JoinHandle<()>>,
    started: Instant,
}

impl Progress {
    pub fn new(interactive: bool, mode: Mode) -> Result<Self, String> {
        let started = Instant::now();
        if interactive && io::stdout().is_terminal() {
            let (tx, rx) = mpsc::channel();
            let worker = thread::spawn(move || dashboard(rx, mode));
            Ok(Self {
                tx: Some(tx),
                worker: Some(worker),
                started,
            })
        } else {
            Ok(Self {
                tx: None,
                worker: None,
                started,
            })
        }
    }

    pub fn run(&self, server: &str, iteration: usize, total: usize) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(Update::Run {
                server: server.into(),
                iteration,
                total,
            });
        } else {
            eprintln!(
                "[{:>5}s] {server} · run {iteration}/{total}",
                self.started.elapsed().as_secs()
            );
        }
    }

    pub fn update(&self, server: Option<&str>, text: &str) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(Update::Message {
                server: server.map(str::to_owned),
                text: text.into(),
            });
        } else {
            eprintln!(
                "[{:>5}s] {}{text}",
                self.started.elapsed().as_secs(),
                server.map(|s| format!("{s} · ")).unwrap_or_default()
            );
            let _ = io::stderr().flush();
        }
    }
}

impl Drop for Progress {
    fn drop(&mut self) {
        self.tx.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[derive(Default)]
struct Card {
    run: String,
    status: String,
    history: Vec<String>,
}

impl Card {
    fn push(&mut self, text: String) {
        self.status.clone_from(&text);
        let heartbeat = |value: &str| {
            [
                "Waiting for readiness",
                "Client scenario running",
                "Measuring",
                "Warmup",
                "Connected",
                "Downloading server.jar",
            ]
            .iter()
            .find(|prefix| value.starts_with(**prefix))
            .copied()
        };
        if self.history.last().and_then(|last| heartbeat(last)) == heartbeat(&text)
            && heartbeat(&text).is_some()
        {
            *self.history.last_mut().unwrap() = text;
        } else {
            self.history.push(text);
        }
        if self.history.len() > 6 {
            self.history.remove(0);
        }
    }
}

struct Dashboard {
    started: Instant,
    mode: Mode,
    phase: String,
    chunkwake: Card,
    vanilla: Card,
}

impl Dashboard {
    fn apply(&mut self, update: Update) {
        match update {
            Update::Run {
                server,
                iteration,
                total,
            } => {
                let card = self.card(&server);
                card.run = format!("run {iteration}/{total}");
                card.history.clear();
                card.push("Preparing workspace".into());
                self.phase = format!("Running {server} · {iteration}/{total}");
            }
            Update::Message { server, text } => {
                if let Some(server) = server {
                    self.card(&server).push(text);
                } else {
                    self.phase = text;
                }
            }
        }
    }

    fn card(&mut self, server: &str) -> &mut Card {
        if server == "vanilla" {
            &mut self.vanilla
        } else {
            &mut self.chunkwake
        }
    }
}

struct Restore;
impl Drop for Restore {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

fn dashboard(rx: Receiver<Update>, mode: Mode) {
    if enable_raw_mode().is_err() || execute!(io::stdout(), EnterAlternateScreen).is_err() {
        return;
    }
    let _restore = Restore;
    let Ok(mut terminal) = Terminal::new(CrosstermBackend::new(io::stdout())) else {
        return;
    };
    let mut state = Dashboard {
        started: Instant::now(),
        mode,
        phase: "Preparing test".into(),
        chunkwake: Card {
            status: "Waiting".into(),
            ..Card::default()
        },
        vanilla: Card {
            status: if mode == Mode::Smoke {
                "Not used in smoke".into()
            } else {
                "Waiting".into()
            },
            ..Card::default()
        },
    };
    loop {
        while let Ok(update) = rx.try_recv() {
            state.apply(update);
        }
        if terminal.draw(|f| draw(f, &state)).is_err() {
            break;
        }
        match rx.recv_timeout(Duration::from_millis(150)) {
            Ok(update) => state.apply(update),
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
}

fn draw(f: &mut ratatui::Frame, state: &Dashboard) {
    let area = f.area();
    if area.width < 54 || area.height < 11 {
        f.render_widget(
            Paragraph::new("Chunkwake Lab is running. Enlarge the terminal for details."),
            area,
        );
        return;
    }
    let title = Rect {
        x: area.x + 1,
        y: area.y,
        width: area.width - 2,
        height: 3,
    };
    f.render_widget(
        Paragraph::new(format!(
            "  ◈ Chunkwake Lab   ·   {}\n  {}   ·   {} s elapsed",
            state.phase,
            match state.mode {
                Mode::Smoke => "SMOKE",
                Mode::Compare => "COMPARE",
                Mode::Bench => "BENCH",
            },
            state.started.elapsed().as_secs()
        ))
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        title,
    );
    let gap = 2;
    let width = (area.width - 4 - gap) / 2;
    let height = area.height - 5;
    let left = Rect {
        x: area.x + 1,
        y: area.y + 3,
        width,
        height,
    };
    let right = Rect {
        x: left.x + width + gap,
        y: left.y,
        width: area.width - 3 - width - gap,
        height,
    };
    card(f, left, "⚙ Chunkwake", &state.chunkwake, Color::Cyan);
    card(f, right, "◈ Vanilla", &state.vanilla, Color::Yellow);
    let footer = Rect {
        x: area.x + 2,
        y: area.y + area.height - 2,
        width: area.width - 4,
        height: 1,
    };
    f.render_widget(
        Paragraph::new("Live progress · report is saved when the test finishes")
            .style(Style::default().fg(Color::Gray)),
        footer,
    );
}

fn card(f: &mut ratatui::Frame, rect: Rect, name: &str, card: &Card, accent: Color) {
    f.render_widget(
        Block::default()
            .title(format!(" {name} "))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(accent)),
        rect,
    );
    let x = rect.x + 2;
    let width = rect.width.saturating_sub(4);
    f.render_widget(
        Paragraph::new(if card.run.is_empty() {
            "Not started"
        } else {
            &card.run
        })
        .style(Style::default().fg(Color::Gray)),
        Rect {
            x,
            y: rect.y + 2,
            width,
            height: 1,
        },
    );
    f.render_widget(
        Paragraph::new(card.status.as_str())
            .style(Style::default().fg(accent).add_modifier(Modifier::BOLD))
            .wrap(Wrap { trim: true }),
        Rect {
            x,
            y: rect.y + 4,
            width,
            height: 2,
        },
    );
    let history = card
        .history
        .iter()
        .rev()
        .skip(1)
        .take(rect.height.saturating_sub(9) as usize)
        .rev()
        .map(|s| format!("· {s}"))
        .collect::<Vec<_>>()
        .join("\n");
    f.render_widget(
        Paragraph::new(history).style(Style::default().fg(Color::DarkGray)),
        Rect {
            x,
            y: rect.y + 7,
            width,
            height: rect.height.saturating_sub(8),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    #[test]
    fn dashboard_renders_both_servers_in_columns() {
        let mut terminal = Terminal::new(TestBackend::new(90, 20)).unwrap();
        let mut state = Dashboard {
            started: Instant::now(),
            mode: Mode::Compare,
            phase: "Measuring".into(),
            chunkwake: Card::default(),
            vanilla: Card::default(),
        };
        state.apply(Update::Run {
            server: "chunkwake".into(),
            iteration: 1,
            total: 1,
        });
        state.apply(Update::Run {
            server: "vanilla".into(),
            iteration: 1,
            total: 1,
        });
        terminal.draw(|frame| draw(frame, &state)).unwrap();
        let buffer = terminal.backend().buffer();
        let line = |y| (0..90).map(|x| buffer[(x, y)].symbol()).collect::<String>();
        let titles = line(3);
        assert!(titles.contains("Chunkwake"));
        assert!(titles.contains("Vanilla"));
    }
}
