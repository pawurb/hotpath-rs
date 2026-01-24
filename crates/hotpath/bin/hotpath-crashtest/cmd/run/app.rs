use crate::cmd::run::events::AppEvent;
use crate::cmd::run::input::spawn_input_reader;
use crate::scenarios::cpu_spike::CpuSpike;
use crate::scenarios::memory_bloat::MemoryBloat;
use crossbeam_channel::{select, Receiver};
use crossterm::event::KeyCode;
use ratatui::{
    layout::{Constraint, Layout},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    Frame,
};
use std::time::Duration;

pub struct App {
    exit: bool,
    cpu_spike: CpuSpike,
    memory_bloat: MemoryBloat,
    thread_panic: bool,
    runtime_block: bool,
    event_rx: Receiver<AppEvent>,
}

impl App {
    pub fn new() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        spawn_input_reader(tx);

        Self {
            exit: false,
            cpu_spike: CpuSpike::new(),
            memory_bloat: MemoryBloat::new(),
            thread_panic: false,
            runtime_block: false,
            event_rx: rx,
        }
    }

    pub fn run(
        &mut self,
        terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    ) -> std::io::Result<()> {
        while !self.exit {
            terminal.draw(|frame| self.render(frame))?;
            self.handle_events();
        }
        Ok(())
    }

    fn render(&self, frame: &mut Frame) {
        let area = frame.area();

        let lines = vec![
            Line::from(self.indicator("[1] CPU Spike", self.cpu_spike.is_activated())),
            Line::from(self.indicator("[2] Memory Bloat", self.memory_bloat.is_activated())),
            Line::from(self.indicator("[3] Thread Panic", self.thread_panic)),
            Line::from(self.indicator("[4] Runtime Block", self.runtime_block)),
        ];

        let vertical = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(4),
            Constraint::Fill(1),
        ]);
        let [_, center, _] = vertical.areas(area);

        let horizontal = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Length(20),
            Constraint::Fill(1),
        ]);
        let [_, center, _] = horizontal.areas(center);

        for (i, line) in lines.into_iter().enumerate() {
            let line_area = ratatui::layout::Rect {
                x: center.x,
                y: center.y + i as u16,
                width: center.width,
                height: 1,
            };
            frame.render_widget(line, line_area);
        }
    }

    fn indicator<'a>(&self, label: &'a str, active: bool) -> Span<'a> {
        if active {
            Span::styled(label, Style::default().fg(Color::Green).bold())
        } else {
            Span::styled(label, Style::default().fg(Color::Gray))
        }
    }

    fn handle_events(&mut self) {
        select! {
            recv(self.event_rx) -> msg => {
                if let Ok(AppEvent::Key(code)) = msg {
                    self.handle_key(code);
                }
            }
            default(Duration::from_millis(100)) => {}
        }
    }

    fn handle_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('1') => {
                self.cpu_spike.set_activated(!self.cpu_spike.is_activated());
            }
            KeyCode::Char('2') => {
                self.memory_bloat
                    .set_activated(!self.memory_bloat.is_activated());
            }
            KeyCode::Char('3') => self.thread_panic = !self.thread_panic,
            KeyCode::Char('4') => self.runtime_block = !self.runtime_block,
            KeyCode::Char('q') | KeyCode::Char('Q') => self.exit = true,
            _ => {}
        }
    }
}
