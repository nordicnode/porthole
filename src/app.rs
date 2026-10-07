//! Application state: screens, the setup wizard state machine, the Doctor,
//! input handling.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver};

use crossterm::event::KeyCode;

use crate::config::{self, Config};
use crate::docker::{self, ServiceStatus};
use crate::provision::{self, Preferences, ProvEvent, StepStatus, STEPS};
use crate::services::SERVICES;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Dashboard,
    Wizard,
    Integrations,
    Doctor,
    Logs,
    Help,
}

impl Screen {
    pub const ALL: [Screen; 6] = [
        Screen::Dashboard,
        Screen::Wizard,
        Screen::Integrations,
        Screen::Doctor,
        Screen::Logs,
        Screen::Help,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Screen::Dashboard => "Fleet",
            Screen::Wizard => "Setup",
            Screen::Integrations => "Wiring",
            Screen::Doctor => "Doctor",
            Screen::Logs => "Logs",
            Screen::Help => "Help",
        }
    }
}

// ─────────────────────────── Setup wizard ───────────────────────────

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

    fn drain_events(&mut self) -> Option<bool> {
        let events: Vec<ProvEvent> = match &self.rx {
            Some(rx) => rx.try_iter().collect(),
            None => Vec::new(),
        };
        let mut finished = None;
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
                    finished = Some(ok);
                }
            }
        }
        finished
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
                // Deliberately no keys: the installer is a separate process
                // and keeps going; we don't offer casual interruption.
            }
            WizardPhase::Done(_) => {
                if code == KeyCode::Enter {
                    *self = WizardState::new();
                }
            }
        }
    }

    fn tick(&mut self) -> Option<bool> {
        self.tick += 1;
        self.drain_events()
    }
}

// ─────────────────────────── Doctor ───────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
}

/// A one-key fix the Doctor can apply.
#[derive(Clone)]
pub enum Fix {
    StartContainer(String),
}

/// One health check, explained in plain language.
#[derive(Clone)]
pub struct Check {
    pub name: String,
    pub message: String,
    pub status: CheckStatus,
    pub fix: Option<Fix>,
    pub fix_label: String,
}

pub struct DoctorState {
    pub checks: Vec<Check>,
    pub selected: usize,
    pub ran: bool,
}

impl DoctorState {
    fn new() -> Self {
        Self {
            checks: Vec::new(),
            selected: 0,
            ran: false,
        }
    }

