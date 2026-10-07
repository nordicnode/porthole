//! Rendering: a calm, readable TUI. No jargon on screen — every service and
//! step is described the way you'd explain it to a friend.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Tabs, Wrap},
    Frame,
};

use crate::{
    app::{App, CheckStatus, Screen, WizardPhase},
    docker::ServiceStatus,
    provision::{StepStatus, STEPS},
    services::{INTEGRATIONS, SERVICES},
};

const ACCENT: Color = Color::Cyan;
const GOOD: Color = Color::Green;
const BAD: Color = Color::Red;
const DIM: Color = Color::DarkGray;
const WARM: Color = Color::Yellow;

fn title_block(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ))
}

pub fn render(f: &mut Frame, app: &App) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(f.area());

    // ── Header: banner + tabs ──
    let header = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(2)])
        .split(root[0]);

    let banner = Paragraph::new(Line::from(vec![
        Span::styled("⛵ ", Style::default().fg(ACCENT)),
        Span::styled(
            "PORTHOLE",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "  — your self-hosted media fleet, through one window",
            Style::default().fg(DIM),
        ),
    ]));
    f.render_widget(banner, header[0]);

    let titles: Vec<Line> = Screen::ALL
        .iter()
        .enumerate()
        .map(|(i, s)| Line::from(format!(" {}:{} ", i + 1, s.title())))
        .collect();
    let tabs = Tabs::new(titles)
        .select(
            Screen::ALL
                .iter()
                .position(|s| *s == app.screen)
                .unwrap_or(0),
        )
        .style(Style::default().fg(DIM))
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(ACCENT)
                .add_modifier(Modifier::BOLD),
        );
    f.render_widget(tabs, header[1]);

    // ── Body ──
    match app.screen {
        Screen::Dashboard => render_dashboard(f, app, root[1]),
        Screen::Wizard => render_wizard(f, app, root[1]),
        Screen::Integrations => render_integrations(f, root[1]),
        Screen::Doctor => render_doctor(f, app, root[1]),
        Screen::Logs => render_logs(f, app, root[1]),
        Screen::Help => render_help(f, root[1]),
    }

    // ── Footer: key hints + transient feedback ──
    let mut keys: Vec<(&str, &str)> = vec![
        ("Tab", "switch view"),
        ("↑↓", "move"),
        ("Enter", "confirm"),
        ("q", "quit"),
    ];
    match app.screen {
        Screen::Dashboard => keys.extend([
            ("r", "refresh"),
            ("s", "start"),
            ("x", "stop"),
            ("R", "restart"),
        ]),
        Screen::Doctor => keys.extend([("d", "re-check"), ("f", "apply fix")]),
        _ => {}
    }
    let key_line = Line::from(
        keys.iter()
            .flat_map(|(k, d)| {
                vec![
                    Span::styled(*k, Style::default().fg(ACCENT)),
                    Span::styled(format!(" {d}   "), Style::default().fg(DIM)),
                ]
            })
            .collect::<Vec<_>>(),
    );
    let status_line = match &app.flash {
        Some(msg) => Line::from(vec![Span::styled(
            format!("→ {msg}"),
            Style::default().fg(WARM).add_modifier(Modifier::BOLD),
        )]),
        None => Line::from(vec![Span::styled(
            "Porthole — your fleet, wired together.",
            Style::default().fg(DIM),
        )]),
    };
    let footer = Paragraph::new(Text::from(vec![key_line, status_line])).block(
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(DIM)),
    );
    f.render_widget(footer, root[2]);
}

fn status_span(status: ServiceStatus) -> Span<'static> {
    match status {
        ServiceStatus::Running => Span::styled("● running", Style::default().fg(GOOD)),
        ServiceStatus::Stopped => Span::styled("○ stopped", Style::default().fg(DIM)),
        ServiceStatus::Failed => Span::styled("✖ failed", Style::default().fg(BAD)),
        ServiceStatus::NotInstalled => Span::styled("○ not set up", Style::default().fg(DIM)),
        ServiceStatus::Unknown => Span::styled("? docker not found", Style::default().fg(WARM)),
    }
}

