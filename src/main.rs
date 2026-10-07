//! Porthole — your self-hosted media fleet through one window.
//!
//! A TUI that installs and, more importantly, *integrates* a complete
//! self-hosted media stack: TorBox, Decypharr, Prowlarr, Byparr, Sonarr,
//! Radarr, Seerr, Plex and Jellyfin. No expert knowledge required.

mod app;
mod config;
mod docker;
mod provision;
mod services;
mod ui;

use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::Backend, backend::CrosstermBackend, Terminal};

use app::App;

fn main() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let res = run(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(e) = res {
        eprintln!("{e:?}");
    }
    Ok(())
}

fn run<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|f| ui::render(f, app))?;
        if event::poll(Duration::from_millis(120))? {
            if let Event::Key(key) = event::read()? {
                app.on_key(key.code);
            }
        }
        app.on_tick();
        if app.should_quit {
            return Ok(());
        }
    }
}
