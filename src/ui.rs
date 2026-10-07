//! Rendering: a calm, readable TUI. No jargon on screen — every service and
//! step is described the way you'd explain it to a friend.

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Tabs, Wrap},
    Frame,
};

use crate::{
    app::{App, CareOp, CareView, CheckStatus, Screen, WizardPhase, CARE_ACTIONS},
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
            format!(
                " v{}  — your self-hosted media fleet, through one window",
                crate::selfupdate::CURRENT_VERSION
            ),
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
        Screen::Care => render_care(f, app, root[1]),
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
        Screen::Care => keys.extend([("Esc", "back")]),
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

    if app.show_welcome {
        render_welcome(f, f.area());
    }
}

/// Centered rectangle taking `w` x `h` of the area (percentages).
fn centered_rect(w: u16, h: u16, area: Rect) -> Rect {
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - h) / 2),
            Constraint::Percentage(h),
            Constraint::Percentage((100 - h) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - w) / 2),
            Constraint::Percentage(w),
            Constraint::Percentage((100 - w) / 2),
        ])
        .split(vert[1])[1]
}

fn render_welcome(f: &mut Frame, area: Rect) {
    let popup = centered_rect(70, 55, area);
    f.render_widget(Clear, popup);
    let text = Text::from(vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "⛵  Welcome to Porthole",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Your whole media fleet — TorBox, the *arrs, Seerr, and Plex or",
            Style::default().fg(Color::Gray),
        )]),
        Line::from(vec![Span::styled(
            "Jellyfin — installed and wired together, with no expert knowledge",
            Style::default().fg(Color::Gray),
        )]),
        Line::from(vec![Span::styled(
            "needed from you.",
            Style::default().fg(Color::Gray),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Porthole doesn't just install apps — it introduces them to each",
            Style::default().fg(Color::Gray),
        )]),
        Line::from(vec![Span::styled(
            "other, then looks after them: health checks, backups, and safe",
            Style::default().fg(Color::Gray),
        )]),
        Line::from(vec![Span::styled(
            "one-key updates with automatic rollback.",
            Style::default().fg(Color::Gray),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "[Enter]",
                Style::default().fg(GOOD).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" set up my fleet      ", Style::default().fg(DIM)),
            Span::styled("[Esc]", Style::default().fg(ACCENT)),
            Span::styled(" look around first", Style::default().fg(DIM)),
        ]),
    ]);
    let para = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ACCENT))
                .title(" Ahoy "),
        )
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: false });
    f.render_widget(para, popup);
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
            "Debrid key",
            &p.torbox_api_key,
            "API key for your debrid service — kept masked, never shown",
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
        ListItem::new(Text::from(vec![
            Line::from(vec![
                Span::styled(
                    if s == 6 { "▸ " } else { "  " },
                    Style::default().fg(ACCENT),
                ),
                Span::styled(
                    format!("{:<14}", "Quality"),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("< {} >", if p.quality_4k { "4K" } else { "1080p" }),
                    Style::default().fg(ACCENT),
                ),
            ]),
            Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    "Space / ← → to switch. 4K needs ~60 Mbps to stream smoothly.",
                    Style::default().fg(DIM),
                ),
            ]),
        ])),
        ListItem::new(Text::from(vec![
            Line::from(vec![
                Span::styled(
                    if s == 7 { "▸ " } else { "  " },
                    Style::default().fg(ACCENT),
                ),
                Span::styled(
                    format!("{:<14}", "Music"),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("< {} >", if p.extras.lidarr { "yes" } else { "no" }),
                    Style::default().fg(ACCENT),
                ),
            ]),
            Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    "Lidarr: your music butler. Space / ← → to switch.",
                    Style::default().fg(DIM),
                ),
            ]),
        ])),
        ListItem::new(Text::from(vec![
            Line::from(vec![
                Span::styled(
                    if s == 8 { "▸ " } else { "  " },
                    Style::default().fg(ACCENT),
                ),
                Span::styled(
                    format!("{:<14}", "Subtitles"),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("< {} >", if p.extras.bazarr { "yes" } else { "no" }),
                    Style::default().fg(ACCENT),
                ),
            ]),
            Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    "Bazarr: fetches subtitles automatically. Space / ← → to switch.",
                    Style::default().fg(DIM),
                ),
            ]),
        ])),
        ListItem::new(Text::from(vec![
            Line::from(vec![
                Span::styled(
                    if s == 9 { "▸ " } else { "  " },
                    Style::default().fg(ACCENT),
                ),
                Span::styled(
                    format!("{:<14}", "Sports"),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("< {} >", if p.extras.sportarr { "yes" } else { "no" }),
                    Style::default().fg(ACCENT),
                ),
            ]),
            Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    "Sportarr: follows your teams (newer, opt-in). Space / ← → to switch.",
                    Style::default().fg(DIM),
                ),
            ]),
        ])),
        ListItem::new(Text::from(vec![
            Line::from(vec![
                Span::styled(
                    if s == 11 { "▸ " } else { "  " },
                    Style::default().fg(ACCENT),
                ),
                Span::styled(
                    format!("{:<14}", "Downloads via"),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("< {} >", p.fleet_profile.label()),
                    Style::default().fg(ACCENT),
                ),
            ]),
            Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    "Debrid = simple. Self-downloaded = private, needs a VPN login later.",
                    Style::default().fg(DIM),
                ),
            ]),
        ])),
        ListItem::new(Text::from(vec![
            Line::from(vec![
                Span::styled(
                    if s == 12 { "▸ " } else { "  " },
                    Style::default().fg(ACCENT),
                ),
                Span::styled(
                    format!("{:<14}", "Debrid service"),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("< {} >", p.debrid_provider.label()),
                    Style::default().fg(ACCENT),
                ),
            ]),
            Line::from(vec![
                Span::raw("    "),
                Span::styled(p.debrid_provider.blurb(), Style::default().fg(DIM)),
            ]),
        ])),
        ListItem::new(Line::from(vec![
            Span::styled(
                if s == 13 { "▸ " } else { "  " },
                Style::default().fg(ACCENT),
            ),
            Span::styled(
                "[ Review the plan → ]",
                Style::default()
                    .fg(if s == 13 { Color::Black } else { GOOD })
                    .bg(if s == 13 { GOOD } else { Color::Reset })
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
    ];

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(6)])
        .split(area);

    let list = List::new(rows).block(title_block(
        "A few questions — Porthole does the hundred tiny configurations",
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
    lines.push(Line::from(vec![
        Span::styled(
            format!("  {:<20}", "Downloads via"),
            Style::default().fg(DIM),
        ),
        Span::styled(
            w.prefs.fleet_profile.label(),
            Style::default().fg(Color::White),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled(
            format!("  {:<20}", "Debrid service"),
            Style::default().fg(DIM),
        ),
        Span::styled(
            w.prefs.debrid_provider.label(),
            Style::default().fg(Color::White),
        ),
    ]));
    if w.prefs.fleet_profile.needs_local_clients() {
        lines.push(Line::from(vec![Span::styled(
            "  (Self-downloaded needs a VPN login — you'll add it in Care afterwards)",
            Style::default().fg(WARM),
        )]));
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

fn render_care(f: &mut Frame, app: &App, area: Rect) {
    let c = &app.care;
    match c.view {
        CareView::Main => {
            let mut items: Vec<ListItem> = vec![
                ListItem::new(Line::from(vec![Span::styled(
                    "Look after your fleet: backups, updates, and a clean goodbye.",
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::ITALIC),
                )])),
                ListItem::new(Line::from("")),
            ];
            for (i, make_op) in CARE_ACTIONS.iter().enumerate() {
                let op = make_op();
                // Skip the restore placeholder's description duplication.
                let selected = i == c.selected;
                items.push(ListItem::new(Text::from(vec![
                    Line::from(vec![
                        Span::styled(
                            if selected { "▸ " } else { "  " },
                            Style::default().fg(ACCENT),
                        ),
                        Span::styled(
                            op.title(),
                            Style::default()
                                .fg(Color::White)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]),
                    Line::from(vec![
                        Span::raw("    "),
                        Span::styled(op.plain(), Style::default().fg(Color::Gray)),
                    ]),
                ])));
            }
            let list = List::new(items).block(title_block("Care — backups, updates, uninstall"));
            f.render_widget(list, area);
        }
        CareView::PickBackup => {
            let mut items: Vec<ListItem> = vec![ListItem::new(Line::from(vec![Span::styled(
                "Which snapshot should come back?",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::ITALIC),
            )]))];
            if c.backups.is_empty() {
                items.push(ListItem::new(Line::from(vec![Span::styled(
                    "No backups yet — choose “Back up now” first.",
                    Style::default().fg(WARM),
                )])));
            }
            for (i, p) in c.backups.iter().enumerate() {
                let selected = i == c.selected;
                items.push(ListItem::new(Line::from(vec![
                    Span::styled(
                        if selected { "▸ " } else { "  " },
                        Style::default().fg(ACCENT),
                    ),
                    Span::styled(
                        p.file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string(),
                        Style::default().fg(if selected { Color::White } else { Color::Gray }),
                    ),
                ])));
            }
            let list = List::new(items).block(title_block("Pick a backup (Esc to go back)"));
            f.render_widget(list, area);
        }
        CareView::PickExtras => {
            let e = &c.extras_pick;
            let rows = [
                (
                    "Music (Lidarr)",
                    e.lidarr,
                    "Your music butler — follows artists, grabs albums.",
                ),
                (
                    "Subtitles (Bazarr)",
                    e.bazarr,
                    "Fetches subtitles for everything, automatically.",
                ),
                (
                    "Sports (Sportarr)",
                    e.sportarr,
                    "Follows your teams. Newer — opt-in.",
                ),
                (
                    "Racing (autobrr)",
                    e.autobrr,
                    "Grabs private-tracker releases the second they appear. Power users.",
                ),
            ];
            let mut items: Vec<ListItem> = vec![ListItem::new(Line::from(vec![Span::styled(
                "Which extras should join the fleet?",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::ITALIC),
            )]))];
            for (i, (label, on, hint)) in rows.iter().enumerate() {
                let selected = i == c.selected;
                items.push(ListItem::new(Text::from(vec![
                    Line::from(vec![
                        Span::styled(
                            if selected { "▸ " } else { "  " },
                            Style::default().fg(ACCENT),
                        ),
                        Span::styled(
                            format!("< {} > {label}", if *on { "yes" } else { "no" }),
                            Style::default().fg(if selected { Color::White } else { Color::Gray }),
                        ),
                    ]),
                    Line::from(vec![
                        Span::raw("    "),
                        Span::styled(*hint, Style::default().fg(DIM)),
                    ]),
                ])));
            }
            items.push(ListItem::new(Line::from(vec![Span::styled(
                "Space toggles · Enter applies · Esc cancels",
                Style::default().fg(DIM),
            )])));
            let list = List::new(items).block(title_block("Extra services"));
            f.render_widget(list, area);
        }
        CareView::VpnForm => {
            let providers = crate::app::VPN_PROVIDERS;
            let prov = providers[c.vpn_provider_idx % providers.len()];
            let masked = "•".repeat(c.vpn_key.chars().count());
            let masked_empty = masked.is_empty();
            let items = vec![
                ListItem::new(Line::from(vec![Span::styled(
                    "Your VPN login — downloads route through it.",
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::ITALIC),
                )])),
                ListItem::new(Text::from(vec![
                    Line::from(vec![
                        Span::styled(
                            if c.vpn_field == 0 { "▸ " } else { "  " },
                            Style::default().fg(ACCENT),
                        ),
                        Span::styled(
                            format!("Provider:  < {prov} >"),
                            Style::default().fg(Color::White),
                        ),
                    ]),
                    Line::from(vec![
                        Span::raw("    "),
                        Span::styled(
                            "Space switches · PIA or Proton VPN (both support port forwarding)",
                            Style::default().fg(DIM),
                        ),
                    ]),
                ])),
                ListItem::new(Text::from(vec![
                    Line::from(vec![
                        Span::styled(
                            if c.vpn_field == 1 { "▸ " } else { "  " },
                            Style::default().fg(ACCENT),
                        ),
                        Span::styled("WireGuard key:  ", Style::default().fg(Color::White)),
                        Span::styled(
                            if masked_empty {
                                "(paste it here)".to_string()
                            } else {
                                masked
                            },
                            Style::default().fg(if masked_empty { DIM } else { GOOD }),
                        ),
                    ]),
                    Line::from(vec![
                        Span::raw("    "),
                        Span::styled(
                            "From your VPN provider's dashboard (WireGuard private key)",
                            Style::default().fg(DIM),
                        ),
                    ]),
                ])),
                ListItem::new(Line::from(vec![Span::styled(
                    "↑↓ switch field · Enter continues · Esc cancels",
                    Style::default().fg(DIM),
                )])),
            ];
            let list = List::new(items).block(title_block("VPN for downloads"));
            f.render_widget(list, area);
        }
        CareView::Confirm => {
            let mut lines: Vec<Line> = vec![Line::from(vec![Span::styled(
                "Please read this before you say yes:",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )])];
            lines.push(Line::from(""));
            if let Some(op) = &c.pending_op {
                for l in op.confirm_lines(&app.config.install_dir) {
                    lines.push(Line::from(vec![Span::styled(
                        l,
                        Style::default().fg(Color::Gray),
                    )]));
                }
                lines.push(Line::from(""));
                let is_uninstall = matches!(op, CareOp::Uninstall);
                if is_uninstall && !c.confirm_armed {
                    lines.push(Line::from(vec![
                        Span::styled(
                            "[Enter]",
                            Style::default().fg(WARM).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            " yes, I understand — ask me once more",
                            Style::default().fg(DIM),
                        ),
                    ]));
                } else if is_uninstall {
                    lines.push(Line::from(vec![
                        Span::styled(
                            "[Enter]",
                            Style::default().fg(BAD).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            " YES, remove everything now",
                            Style::default().fg(BAD).add_modifier(Modifier::BOLD),
                        ),
                    ]));
                } else {
                    lines.push(Line::from(vec![
                        Span::styled(
                            "[Enter]",
                            Style::default().fg(GOOD).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(" do it      ", Style::default().fg(DIM)),
                    ]));
                }
                lines.push(Line::from(vec![
                    Span::styled("[Esc]", Style::default().fg(ACCENT)),
                    Span::styled(" back", Style::default().fg(DIM)),
                ]));
            }
            let para = Paragraph::new(Text::from(lines))
                .block(title_block("Confirm — no surprises"))
                .wrap(Wrap { trim: false });
            f.render_widget(para, area);
        }
        CareView::Working => {
            let log_lines: Vec<Line> = c
                .logs
                .iter()
                .rev()
                .take(40)
                .rev()
                .map(|l| {
                    let style = if l.contains("[ok]") {
                        Style::default().fg(GOOD)
                    } else if l.contains("[fail]") {
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
            let block_title = format!("Working {} — please wait", spinner(c.tick));
            let log = Paragraph::new(Text::from(log_lines)).block(title_block(&block_title));
            f.render_widget(log, area);
        }
        CareView::Done => {
            let color = if c.done_ok { GOOD } else { BAD };
            let glyph = if c.done_ok { "✓" } else { "✖" };
            let para = Paragraph::new(Text::from(vec![
                Line::from(""),
                Line::from(vec![Span::styled(
                    format!("{glyph} {}", c.done_message),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                )]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("[Enter]", Style::default().fg(ACCENT)),
                    Span::styled(" back to Care", Style::default().fg(DIM)),
                ]),
            ]))
            .block(title_block("Done"))
            .wrap(Wrap { trim: false });
            f.render_widget(para, area);
        }
    }
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
    let mut text = Text::from(vec![
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
        Line::from("  1–6 / Tab      switch views"),
        Line::from("  ↑ ↓            move in lists and forms"),
        Line::from("  type           fill in the setup form"),
        Line::from("  Space / ← →    switch Plex ↔ Jellyfin"),
        Line::from("  Enter          confirm / start / apply fix"),
        Line::from("  Esc            back out of a choice"),
        Line::from("  s / x / R      start / stop / restart service (Fleet view)"),
        Line::from("  r              refresh fleet status (Fleet view)"),
        Line::from("  d              re-run Doctor checks"),
        Line::from("  f              apply the Doctor's suggested fix"),
        Line::from("  ?              this help"),
        Line::from("  q              quit"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Privacy — what your provider can see",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
    ]);
    for line in crate::download::privacy_explainer() {
        if line.is_empty() {
            text.lines.push(Line::from(""));
        } else if line.starts_with("•") {
            text.lines
                .push(Line::from(Span::styled(line, Style::default().fg(DIM))));
        } else if line.ends_with(":") {
            text.lines.push(Line::from(vec![Span::styled(
                line,
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )]));
        } else {
            text.lines.push(Line::from(Span::styled(
                line,
                Style::default().fg(Color::White),
            )));
        }
    }
    text.lines.push(Line::from(""));
    text.lines.push(Line::from(vec![Span::styled(
        "Built on the proven TorBox-Media-Server installer (55★).",
        Style::default().fg(DIM),
    )]));
    let para = Paragraph::new(text).block(title_block("Help"));
    f.render_widget(para, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    fn screen_text(app: &App, w: u16, h: u16) -> String {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, app)).unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    #[test]
    fn welcome_overlay_renders_on_first_run() {
        let mut app = App::new();
        app.show_welcome = true;
        let text = screen_text(&app, 100, 40);
        assert!(text.contains("Welcome to Porthole"));
        assert!(text.contains("set up my fleet"));
        assert!(text.contains("look around first"));
    }

    #[test]
    fn welcome_dismiss_goes_to_setup_or_stays() {
        let mut app = App::new();
        app.show_welcome = true;
        // Shield the test from touching the real config file.
        app.config.onboarded = true;
        app.on_key(crossterm::event::KeyCode::Enter);
        assert!(!app.show_welcome);
        assert_eq!(app.screen, Screen::Wizard);
    }

    #[test]
    fn header_shows_version() {
        let app = App::new();
        let text = screen_text(&app, 100, 40);
        assert!(text.contains(&format!("v{}", crate::selfupdate::CURRENT_VERSION)));
    }
}
