//! Application state: screens, the setup wizard state machine, the Doctor,
//! input handling.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};

use crossterm::event::KeyCode;

use crate::config::{self, Config};
use crate::docker::{self, ServiceStatus};
use crate::provision::{self, Preferences, ProvEvent, StepStatus, STEPS};
use crate::services::SERVICES;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    Dashboard,
    Wizard,
    Integrations,
    Doctor,
    Care,
    Logs,
    Help,
}

impl Screen {
    pub const ALL: [Screen; 7] = [
        Screen::Dashboard,
        Screen::Wizard,
        Screen::Integrations,
        Screen::Doctor,
        Screen::Care,
        Screen::Logs,
        Screen::Help,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Screen::Dashboard => "Fleet",
            Screen::Wizard => "Setup",
            Screen::Integrations => "Wiring",
            Screen::Doctor => "Doctor",
            Screen::Care => "Care",
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

        // Local disk: small-disk mode needs transcode headroom.
        match &config.install_dir {
            Some(dir) => match crate::storage::free_bytes(std::path::Path::new(dir)) {
                Ok(free) => {
                    let verdict = crate::storage::DiskVerdict::from_free_bytes(free);
                    let gb = verdict.free_gb();
                    match verdict {
                        crate::storage::DiskVerdict::Plenty(_) => checks.push(Check {
                            name: "Local disk".to_string(),
                            message: format!(
                                "{gb} GB free — plenty of room for the stream cache and transcodes."
                            ),
                            status: CheckStatus::Pass,
                            fix: None,
                            fix_label: String::new(),
                        }),
                        crate::storage::DiskVerdict::Tight(_) => checks.push(Check {
                            name: "Local disk".to_string(),
                            message: format!(
                                "Only {gb} GB free. Playback still works (files stream from the cloud), but 4K transcodes may run out of room."
                            ),
                            status: CheckStatus::Warn,
                            fix: None,
                            fix_label: String::new(),
                        }),
                        crate::storage::DiskVerdict::Critical(_) => checks.push(Check {
                            name: "Local disk".to_string(),
                            message: format!(
                                "Only {gb} GB free — critically low. Free up space or playback and transcodes will fail."
                            ),
                            status: CheckStatus::Fail,
                            fix: None,
                            fix_label: String::new(),
                        }),
                    }
                }
                Err(e) => checks.push(Check {
                    name: "Local disk".to_string(),
                    message: format!("Couldn't measure free disk space: {e}."),
                    status: CheckStatus::Warn,
                    fix: None,
                    fix_label: String::new(),
                }),
            },
            None => checks.push(Check {
                name: "Local disk".to_string(),
                message: "No install location set, so disk space can't be checked yet.".to_string(),
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

// ─────────────────────────── Care ───────────────────────────

/// A care action. Cloneable so confirmations can carry the exact op.
#[derive(Clone)]
pub enum CareOp {
    Backup,
    Restore(PathBuf),
    RegenConfigs,
    SmallDisk,
    CheckUpdates,
    CheckPortholeUpdate,
    InstallPortholeUpdate(crate::selfupdate::ReleaseInfo),
    UpdateFleet,
    Uninstall,
}

impl CareOp {
    pub fn title(&self) -> &'static str {
        match self {
            CareOp::Backup => "Back up now",
            CareOp::Restore(_) => "Restore a backup",
            CareOp::RegenConfigs => "Regenerate configs",
            CareOp::SmallDisk => "Optimize for small disk",
            CareOp::CheckUpdates => "Check for updates",
            CareOp::CheckPortholeUpdate => "Check for Porthole updates",
            CareOp::InstallPortholeUpdate(_) => "Install Porthole update",
            CareOp::UpdateFleet => "Update fleet",
            CareOp::Uninstall => "Uninstall fleet",
        }
    }

    pub fn plain(&self) -> &'static str {
        match self {
            CareOp::Backup => "Save a snapshot of your configs. Do this before anything scary.",
            CareOp::Restore(_) => "Bring back a snapshot. Your fleet returns to exactly how it was.",
            CareOp::RegenConfigs => {
                "Rewrite all config files with Porthole's native generator. Fixes corrupted configs; secrets are preserved."
            }
            CareOp::SmallDisk => {
                "Mount the debrid cloud as a filesystem and make imports instant symlinks. Your library lives remotely; this disk only holds a small stream cache."
            }
            CareOp::CheckUpdates => {
                "See if any service has a new version. Downloads, but changes nothing."
            }
            CareOp::CheckPortholeUpdate => {
                "See if a new Porthole itself is out. Nothing changes until you say so."
            }
            CareOp::InstallPortholeUpdate(_) => "Replace this Porthole with the new release.",
            CareOp::UpdateFleet => {
                "Back up, update everything, check health, roll back automatically if it breaks."
            }
            CareOp::Uninstall => "Remove everything Porthole installed. The point of no return.",
        }
    }

    /// What the confirmation screen tells the user, in plain language.
    pub fn confirm_lines(&self, install_dir: &Option<String>) -> Vec<String> {
        let dir = install_dir
            .clone()
            .unwrap_or_else(|| "(not set)".to_string());
        match self {
            CareOp::Backup => vec![
                "Porthole will save:".to_string(),
                format!("  • everything in {dir} except your media data"),
                "to ~/.local/share/porthole/backups/ as a timestamped archive.".to_string(),
                "Your media data isn't included — it's re-fetchable from the cloud.".to_string(),
            ],
            CareOp::Restore(p) => vec![
                "Porthole will:".to_string(),
                "  • stop your fleet".to_string(),
                format!(
                    "  • replace your configs with the backup '{}'",
                    p.file_name().unwrap_or_default().to_string_lossy()
                ),
                "Your media data is untouched. Start the fleet again from the Fleet view."
                    .to_string(),
            ],
            CareOp::RegenConfigs => vec![
                "Porthole will:".to_string(),
                "  • take a backup first".to_string(),
                "  • rewrite .env, the Decypharr config and the three *arr configs".to_string(),
                "  • keep your existing API keys and passwords".to_string(),
            ],
            CareOp::SmallDisk => vec![
                "Porthole will:".to_string(),
                "  • take a backup first".to_string(),
                "  • turn on Decypharr's DFS mount (the debrid cloud appears as a folder)"
                    .to_string(),
                "  • size the stream cache from your actual free disk space".to_string(),
                "  • make new downloads import as symlinks — zero local bytes".to_string(),
                "Restart Decypharr afterwards for the mount to take effect.".to_string(),
            ],
            CareOp::CheckUpdates => vec![
                "Porthole will download the latest images and tell you what's new.".to_string(),
                "Nothing restarts. Nothing changes.".to_string(),
            ],
            CareOp::CheckPortholeUpdate => vec![
                "Porthole will ask GitHub if a new release is out.".to_string(),
                "Nothing downloads until you say so.".to_string(),
            ],
            CareOp::InstallPortholeUpdate(rel) => vec![
                format!(
                    "Porthole {} is available (you're running v{}).",
                    rel.tag,
                    crate::selfupdate::CURRENT_VERSION
                ),
                "Porthole will:".to_string(),
                "  • download the new release".to_string(),
                "  • verify its checksum before touching anything".to_string(),
                "  • swap the binary (your settings are kept)".to_string(),
                "You'll restart Porthole yourself afterwards.".to_string(),
            ],
            CareOp::UpdateFleet => vec![
                "Porthole will:".to_string(),
                "  1. back up your configs".to_string(),
                "  2. download updates".to_string(),
                "  3. restart everything".to_string(),
                "  4. check every service is healthy".to_string(),
                "  5. roll back automatically if anything breaks".to_string(),
            ],
            CareOp::Uninstall => {
                let mut lines = vec!["Porthole will remove:".to_string()];
                lines.extend(crate::care::uninstall_plan(std::path::Path::new(&dir)));
                lines.push("This cannot be undone.".to_string());
                lines.push("Your cloud media (TorBox) is untouched.".to_string());
                lines
            }
        }
    }
}

pub(crate) const CARE_ACTIONS: &[fn() -> CareOp] = &[
    || CareOp::Backup,
    || CareOp::Restore(PathBuf::new()), // placeholder → backup picker
    || CareOp::RegenConfigs,
    || CareOp::SmallDisk,
    || CareOp::CheckUpdates,
    || CareOp::CheckPortholeUpdate,
    || CareOp::UpdateFleet,
    || CareOp::Uninstall,
];

#[derive(PartialEq, Eq)]
pub enum CareView {
    Main,
    PickBackup,
    Confirm,
    Working,
    Done,
}

pub struct CareState {
    pub view: CareView,
    pub selected: usize,
    pub backups: Vec<PathBuf>,
    pub pending_op: Option<CareOp>,
    /// For destructive ops: first Enter arms, second Enter fires.
    pub confirm_armed: bool,
    pub logs: Vec<String>,
    pub done_message: String,
    pub done_ok: bool,
    pub tick: u64,
    rx: Option<mpsc::Receiver<crate::care::CareEvent>>,
}

impl CareState {
    fn new() -> Self {
        Self {
            view: CareView::Main,
            selected: 0,
            backups: Vec::new(),
            pending_op: None,
            confirm_armed: false,
            logs: Vec::new(),
            done_message: String::new(),
            done_ok: false,
            tick: 0,
            rx: None,
        }
    }

    fn refresh_backups(&mut self) {
        self.backups = crate::care::list_backups();
    }

    fn start_op(&mut self, op: CareOp, install_dir: Option<String>) {
        self.logs.clear();
        self.logs.push(format!("── {} ──", op.title()));
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        std::thread::spawn(move || {
            run_care_op(op, install_dir, tx);
        });
        self.view = CareView::Working;
    }

    fn push_log(&mut self, line: String) {
        self.logs.push(line);
        if self.logs.len() > 300 {
            let drain = self.logs.len() - 300;
            self.logs.drain(..drain);
        }
    }

    fn drain(&mut self) {
        let events: Vec<crate::care::CareEvent> = match &self.rx {
            Some(rx) => rx.try_iter().collect(),
            None => Vec::new(),
        };
        for ev in events {
            match ev {
                crate::care::CareEvent::Log(line) => self.push_log(line),
                crate::care::CareEvent::UpdateAvailable(rel) => {
                    self.rx = None;
                    self.pending_op = Some(CareOp::InstallPortholeUpdate(rel));
                    self.confirm_armed = false;
                    self.view = CareView::Confirm;
                }
                crate::care::CareEvent::Finished(Ok(msg)) => {
                    self.rx = None;
                    self.done_message = msg;
                    self.done_ok = true;
                    self.view = CareView::Done;
                    self.refresh_backups();
                }
                crate::care::CareEvent::Finished(Err(msg)) => {
                    self.rx = None;
                    self.done_message = msg;
                    self.done_ok = false;
                    self.view = CareView::Done;
                    self.refresh_backups();
                }
            }
        }
    }

    fn on_key(&mut self, code: KeyCode, install_dir: &Option<String>) {
        match self.view {
            CareView::Main => match code {
                KeyCode::Up => self.selected = self.selected.saturating_sub(1),
                KeyCode::Down => self.selected = (self.selected + 1).min(CARE_ACTIONS.len() - 1),
                KeyCode::Enter => {
                    let op = CARE_ACTIONS[self.selected]();
                    self.confirm_armed = false;
                    match op {
                        CareOp::Restore(_) => {
                            self.refresh_backups();
                            self.selected = 0;
                            self.view = CareView::PickBackup;
                        }
                        _ => {
                            self.pending_op = Some(op);
                            self.view = CareView::Confirm;
                        }
                    }
                }
                _ => {}
            },
            CareView::PickBackup => match code {
                KeyCode::Up => self.selected = self.selected.saturating_sub(1),
                KeyCode::Down => {
                    self.selected = (self.selected + 1).min(self.backups.len().saturating_sub(1))
                }
                KeyCode::Enter => {
                    if let Some(p) = self.backups.get(self.selected).cloned() {
                        self.pending_op = Some(CareOp::Restore(p));
                        self.confirm_armed = false;
                        self.view = CareView::Confirm;
                    }
                }
                KeyCode::Esc => {
                    self.selected = 1;
                    self.view = CareView::Main;
                }
                _ => {}
            },
            CareView::Confirm => match code {
                KeyCode::Enter => {
                    if let Some(op) = self.pending_op.clone() {
                        let needs_double = matches!(op, CareOp::Uninstall);
                        if needs_double && !self.confirm_armed {
                            // First yes arms it; the screen now asks once more.
                            self.confirm_armed = true;
                            return;
                        }
                        self.confirm_armed = false;
                        self.start_op(op, install_dir.clone());
                    }
                }
                KeyCode::Esc => {
                    self.pending_op = None;
                    self.confirm_armed = false;
                    self.view = CareView::Main;
                }
                _ => {}
            },
            CareView::Working => {}
            CareView::Done => {
                if code == KeyCode::Enter || code == KeyCode::Esc {
                    self.view = CareView::Main;
                    self.selected = 0;
                }
            }
        }
    }

    fn tick(&mut self) {
        self.tick += 1;
        self.drain();
    }
}

/// Run a care op in a worker thread. install_dir comes from the app config.
fn run_care_op(
    op: CareOp,
    install_dir: Option<String>,
    tx: std::sync::mpsc::Sender<crate::care::CareEvent>,
) {
    // Self-update check is special: when an update is found we hand control
    // to the confirm screen instead of finishing.
    if matches!(op, CareOp::CheckPortholeUpdate) {
        match crate::selfupdate::check_for_update() {
            Ok(Some(rel)) => {
                let _ = tx.send(crate::care::CareEvent::UpdateAvailable(rel));
            }
            Ok(None) => {
                let _ = tx.send(crate::care::CareEvent::Finished(Ok(
                    "You're running the latest Porthole.".to_string(),
                )));
            }
            Err(e) => {
                let _ = tx.send(crate::care::CareEvent::Finished(Err(format!("{e:#}"))));
            }
        }
        return;
    }
    let dir = || -> anyhow::Result<String> {
        install_dir.clone().ok_or_else(|| {
            anyhow::anyhow!(
                "Porthole doesn't know where your fleet lives yet — run the Setup wizard once."
            )
        })
    };
    let result: anyhow::Result<String> = (|| match op {
        CareOp::Backup => {
            let d = dir()?;
            let dest = crate::care::create_backup(std::path::Path::new(&d))?;
            Ok(format!(
                "Backup saved: {}",
                dest.file_name().unwrap_or_default().to_string_lossy()
            ))
        }
        CareOp::Restore(p) => {
            let d = dir()?;
            crate::care::restore_backup(&p, std::path::Path::new(&d))?;
            Ok("Backup restored. Start your fleet again from the Fleet view.".to_string())
        }
        CareOp::RegenConfigs => {
            let d = dir()?;
            crate::care::regenerate_configs(std::path::Path::new(&d), &tx)?;
            Ok("Configs rewritten natively — your secrets were preserved.".to_string())
        }
        CareOp::CheckUpdates => {
            let d = dir()?;
            let updates = crate::care::check_updates(std::path::Path::new(&d), &tx)?;
            Ok(if updates.is_empty() {
                "Everything is already up to date.".to_string()
            } else {
                let names: Vec<_> = updates.iter().map(|u| u.service.clone()).collect();
                format!("Updates available for: {}", names.join(", "))
            })
        }
        CareOp::SmallDisk => {
            let d = dir()?;
            crate::care::apply_small_disk_mode(std::path::Path::new(&d), &tx)?;
            Ok("Small-disk mode enabled — the debrid cloud is now a filesystem.".to_string())
        }
        CareOp::UpdateFleet => {
            let d = dir()?;
            crate::care::update_fleet(std::path::Path::new(&d), &tx)?;
            Ok("Fleet updated — every service is healthy.".to_string())
        }
        CareOp::CheckPortholeUpdate => {
            unreachable!("handled above")
        }
        CareOp::InstallPortholeUpdate(rel) => {
            let msg = crate::selfupdate::install_update(&rel)?;
            Ok(msg)
        }
        CareOp::Uninstall => {
            let d = dir()?;
            crate::care::uninstall(std::path::Path::new(&d), &tx)?;
            Ok("Fleet uninstalled. Thanks for sailing with Porthole.".to_string())
        }
    })();
    let _ = tx.send(crate::care::CareEvent::Finished(
        result.map_err(|e| format!("{e:#}")),
    ));
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
    pub care: CareState,
    /// First-run welcome overlay.
    pub show_welcome: bool,
    /// Transient one-line feedback, cleared on the next keypress.
    pub flash: Option<String>,
    update_rx: Option<Receiver<Result<Option<crate::selfupdate::ReleaseInfo>, String>>>,
}

impl App {
    pub fn new() -> Self {
        let config = config::load();
        let show_welcome = !config.onboarded;
        let mut app = Self {
            screen: Screen::Dashboard,
            should_quit: false,
            dashboard_selected: 0,
            statuses: HashMap::new(),
            docker_missing: !docker::docker_available(),
            config,
            wizard: WizardState::new(),
            doctor: DoctorState::new(),
            care: CareState::new(),
            show_welcome,
            flash: None,
            update_rx: None,
        };
        app.refresh_statuses();
        // Never phone home during tests.
        #[cfg(not(test))]
        app.maybe_check_for_update();
        app
    }

    /// Check for a Porthole update at most once a day, in the background.
    /// Silent on failure — this must never interrupt startup.
    #[cfg_attr(test, allow(dead_code))]
    fn maybe_check_for_update(&mut self) {
        if !crate::selfupdate::should_check(self.config.last_update_check) {
            return;
        }
        self.config.last_update_check = Some(crate::selfupdate::now_secs());
        let _ = config::save(&self.config);
        let (tx, rx) = mpsc::channel();
        self.update_rx = Some(rx);
        std::thread::spawn(move || {
            let result = crate::selfupdate::check_for_update().map_err(|e| format!("{e:#}"));
            let _ = tx.send(result);
        });
    }

    fn dismiss_welcome(&mut self, goto_setup: bool) {
        self.show_welcome = false;
        self.config.onboarded = true;
        if let Err(e) = config::save(&self.config) {
            self.flash = Some(format!("Couldn't save settings: {e}"));
        }
        if goto_setup {
            self.goto(Screen::Wizard);
        }
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
        if screen == Screen::Care {
            self.care.refresh_backups();
            // Reset to a clean slate each visit.
            if self.care.view != CareView::Working {
                self.care.view = CareView::Main;
                self.care.selected = 0;
            }
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
        self.care.tick();
        // Background self-update check.
        if let Some(rx) = &self.update_rx {
            let results: Vec<_> = rx.try_iter().collect();
            for r in results {
                self.update_rx = None;
                if let Ok(Some(rel)) = r {
                    self.flash = Some(format!(
                        "Porthole {} is available — see Care → Check for Porthole updates",
                        rel.tag
                    ));
                }
            }
        }
    }

    pub fn on_key(&mut self, code: KeyCode) {
        self.flash = None;

        // First-run welcome is modal: only Enter/Esc get through.
        if self.show_welcome {
            match code {
                KeyCode::Enter => self.dismiss_welcome(true),
                KeyCode::Esc => self.dismiss_welcome(false),
                _ => {}
            }
            return;
        }

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
            KeyCode::Char('5') => self.goto(Screen::Care),
            KeyCode::Char('6') => self.goto(Screen::Logs),
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
            Screen::Care => {
                let install_dir = self.config.install_dir.clone();
                self.care.on_key(code, &install_dir);
            }
            _ => {}
        }
    }
}