fn render_dashboard(f: &mut Frame, app: &App, area: Rect) {
    let mut items: Vec<ListItem> = Vec::new();
    let mut last_group = "";
    for (i, svc) in SERVICES.iter().enumerate() {
        if svc.group != last_group {
            last_group = svc.group;
            items.push(ListItem::new(Line::from(vec![Span::styled(
                format!("─ {} ─", svc.group),
                Style::default().fg(WARM).add_modifier(Modifier::BOLD),
            )])));
        }
        let port = if svc.port == 0 {
            "cloud".to_string()
        } else {
            format!(":{}", svc.port)
        };
        let selected = i == app.dashboard_selected;
        let marker = if selected { "▸ " } else { "  " };
        let style = if selected {
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        let status = if svc.id == "torbox" {
            Span::styled("☁ cloud service", Style::default().fg(DIM))
        } else {
            status_span(
                app.statuses
                    .get(svc.id)
                    .copied()
                    .unwrap_or(ServiceStatus::NotInstalled),
            )
        };
        items.push(ListItem::new(Text::from(vec![
            Line::from(vec![
                Span::styled(marker, Style::default().fg(ACCENT)),
                Span::styled(format!("{:<10}", svc.name), style),
                Span::styled(format!("{:<8}", port), Style::default().fg(DIM)),
                status,
            ]),
            Line::from(vec![
                Span::raw("    "),
                Span::styled(svc.plain, Style::default().fg(Color::Gray)),
            ]),
        ])));
    }
    if app.docker_missing {
        items.push(ListItem::new(Line::from(vec![Span::styled(
            "Docker isn't installed here — the Setup wizard will offer to install it.",
            Style::default().fg(WARM),
        )])));
    }
    let list = List::new(items).block(title_block(
        "Fleet — every service, one glance (r to refresh)",
    ));
    f.render_widget(list, area);
}

fn render_wizard(f: &mut Frame, app: &App, area: Rect) {
    match app.wizard.phase {
        WizardPhase::Prefs => render_prefs(f, app, area),
        WizardPhase::Plan => render_plan(f, app, area),
        WizardPhase::Running | WizardPhase::Done(_) => render_progress(f, app, area),
    }
}

fn form_row(
    label: &str,
    value: &str,
    hint: &str,
    selected: bool,
    masked: bool,
) -> ListItem<'static> {
    let shown = if masked {
        "•".repeat(value.chars().count())
    } else {
        value.to_string()
    };
    let cursor = if selected { "▌" } else { "" };
    ListItem::new(Text::from(vec![
        Line::from(vec![
            Span::styled(
                if selected { "▸ " } else { "  " },
                Style::default().fg(ACCENT),
            ),
            Span::styled(
                format!("{label:<14}"),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("{shown}{cursor}"), Style::default().fg(ACCENT)),
        ]),
        Line::from(vec![
            Span::raw("    "),
            Span::styled(hint.to_string(), Style::default().fg(DIM)),
        ]),
    ]))
}

fn render_prefs(f: &mut Frame, app: &App, area: Rect) {
    let w = &app.wizard;
    let p = &w.prefs;
    let s = w.form_selected;
    let rows = vec![
        form_row(
            "TorBox key",
            &p.torbox_api_key,
            "Your API key from torbox.app — kept masked, never shown",
            s == 0,
            true,
        ),
        form_row(
            "Install dir",
            &p.install_dir,
            "Where Porthole keeps configs and containers",
            s == 1,
            false,
        ),
        ListItem::new(Text::from(vec![
            Line::from(vec![
                Span::styled(
                    if s == 2 { "▸ " } else { "  " },
                    Style::default().fg(ACCENT),
                ),
                Span::styled(
                    format!("{:<14}", "Watch with"),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("< {} >", p.media_server.label()),
                    Style::default().fg(ACCENT),
                ),
            ]),
            Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    "Space / ← → to switch. Your cinema app.",
                    Style::default().fg(DIM),
                ),
            ]),
        ])),
        form_row(
            "PUID",
            &p.puid,
            "Your user id — so files belong to you, not root (usually 1000)",
            s == 3,
            false,
        ),
        form_row(
            "PGID",
            &p.pgid,
            "Your group id — same idea (usually 1000)",
            s == 4,
            false,
        ),
        form_row(
            "Timezone",
            &p.tz,
            "e.g. America/Los_Angeles — keeps download times sane",
            s == 5,
            false,
        ),
        ListItem::new(Line::from(vec![
            Span::styled(
                if s == 6 { "▸ " } else { "  " },
                Style::default().fg(ACCENT),
            ),
            Span::styled(
                "[ Review the plan → ]",
                Style::default()
                    .fg(if s == 6 { Color::Black } else { GOOD })
                    .bg(if s == 6 { GOOD } else { Color::Reset })
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
    ];

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(6)])
        .split(area);

    let list = List::new(rows).block(title_block(
        "Three questions — Porthole does the hundred tiny configurations",
    ));
    f.render_widget(list, chunks[0]);

    let mut err_lines: Vec<Line> = vec![Line::from(vec![Span::styled(
        "Type to fill in, ↑↓ to move, Enter on the last row to continue.",
        Style::default().fg(DIM),
    )])];
    for e in &w.form_errors {
        err_lines.push(Line::from(vec![Span::styled(
            format!("✖ {e}"),
            Style::default().fg(BAD),
        )]));
    }
    let help = Paragraph::new(Text::from(err_lines)).block(title_block(" "));
    f.render_widget(help, chunks[1]);
}