    /// Run every check fresh. Plain language throughout: each check says
    /// what's wrong *and* what it means for the user.
    fn run(&mut self, config: &Config) {
        let mut checks = Vec::new();

        // Docker itself.
        if docker::docker_available() {
            checks.push(Check {
                name: "Docker".to_string(),
                message: "Docker is installed and answering.".to_string(),
                status: CheckStatus::Pass,
                fix: None,
                fix_label: String::new(),
            });
        } else {
            checks.push(Check {
                name: "Docker".to_string(),
                message: "Docker isn't installed or isn't running. Nothing else can work until it is — install it from docker.com, then come back.".to_string(),
                status: CheckStatus::Fail,
                fix: None,
                fix_label: String::new(),
            });
            self.checks = checks;
            self.selected = 0;
            self.ran = true;
            return;
        }

        if docker::compose_available() {
            checks.push(Check {
                name: "Compose".to_string(),
                message: "The docker compose helper is here.".to_string(),
                status: CheckStatus::Pass,
                fix: None,
                fix_label: String::new(),
            });
        } else {
            checks.push(Check {
                name: "Compose".to_string(),
                message: "The 'docker compose' helper is missing. The Setup wizard can install it for you.".to_string(),
                status: CheckStatus::Warn,
                fix: None,
                fix_label: String::new(),
            });
        }

        match &config.install_dir {
            Some(dir) => checks.push(Check {
                name: "Install location".to_string(),
                message: format!("Porthole remembers your fleet lives at {dir}."),
                status: CheckStatus::Pass,
                fix: None,
                fix_label: String::new(),
            }),
            None => checks.push(Check {
                name: "Install location".to_string(),
                message: "Porthole doesn't know where your fleet was installed yet. Run the Setup wizard once and it'll remember.".to_string(),
                status: CheckStatus::Warn,
                fix: None,
                fix_label: String::new(),
            }),
        }

        // Every service: exists? running? actually answering?
        let statuses = docker::service_statuses();
        for svc in SERVICES {
            if svc.id == "torbox" {
                checks.push(Check {
                    name: svc.name.to_string(),
                    message: "TorBox lives in the cloud — nothing to check on this machine."
                        .to_string(),
                    status: CheckStatus::Pass,
                    fix: None,
                    fix_label: String::new(),
                });
                continue;
            }
            let status = statuses
                .get(svc.id)
                .copied()
                .unwrap_or(ServiceStatus::NotInstalled);
            match status {
                ServiceStatus::Running => {
                    if docker::port_open(svc.port, 400) {
                        checks.push(Check {
                            name: svc.name.to_string(),
                            message: format!("Up and answering on port {}.", svc.port),
                            status: CheckStatus::Pass,
                            fix: None,
                            fix_label: String::new(),
                        });
                    } else {
                        checks.push(Check {
                            name: svc.name.to_string(),
                            message: "Its container is running but its page isn't answering yet — it's probably still starting up. Give it a minute.".to_string(),
                            status: CheckStatus::Warn,
                            fix: None,
                            fix_label: String::new(),
                        });
                    }
                }
                ServiceStatus::Stopped => checks.push(Check {
                    name: svc.name.to_string(),
                    message: "Installed but not running. Your fleet is missing a crew member."
                        .to_string(),
                    status: CheckStatus::Warn,
                    fix: Some(Fix::StartContainer(svc.id.to_string())),
                    fix_label: format!("Start {}", svc.name),
                }),
                ServiceStatus::Failed => checks.push(Check {
                    name: svc.name.to_string(),
                    message: "It keeps stopping on its own. This usually means something in its settings needs attention — starting it again probably won't help.".to_string(),
                    status: CheckStatus::Fail,
                    fix: None,
                    fix_label: String::new(),
                }),
                ServiceStatus::NotInstalled => checks.push(Check {
                    name: svc.name.to_string(),
                    message: "Not installed yet. The Setup wizard will bring it aboard."
                        .to_string(),
                    status: CheckStatus::Warn,
                    fix: None,
                    fix_label: String::new(),
                }),
                ServiceStatus::Unknown => checks.push(Check {
                    name: svc.name.to_string(),
                    message: "Can't tell — Docker isn't answering.".to_string(),
                    status: CheckStatus::Warn,
                    fix: None,
                    fix_label: String::new(),
                }),
            }
        }

        self.checks = checks;
        self.selected = 0;
        self.ran = true;
    }

    /// Apply the selected check's fix. Returns a message for the flash line.
    fn apply_fix(&mut self) -> Option<String> {
        let check = self.checks.get(self.selected)?;
        match check.fix.clone()? {
            Fix::StartContainer(name) => {
                if docker::start_container(&name) {
                    Some(format!("{} started.", check.name))
                } else {
                    Some(format!(
                        "Couldn't start {} — see the Logs view.",
                        check.name
                    ))
                }
            }
        }
    }

    fn on_key(&mut self, code: KeyCode, config: &Config) -> Option<String> {
        match code {
            KeyCode::Up => {
                self.selected = self.selected.saturating_sub(1);
                None
            }
            KeyCode::Down => {
                let max = self.checks.len().saturating_sub(1);
                self.selected = (self.selected + 1).min(max);
                None
            }
            KeyCode::Char('d') => {
                self.run(config);
                None
            }
            KeyCode::Char('f') | KeyCode::Enter => {
                let msg = self.apply_fix();
                self.run(config); // re-check after the fix
                msg
            }
            _ => None,
        }
    }
}

// ─────────────────────────── App ───────────────────────────

