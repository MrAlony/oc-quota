mod interceptor;
mod state;
mod ui;
mod tor;
mod nine_router;

use crossterm::{
    event::{self, Event, KeyCode, EnableMouseCapture, DisableMouseCapture, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use state::AppState;
use std::{io, sync::Arc, time::Duration};
use parking_lot::RwLock;

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        std::fs::write("fatal_error.log", format!("Application error: {}", e)).unwrap();
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Shared state
    let state = Arc::new(RwLock::new(AppState::new()));

    // Spawn background tasks
    tokio::spawn(interceptor::run_interceptor(state.clone()));
    tokio::spawn(tor::run_tor_manager(state.clone()));
    tokio::spawn(nine_router::run_9router(state.clone()));

    let mut list_state = ratatui::widgets::ListState::default();

    // Main UI loop
    loop {
        terminal.draw(|f| ui::draw(f, state.clone(), &mut list_state))?;

        if let Ok(true) = event::poll(Duration::from_millis(250)) {
            if let Ok(event) = event::read() {
                match event {
                    Event::Key(key) => {
                        if key.kind == KeyEventKind::Press {
                            match key.code {
                            KeyCode::Char('q') => break,
                            KeyCode::Tab | KeyCode::Right => {
                                let mut s = state.write();
                                s.active_tab = (s.active_tab + 1) % 2; // Assuming 2 tabs: Dashboard and Logs
                            }
                            KeyCode::Left => {
                                let mut s = state.write();
                                s.active_tab = s.active_tab.saturating_sub(1);
                            }
                            KeyCode::Up => {
                                let i = match list_state.selected() {
                                    Some(i) => if i == 0 { 0 } else { i - 1 },
                                    None => 0,
                                };
                                list_state.select(Some(i));
                            }
                            KeyCode::Down => {
                                let len = state.read().logs.len();
                                let i = match list_state.selected() {
                                    Some(i) => if i >= len.saturating_sub(1) { len.saturating_sub(1) } else { i + 1 },
                                    None => 0,
                                };
                                list_state.select(Some(i));
                            }
                            _ => {}
                            }
                        }
                    },
                    Event::Mouse(mouse_event) => {
                        match mouse_event.kind {
                            event::MouseEventKind::ScrollUp => {
                                let i = match list_state.selected() {
                                    Some(i) => if i == 0 { 0 } else { i - 1 },
                                    None => 0,
                                };
                                list_state.select(Some(i));
                            }
                            event::MouseEventKind::ScrollDown => {
                                let len = state.read().logs.len();
                                let i = match list_state.selected() {
                                    Some(i) => if i >= len.saturating_sub(1) { len.saturating_sub(1) } else { i + 1 },
                                    None => 0,
                                };
                                list_state.select(Some(i));
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}