fn render_plan(f: &mut Frame, app: &App, area: Rect) {
    let w = &app.wizard;
    let mut lines: Vec<Line> = vec![
        Line::from(vec![Span::styled(
            "Here's exactly what Porthole is about to do. Nothing runs until you say go.",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::ITALIC),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Command",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![Span::styled(
            "  bash setup.sh --yes   (in ~/.local/share/porthole/torbox-media-server)",
            Style::default().fg(Color::White),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Settings it will use",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        )]),
    ];
    for (k, v) in w.prefs.masked_env() {
        lines.push(Line::from(vec![
            Span::styled(format!("  {k:<20}"), Style::default().fg(DIM)),
            Span::styled(v, Style::default().fg(Color::White)),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        "Then it wires everything together:",
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(vec![Span::styled(
        "  Decypharr→Sonarr/Radarr · Prowlarr→Sonarr/Radarr · Seerr→everything",
        Style::default().fg(Color::White),
    )]));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(
            "[Enter]",
            Style::default().fg(GOOD).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" run it      ", Style::default().fg(DIM)),
        Span::styled(
            "[Esc]",
            Style::default().fg(WARM).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" back to questions", Style::default().fg(DIM)),
    ]));

    let para = Paragraph::new(Text::from(lines))
        .block(title_block("Dry run — the full plan, up front"))
        .wrap(Wrap { trim: false });
    f.render_widget(para, area);
}

fn spinner(tick: u64) -> &'static str {
    const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    FRAMES[(tick as usize / 4) % FRAMES.len()]
}

fn render_progress(f: &mut Frame, app: &App, area: Rect) {
    let w = &app.wizard;
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(area);

    // Left: steps.
    let mut items: Vec<ListItem> = Vec::new();
    for (i, step) in STEPS.iter().enumerate() {
        let (glyph, color) = match w.step_status[i] {
            StepStatus::Done => ("✓", GOOD),
            StepStatus::Active => ("▶", ACCENT),
            StepStatus::Failed => ("✖", BAD),
            StepStatus::Pending => ("○", DIM),
        };
        let glyph = if w.step_status[i] == StepStatus::Active {
            spinner(w.tick).to_string()
        } else {
            glyph.to_string()
        };
        items.push(ListItem::new(Text::from(vec![
            Line::from(vec![
                Span::styled(format!("{glyph} "), Style::default().fg(color)),
                Span::styled(
                    format!("{}. {}", i + 1, step.title),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::raw("    "),
                Span::styled(step.plain, Style::default().fg(Color::Gray)),
            ]),
            Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    format!("wires up: {}", step.wires_up),
                    Style::default().fg(DIM),
                ),
            ]),
        ])));
    }
    if let WizardPhase::Done(ok) = w.phase {
        items.push(ListItem::new(Line::from(vec![Span::styled(
            if ok {
                "✓ Finished — press Enter to start over, Tab for the Fleet view."
            } else {
                "✖ Stopped early — read the log, fix the issue, press Enter to retry."
            },
            Style::default().fg(if ok { GOOD } else { BAD }),
        )])));
    } else if w.phase == WizardPhase::Running {
        items.push(ListItem::new(Line::from(vec![Span::styled(
            "Quitting (q) won't stop the installer — it keeps running on its own.",
            Style::default().fg(DIM),
        )])));
    }
    let steps = List::new(items).block(title_block("Guided setup — live"));
    f.render_widget(steps, cols[0]);

    // Right: log.
    let log_lines: Vec<Line> = w
        .logs
        .iter()
        .rev()
        .take(60)
        .rev()
        .map(|l| {
            let style = if l.contains("[ok]") || l.starts_with('✓') {
                Style::default().fg(GOOD)
            } else if l.contains("[fail]") || l.contains("[error]") || l.starts_with('✖') {
                Style::default().fg(BAD)
            } else if l.contains("[warn]") {
                Style::default().fg(WARM)
            } else if l.starts_with("──") {
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };
            Line::from(Span::styled(l.clone(), style))
        })
        .collect();
    let log = Paragraph::new(Text::from(log_lines))
        .block(title_block("What Porthole is doing"))
        .wrap(Wrap { trim: false });
    f.render_widget(log, cols[1]);
}

