//! Rendering: a calm, readable TUI. No jargon on screen — every service and
//! step is described the way you'd explain it to a friend.

use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Tabs, Wrap},
    Frame,
};

use crate::{
    app::{App, Screen},
    provision::{StepStatus, STEPS},
    services::{INTEGRATIONS, SERVICES},
};

const ACCENT: Color = Color::Cyan;
const GOOD: Color = Color::Green;
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
        Screen::Logs => render_logs(f, app, root[1]),
        Screen::Help => render_help(f, root[1]),
    }

    // ── Footer ──
    let footer = Paragraph::new(Line::from(vec![
        Span::styled("Tab", Style::default().fg(ACCENT)),
        Span::styled(" switch view   ", Style::default().fg(DIM)),
        Span::styled("↑↓", Style::default().fg(ACCENT)),
        Span::styled(" move   ", Style::default().fg(DIM)),
        Span::styled("Enter", Style::default().fg(ACCENT)),
        Span::styled(" start setup   ", Style::default().fg(DIM)),
        Span::styled("q", Style::default().fg(ACCENT)),
        Span::styled(" quit", Style::default().fg(DIM)),
    ]))
    .block(
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(DIM)),
    );
    f.render_widget(footer, root[2]);
}

fn render_dashboard(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
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
        items.push(ListItem::new(Text::from(vec![
            Line::from(vec![
                Span::styled(marker, Style::default().fg(ACCENT)),
                Span::styled(format!("{:<10}", svc.name), style),
                Span::styled(format!("{:<8}", port), Style::default().fg(DIM)),
                Span::styled("○ not set up", Style::default().fg(DIM)),
            ]),
            Line::from(vec![
                Span::raw("    "),
                Span::styled(svc.plain, Style::default().fg(Color::Gray)),
            ]),
        ])));
    }
    let list = List::new(items).block(title_block("Fleet — every service, one glance"));
    f.render_widget(list, area);
}

fn render_wizard(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(area);

    // Left: steps.
    let mut items: Vec<ListItem> = Vec::new();
    for (i, step) in STEPS.iter().enumerate() {
        let (glyph, color) = match app.wizard.status[i] {
            StepStatus::Done => ("✓", GOOD),
            StepStatus::Active => ("▶", ACCENT),
            StepStatus::Pending => ("○", DIM),
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
    let steps = List::new(items).block(title_block(
        "Guided setup — plain language, no expertise needed",
    ));
    f.render_widget(steps, cols[0]);

    // Right: progress + log.
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(cols[1]);

    let gauge = Gauge::default()
        .block(title_block("Progress"))
        .gauge_style(Style::default().fg(ACCENT))
        .percent(app.wizard.progress);
    f.render_widget(gauge, right[0]);

    let log_lines: Vec<Line> = app
        .wizard
        .logs
        .iter()
        .rev()
        .take(40)
        .rev()
        .map(|l| {
            let style = if l.contains("[ok]") {
                Style::default().fg(GOOD)
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
    f.render_widget(log, right[1]);
}

fn render_integrations(f: &mut Frame, area: ratatui::layout::Rect) {
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

fn render_logs(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
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

fn render_help(f: &mut Frame, area: ratatui::layout::Rect) {
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
        Line::from("  1–4 / Tab      switch views"),
        Line::from("  ↑ ↓            move in the fleet list"),
        Line::from("  Enter          start the guided setup (on the Setup view)"),
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
