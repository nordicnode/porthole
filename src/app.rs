//! Application state: screens, wizard progress, input handling.

use crossterm::event::KeyCode;

use crate::provision::{StepStatus, STEPS};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Dashboard,
    Wizard,
    Integrations,
    Logs,
    Help,
}

impl Screen {
    pub const ALL: [Screen; 5] = [
        Screen::Dashboard,
        Screen::Wizard,
        Screen::Integrations,
        Screen::Logs,
        Screen::Help,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Screen::Dashboard => "Fleet",
            Screen::Wizard => "Setup",
            Screen::Integrations => "Wiring",
            Screen::Logs => "Logs",
            Screen::Help => "Help",
        }
    }
}

pub struct WizardState {
    pub step_idx: usize,
    pub status: Vec<StepStatus>,
    pub logs: Vec<String>,
    pub running: bool,
    pub progress: u16,
    log_cursor: usize,
}

impl WizardState {
    fn new() -> Self {
        Self {
            step_idx: 0,
            status: vec![StepStatus::Pending; STEPS.len()],
            logs: vec!["Press Enter to start the guided setup (demo mode).".to_string()],
            running: false,
            progress: 0,
            log_cursor: 0,
        }
    }

    fn start(&mut self) {
        if self.running {
            return;
        }
        *self = WizardState::new();
        self.running = true;
        self.status[0] = StepStatus::Active;
        self.logs.push("── Starting guided setup ──".to_string());
    }

    /// Advance the demo runner one tick. Returns nothing; driven by the UI tick.
    fn tick(&mut self) {
        if !self.running {
            return;
        }
        // Emit the next demo log line for the active step, then progress.
        let step = &STEPS[self.step_idx];
        if self.log_cursor < step.demo_logs.len() {
            self.logs.push(format!(
                "  {}  {}",
                step.title, step.demo_logs[self.log_cursor]
            ));
            self.log_cursor += 1;
            self.progress = ((self.log_cursor as f32 / step.demo_logs.len() as f32) * 100.0) as u16;
        } else {
            self.status[self.step_idx] = StepStatus::Done;
            self.progress = 100;
            if self.step_idx + 1 < STEPS.len() {
                self.step_idx += 1;
                self.status[self.step_idx] = StepStatus::Active;
                self.progress = 0;
                self.log_cursor = 0;
                self.logs.push(format!(
                    "── Step {}/{}: {} ──",
                    self.step_idx + 1,
                    STEPS.len(),
                    STEPS[self.step_idx].title
                ));
            } else {
                self.running = false;
                self.logs
                    .push("── Setup complete. Your fleet is wired together. ──".to_string());
            }
        }
    }
}

pub struct App {
    pub screen: Screen,
    pub should_quit: bool,
    pub dashboard_selected: usize,
    pub wizard: WizardState,
}

impl App {
    pub fn new() -> Self {
        Self {
            screen: Screen::Dashboard,
            should_quit: false,
            dashboard_selected: 0,
            wizard: WizardState::new(),
        }
    }

    pub fn on_tick(&mut self) {
        self.wizard.tick();
    }

    pub fn on_key(&mut self, code: KeyCode) {
        // Global keys.
        match code {
            KeyCode::Char('q') => {
                self.should_quit = true;
                return;
            }
            KeyCode::Char('1') => self.screen = Screen::Dashboard,
            KeyCode::Char('2') => self.screen = Screen::Wizard,
            KeyCode::Char('3') => self.screen = Screen::Integrations,
            KeyCode::Char('4') => self.screen = Screen::Logs,
            KeyCode::Char('?') => self.screen = Screen::Help,
            KeyCode::Tab => {
                let i = Screen::ALL
                    .iter()
                    .position(|s| *s == self.screen)
                    .unwrap_or(0);
                self.screen = Screen::ALL[(i + 1) % Screen::ALL.len()];
            }
            _ => {}
        }

        // Screen-local keys.
        match self.screen {
            Screen::Dashboard => match code {
                KeyCode::Up => {
                    self.dashboard_selected = self.dashboard_selected.saturating_sub(1);
                }
                KeyCode::Down => {
                    let max = crate::services::SERVICES.len().saturating_sub(1);
                    self.dashboard_selected = (self.dashboard_selected + 1).min(max);
                }
                _ => {}
            },
            Screen::Wizard => {
                if code == KeyCode::Enter {
                    self.wizard.start();
                }
            }
            _ => {}
        }
    }
}