fn render_integrations(f: &mut Frame, area: Rect) {
    let mut lines: Vec<Line> = vec![Line::from(vec![Span::styled(
        "Porthole doesn't just install apps — it introduces them to each other.",
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::ITALIC),
    )])];
    lines.push(Line::from(""));
    for integ in INTEGRATIONS {
        lines.push(Line::from(vec![Span::styled(
            format!("{} → {}", integ.from, integ.to),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        )]));
        lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(integ.plain, Style::default().fg(Color::Gray)),
        ]));
        lines.push(Line::from(""));
    }
    let para = Paragraph::new(Text::from(lines))
        .block(title_block(
            "The wiring map — every introduction, automatic",
        ))
        .wrap(Wrap { trim: false });
    f.render_widget(para, area);
}

fn render_doctor(f: &mut Frame, app: &App, area: Rect) {
    let d = &app.doctor;
    let mut items: Vec<ListItem> = vec![
        ListItem::new(Line::from(vec![Span::styled(
            "The Doctor checks every part of your fleet and explains problems in plain words.",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::ITALIC),
        )])),
        ListItem::new(Line::from("")),
    ];
    for (i, c) in d.checks.iter().enumerate() {
        let (glyph, color) = match c.status {
            CheckStatus::Pass => ("●", GOOD),
            CheckStatus::Warn => ("◐", WARM),
            CheckStatus::Fail => ("✖", BAD),
        };
        let selected = i == d.selected;
        let mut lines = vec![
            Line::from(vec![
                Span::styled(
                    if selected { "▸ " } else { "  " },
                    Style::default().fg(ACCENT),
                ),
                Span::styled(format!("{glyph} "), Style::default().fg(color)),
                Span::styled(
                    c.name.clone(),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::raw("      "),
                Span::styled(c.message.clone(), Style::default().fg(Color::Gray)),
            ]),
        ];
        if !c.fix_label.is_empty() {
            lines.push(Line::from(vec![
                Span::raw("      "),
                Span::styled(
                    format!("[f] {}", c.fix_label),
                    Style::default().fg(GOOD).add_modifier(Modifier::BOLD),
                ),
            ]));
        }
        items.push(ListItem::new(Text::from(lines)));
    }
    let list = List::new(items).block(title_block(
        "Doctor — fleet health in plain words (d to re-check)",
    ));
    f.render_widget(list, area);
}

fn render_logs(f: &mut Frame, app: &App, area: Rect) {
    let lines: Vec<Line> = app
        .wizard
        .logs
        .iter()
        .map(|l| Line::from(Span::styled(l.clone(), Style::default().fg(Color::Gray))))
        .collect();
    let para = Paragraph::new(Text::from(lines))
        .block(title_block("Logs"))
        .wrap(Wrap { trim: false });
    f.render_widget(para, area);
}

fn render_help(f: &mut Frame, area: Rect) {
    let text = Text::from(vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "Porthole in one paragraph",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
        Line::from(Span::styled(
            "Nine apps make a great media server — but only if they talk to each other.",
            Style::default().fg(Color::White),
        )),
        Line::from(Span::styled(
            "Porthole installs them, then wires them together: search sources shared,",
            Style::default().fg(Color::White),
        )),
        Line::from(Span::styled(
            "downloads handed off, libraries created, requests connected. You answer",
            Style::default().fg(Color::White),
        )),
        Line::from(Span::styled(
            "three questions; Porthole does the hundred tiny configurations.",
            Style::default().fg(Color::White),
        )),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Keys",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
        Line::from("  1–5 / Tab      switch views"),
        Line::from("  ↑ ↓            move in lists and forms"),
        Line::from("  type           fill in the setup form"),
        Line::from("  Space / ← →    switch Plex ↔ Jellyfin"),
        Line::from("  Enter          confirm / start / apply fix"),
        Line::from("  s / x / R      start / stop / restart service (Fleet view)"),
        Line::from("  r              refresh fleet status (Fleet view)"),
        Line::from("  d              re-run Doctor checks"),
        Line::from("  f              apply the Doctor's suggested fix"),
        Line::from("  ?              this help"),
        Line::from("  q              quit"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Built on the proven TorBox-Media-Server installer (55★).",
            Style::default().fg(DIM),
        )]),
    ]);
    let para = Paragraph::new(text).block(title_block("Help"));
    f.render_widget(para, area);
}
