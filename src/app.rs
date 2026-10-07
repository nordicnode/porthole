//! Application state: screens, the setup wizard state machine, input handling.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver};

use crossterm::event::KeyCode;

use crate::docker::{self, ServiceStatus};
use crate::provision::{self, Preferences, ProvEvent, StepStatus, STEPS};
use crate::services::SERVICES;

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

/// Where the wizard is in its flow.
#[derive(PartialEq, Eq)]
pub enum WizardPhase {
    /// Collecting preferences in the form.
    Prefs,
    /// Showing the dry-run plan before executing.
    Plan,
    /// The worker thread is running the real provisioning.
    Running,
    /// Finished; bool = overall success.
    Done(bool),
}

/// Form rows: 0 API key, 1 install dir, 2 media server, 3 PUID, 4 PGID,
/// 5 timezone, 6 the "review plan" action row.
pub const FORM_ROWS: usize = 7;

pub struct WizardState {
    pub phase: WizardPhase,
    pub prefs: Preferences,
    pub form_selected: usize,
    pub form_errors: Vec<String>,
    pub step_status: Vec<StepStatus>,
    pub logs: Vec<String>,
    pub tick: u64,
    rx: Option<Receiver<ProvEvent>>,
}

impl WizardState {
    fn new() -> Self {
        Self {
            phase: WizardPhase::Prefs,
            prefs: Preferences::default(),
            form_selected: 0,
            form_errors: Vec::new(),
            step_status: vec![StepStatus::Pending; STEPS.len()],
            logs: vec!["Answer three questions and Porthole does the rest.".to_string()],
            tick: 0,
            rx: None,
        }
    }

    /// Mutable access to the text field backing a form row, if it has one.
    fn field_mut(&mut self, row: usize) -> Option<&mut String> {
        match row {
            0 => Some(&mut self.prefs.torbox_api_key),
            1 => Some(&mut self.prefs.install_dir),
            3 => Some(&mut self.prefs.puid),
            4 => Some(&mut self.prefs.pgid),
            5 => Some(&mut self.prefs.tz),
            _ => None,
        }
    }

    fn submit_prefs(&mut self) {
        let errs = self.prefs.validate();
        if errs.is_empty() {
            self.form_errors.clear();
            self.phase = WizardPhase::Plan;
        } else {
            self.form_errors = errs;
        }
    }

    fn start_run(&mut self) {
        self.step_status = vec![StepStatus::Pending; STEPS.len()];
        self.logs.push("── Starting guided setup ──".to_string());
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        let prefs = self.prefs.clone();
        std::thread::spawn(move || provision::run_provision(prefs, tx));
        self.phase = WizardPhase::Running;
    }

    /// Redact the API key from a log line before it reaches the screen.
    fn redact(&self, line: &str) -> String {
        let key = self.prefs.torbox_api_key.trim();
        if key.is_empty() {
            line.to_string()
        } else {
            line.replace(key, "[redacted]")
        }
    }

    fn push_log(&mut self, line: String) {
        let line = self.redact(&line);
        self.logs.push(line);
        if self.logs.len() > 500 {
            let drain = self.logs.len() - 500;
            self.logs.drain(..drain);
        }
    }

    fn drain_events(&mut self) {
        let events: Vec<ProvEvent> = match &self.rx {
            Some(rx) => rx.try_iter().collect(),
            None => Vec::new(),
        };
        for ev in events {
            match ev {
                ProvEvent::Log(line) => self.push_log(line),
                ProvEvent::StepBegin(i) => {
                    if let Some(s) = self.step_status.get_mut(i) {
                        *s = StepStatus::Active;
                    }
                    self.push_log(format!(
                        "── Step {}/{}: {} ──",
                        i + 1,
                        STEPS.len(),
                        STEPS[i].title
                    ));
                }
                ProvEvent::StepDone(i, ok) => {
                    if let Some(s) = self.step_status.get_mut(i) {
                        *s = if ok {
                            StepStatus::Done
                        } else {
                            StepStatus::Failed
                        };
                    }
                }
                ProvEvent::Finished(ok) => {
                    self.rx = None;
                    self.phase = WizardPhase::Done(ok);
                    self.push_log(if ok {
                        "── Setup complete. Your fleet is wired together. ──".to_string()
                    } else {
                        "── Setup stopped early — see the log above, fix it, run again. ──"
                            .to_string()
                    });
                }
            }
        }
    }

    fn on_key(&mut self, code: KeyCode) {
        match self.phase {
            WizardPhase::Prefs => match code {
                KeyCode::Up => {
                    self.form_selected = self.form_selected.saturating_sub(1);
                }
                KeyCode::Down => {
                    self.form_selected = (self.form_selected + 1).min(FORM_ROWS - 1);
                }
                KeyCode::Backspace => {
                    if let Some(f) = self.field_mut(self.form_selected) {
                        f.pop();
                    }
                }
                KeyCode::Char(c) => {
                    if self.form_selected == 2 {
                        if c == ' ' {
                            self.prefs.media_server = self.prefs.media_server.toggle();
                        }
                    } else if let Some(f) = self.field_mut(self.form_selected) {
                        f.push(c);
                    }
                }
                KeyCode::Left | KeyCode::Right => {
                    if self.form_selected == 2 {
                        self.prefs.media_server = self.prefs.media_server.toggle();
                    }
                }
                KeyCode::Enter => {
                    if self.form_selected == FORM_ROWS - 1 {
                        self.submit_prefs();
                    } else {
                        self.form_selected = (self.form_selected + 1).min(FORM_ROWS - 1);
                    }
                }
                _ => {}
            },
            WizardPhase::Plan => match code {
                KeyCode::Enter => self.start_run(),
                KeyCode::Esc => self.phase = WizardPhase::Prefs,
                _ => {}
            },
            WizardPhase::Running => {
                // Deliberately no keys: killing the UI mid-install is safe
                // (the installer is a separate process and keeps going),
                // but we don't offer it as a casual action.
            }
            WizardPhase::Done(_) => {
                if code == KeyCode::Enter {
                    *self = WizardState::new();
                }
            }
        }
    }

    fn tick(&mut self) {
        self.tick += 1;
        self.drain_events();
    }
}

pub struct App {
    pub screen: Screen,
    pub should_quit: bool,
    pub dashboard_selected: usize,
    pub statuses: HashMap<String, ServiceStatus>,
    pub docker_missing: bool,
    pub wizard: WizardState,
}

impl App {
    pub fn new() -> Self {
        let mut app = Self {
            screen: Screen::Dashboard,
            should_quit: false,
            dashboard_selected: 0,
            statuses: HashMap::new(),
            docker_missing: !docker::docker_available(),
            wizard: WizardState::new(),
        };
        app.refresh_statuses();
        app
    }

    pub fn refresh_statuses(&mut self) {
        self.docker_missing = !docker::docker_available();
        self.statuses = docker::service_statuses();
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
                    let max = SERVICES.len().saturating_sub(1);
                    self.dashboard_selected = (self.dashboard_selected + 1).min(max);
                }
                KeyCode::Char('r') => self.refresh_statuses(),
                _ => {}
            },
            Screen::Wizard => self.wizard.on_key(code),
            _ => {}
        }
    }
}
