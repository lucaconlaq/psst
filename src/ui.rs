use self::{
    app::{App, Tasks},
    render::draw,
};
use crate::config::Config;
use anyhow::{Result, bail};
use crossterm::{
    event::{self, DisableBracketedPaste, EnableBracketedPaste, Event},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::StreamExt;
use ratatui::prelude::*;
use std::{
    io::{self, IsTerminal},
    time::Duration,
};
use tokio::sync::mpsc;

mod app;
mod editor;
mod render;

#[cfg(test)]
mod tests;

struct TerminalGuard;
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            DisableBracketedPaste,
            LeaveAlternateScreen,
            crossterm::cursor::Show
        );
    }
}
/// Run the interactive manager on terminal stdin/stdout, restoring state on exit.
/// Fetches and browsers run independently; previews are explicit and expire.
pub async fn run(config: Config) -> Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        bail!("The manager needs a terminal. Use --help, --write, --copy NAME or a command.");
    }
    terminal::enable_raw_mode()?;
    let _guard = TerminalGuard;
    execute!(io::stdout(), EnterAlternateScreen, EnableBracketedPaste)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let old_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|info| {
        let _ = terminal::disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, crossterm::cursor::Show);
        eprintln!("psst panicked: {info}");
    }));
    let result = loop_run(&mut terminal, App::new(config)).await;
    std::panic::set_hook(old_hook);
    result
}
async fn loop_run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    mut app: App,
) -> Result<()> {
    let mut events = event::EventStream::new();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut tasks = Tasks(Vec::new());
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    #[cfg(unix)]
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    #[cfg(not(unix))]
    let mut term = ();
    loop {
        tasks.cancel_stale(app.generation);
        app.expire();
        terminal.draw(|f| draw(f, &mut app))?;
        tokio::select! {
            event = events.next() => match event {
                Some(Ok(Event::Key(key))) => if app.key(key, &tx, &mut tasks) { break; },
                Some(Ok(Event::Paste(text))) => app.paste(text),
                Some(Err(e)) => return Err(e.into()),
                None => break,
                _ => {}
            },
            Some(msg) = rx.recv() => app.message(msg),
            _ = tick.tick() => app.expire(),
            _ = tokio::signal::ctrl_c() => break,
            _ = termination(&mut term) => break,
        }
    }
    Ok(())
}
#[cfg(unix)]
async fn termination(term: &mut tokio::signal::unix::Signal) {
    term.recv().await;
}

#[cfg(not(unix))]
async fn termination(_: &mut ()) {
    std::future::pending::<()>().await;
}
