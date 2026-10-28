//! Live terminal feedback shared by headless runs and the interactive dashboard.

use crossterm::execute;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode};
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
    Run { server: String, iteration: usize, total: usize },
    Message { server: Option<String>, text: String },
}

pub struct Progress {
    tx: Option<Sender<Update>>,
    worker: Option<JoinHandle<()>>,
    started: Instant,
}

impl Progress {
    pub fn new(interactive: bool) -> Result<Self, String> {
        let started = Instant::now();
        if interactive && io::stdout().is_terminal() {
            let (tx, rx) = mpsc::channel();
            let worker = thread::spawn(move || dashboard(rx));
            Ok(Self { tx: Some(tx), worker: Some(worker), started })
        } else {
            Ok(Self { tx: None, worker: None, started })
        }
    }

    pub fn run(&self, server: &str, iteration: usize, total: usize) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(Update::Run { server: server.into(), iteration, total });
        } else {
            eprintln!("[{:>5}s] {server} · run {iteration}/{total}", self.started.elapsed().as_secs());
        }
    }

    pub fn update(&self, server: Option<&str>, text: &str) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(Update::Message { server: server.map(str::to_owned), text: text.into() });
        } else {
            eprintln!("[{:>5}s] {}{text}", self.started.elapsed().as_secs(), server.map(|s| format!("{s} · ")).unwrap_or_default());
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
        self.history.push(text);
        if self.history.len() > 6 {
            self.history.remove(0);
        }
    }
}

struct Dashboard {
    started: Instant,
    phase: String,
    chunkwake: Card,
    vanilla: Card,
}

impl Dashboard {
    fn apply(&mut self, update: Update) {
        match update {
            Update::Run { server, iteration, total } => {
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
        if server == "vanilla" { &mut self.vanilla } else { &mut self.chunkwake }
    }
}

struct Restore;
impl Drop for Restore {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

fn dashboard(rx: Receiver<Update>) {
    if enable_raw_mode().is_err() || execute!(io::stdout(), EnterAlternateScreen).is_err() {
        return;
    }
    let _restore = Restore;
    let Ok(mut terminal) = Terminal::new(CrosstermBackend::new(io::stdout())) else {
        return;
    };
    let mut state = Dashboard {
        started: Instant::now(),
        phase: "Preparing test".into(),
        chunkwake: Card { status: "Waiting".into(), ..Card::default() },
        vanilla: Card { status: "Waiting".into(), ..Card::default() },
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
        f.render_widget(Paragraph::new("Chunkwake Lab is running. Enlarge the terminal for details."), area);
        return;
    }
    let title = Rect { x: area.x + 1, y: area.y, width: area.width - 2, height: 3 };
    f.render_widget(
        Paragraph::new(format!("  ◈ Chunkwake Lab   ·   {}\n  {}   ·   {} s elapsed", state.phase, if state.vanilla.run.is_empty() { "Smoke / prepare" } else { "Chunkwake vs vanilla" }, state.started.elapsed().as_secs()))
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        title,
    );
    let gap = 2;
    let width = (area.width - 4 - gap) / 2;
    let height = area.height - 5;
    let left = Rect { x: area.x + 1, y: area.y + 3, width, height };
    let right = Rect { x: left.x + width + gap, y: left.y, width: area.width - 3 - width - gap, height };
    card(f, left, "⚙ Chunkwake", &state.chunkwake, Color::Cyan);
    card(f, right, "◈ Vanilla", &state.vanilla, Color::Yellow);
    let footer = Rect { x: area.x + 2, y: area.y + area.height - 2, width: area.width - 4, height: 1 };
    f.render_widget(Paragraph::new("Progress updates every second · report is saved when the test finishes").style(Style::default().fg(Color::Gray)), footer);
}

fn card(f: &mut ratatui::Frame, rect: Rect, name: &str, card: &Card, accent: Color) {
    f.render_widget(Block::default().title(format!(" {name} ")).borders(Borders::ALL).border_style(Style::default().fg(accent)), rect);
    let inner = Rect { x: rect.x + 2, y: rect.y + 2, width: rect.width.saturating_sub(4), height: rect.height.saturating_sub(3) };
    let history = card.history.iter().rev().skip(1).take(inner.height.saturating_sub(4) as usize).rev().map(|s| format!("  {s}")).collect::<Vec<_>>().join("\n");
    f.render_widget(Paragraph::new(format!("{}\n\n{}\n\n{}", if card.run.is_empty() { "Not started" } else { &card.run }, card.status, history)).wrap(Wrap { trim: true }), inner);
}