pub struct App {
    pub screen: Screen,
    pub should_quit: bool,
    pub dashboard_selected: usize,
    pub statuses: HashMap<String, ServiceStatus>,
    pub docker_missing: bool,
    pub config: Config,
    pub wizard: WizardState,
    pub doctor: DoctorState,
    /// Transient one-line feedback, cleared on the next keypress.
    pub flash: Option<String>,
}

impl App {
    pub fn new() -> Self {
        let mut app = Self {
            screen: Screen::Dashboard,
            should_quit: false,
            dashboard_selected: 0,
            statuses: HashMap::new(),
            docker_missing: !docker::docker_available(),
            config: config::load(),
            wizard: WizardState::new(),
            doctor: DoctorState::new(),
            flash: None,
        };
        app.refresh_statuses();
        app
    }

    pub fn refresh_statuses(&mut self) {
        self.docker_missing = !docker::docker_available();
        self.statuses = docker::service_statuses();
    }

    fn goto(&mut self, screen: Screen) {
        self.screen = screen;
        if screen == Screen::Doctor {
            self.doctor.run(&self.config);
        }
        if screen == Screen::Dashboard {
            self.refresh_statuses();
        }
    }

    /// Fleet action on the selected dashboard service. Returns feedback.
    fn fleet_action(&mut self, action: &str) -> String {
        let svc = match SERVICES.get(self.dashboard_selected) {
            Some(s) => s,
            None => return "Nothing selected.".to_string(),
        };
        if svc.id == "torbox" {
            return "TorBox lives in the cloud — nothing to start or stop here.".to_string();
        }
        let ok = match action {
            "start" => docker::start_container(svc.id),
            "stop" => docker::stop_container(svc.id),
            "restart" => docker::restart_container(svc.id),
            _ => false,
        };
        self.refresh_statuses();
        if ok {
            format!("{}: {action} requested.", svc.name)
        } else {
            format!("{}: couldn't {action} it — is Docker running?", svc.name)
        }
    }

    pub fn on_tick(&mut self) {
        if let Some(finished_ok) = self.wizard.tick() {
            // The wizard just finished: remember where the fleet lives.
            if finished_ok {
                self.config.install_dir = Some(self.wizard.prefs.install_dir.clone());
                if let Err(e) = config::save(&self.config) {
                    self.flash = Some(format!("Setup done, but couldn't save settings: {e}"));
                } else {
                    self.refresh_statuses();
                }
            }
        }
    }

    pub fn on_key(&mut self, code: KeyCode) {
        self.flash = None;

        // Global keys.
        match code {
            KeyCode::Char('q') => {
                self.should_quit = true;
                return;
            }
            KeyCode::Char('1') => self.goto(Screen::Dashboard),
            KeyCode::Char('2') => self.goto(Screen::Wizard),
            KeyCode::Char('3') => self.goto(Screen::Integrations),
            KeyCode::Char('4') => self.goto(Screen::Doctor),
            KeyCode::Char('5') => self.goto(Screen::Logs),
            KeyCode::Char('?') => self.goto(Screen::Help),
            KeyCode::Tab => {
                let i = Screen::ALL
                    .iter()
                    .position(|s| *s == self.screen)
                    .unwrap_or(0);
                let next = Screen::ALL[(i + 1) % Screen::ALL.len()];
                self.goto(next);
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
                KeyCode::Char('s') => {
                    self.flash = Some(self.fleet_action("start"));
                }
                KeyCode::Char('x') => {
                    self.flash = Some(self.fleet_action("stop"));
                }
                KeyCode::Char('R') => {
                    self.flash = Some(self.fleet_action("restart"));
                }
                _ => {}
            },
            Screen::Wizard => self.wizard.on_key(code),
            Screen::Doctor => {
                let config = self.config.clone();
                if let Some(msg) = self.doctor.on_key(code, &config) {
                    self.flash = Some(msg);
                    self.refresh_statuses();
                }
            }
            _ => {}
        }
    }
}
