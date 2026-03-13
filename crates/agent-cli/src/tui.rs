use anyhow::Result;
use crossterm::event::{self, Event, KeyEvent, KeyEventKind, MouseEvent};
use std::time::Duration;
use tokio::sync::mpsc;

use agent_core::{InputAttachment, PermissionPreset, ThinkingLevel};
use agent_storage::Storage;

use crate::ensure_daemon;

mod app;
mod events;
mod render;
mod terminal;

use app::TuiApp;
use events::spawn_daemon_event_poller;
use render::draw_app;
use terminal::TerminalSession;

pub(crate) async fn run_tui_session(
    storage: &Storage,
    alias: Option<String>,
    session_id: Option<String>,
    initial_prompt: Option<String>,
    thinking_level: Option<ThinkingLevel>,
    attachments: Vec<InputAttachment>,
    permission_preset: Option<PermissionPreset>,
) -> Result<()> {
    let client = ensure_daemon(storage).await?;
    let mut app = TuiApp::new(
        storage,
        client,
        alias,
        session_id,
        thinking_level,
        attachments,
        permission_preset,
    )
    .await?;

    let mut terminal = Some(TerminalSession::new()?);
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    spawn_daemon_event_poller(app.client.clone(), app.event_cursor(), event_tx.clone());

    if let Some(prompt) = initial_prompt {
        app.queue_prompt(prompt, &event_tx)?;
    }

    loop {
        while let Ok(app_event) = event_rx.try_recv() {
            if let Err(error) = app.handle_event(app_event).await {
                app.record_error(format!("{error:#}"));
            }
        }

        if let Some(action) = app.take_external_action() {
            drop(terminal.take());
            if let Err(error) = app.run_external_action(action).await {
                app.record_error(format!("{error:#}"));
            }
            terminal = Some(TerminalSession::new()?);
        }

        if let Some(active_terminal) = terminal.as_mut() {
            active_terminal.draw(|frame| draw_app(frame, &app))?;
        }
        if app.exit_requested() {
            break;
        }

        if !event::poll(Duration::from_millis(100))? {
            continue;
        }

        match event::read()? {
            Event::Key(key) => {
                if !should_process_key_event(&key) {
                    continue;
                }
                if let Err(error) = app.handle_key(key, &event_tx).await {
                    app.record_error(format!("{error:#}"));
                }
            }
            Event::Mouse(mouse) => {
                if should_process_mouse_event(&mouse) {
                    if let Err(error) = app.handle_mouse(mouse).await {
                        app.record_error(format!("{error:#}"));
                    }
                }
            }
            _ => {}
        }
    }

    Ok(())
}

fn should_process_key_event(key: &KeyEvent) -> bool {
    matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
}

fn should_process_mouse_event(_mouse: &MouseEvent) -> bool {
    matches!(
        _mouse.kind,
        crossterm::event::MouseEventKind::ScrollUp | crossterm::event::MouseEventKind::ScrollDown
    )
}

#[cfg(test)]
mod tests {
    use super::should_process_key_event;
    use super::should_process_mouse_event;
    use crossterm::event::{
        KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind,
    };

    #[test]
    fn processes_press_events() {
        let key =
            KeyEvent::new_with_kind(KeyCode::Char('a'), KeyModifiers::NONE, KeyEventKind::Press);
        assert!(should_process_key_event(&key));
    }

    #[test]
    fn processes_repeat_events() {
        let key =
            KeyEvent::new_with_kind(KeyCode::Backspace, KeyModifiers::NONE, KeyEventKind::Repeat);
        assert!(should_process_key_event(&key));
    }

    #[test]
    fn ignores_release_events() {
        let key = KeyEvent::new_with_kind(
            KeyCode::Char('a'),
            KeyModifiers::NONE,
            KeyEventKind::Release,
        );
        assert!(!should_process_key_event(&key));
    }

    #[test]
    fn processes_mouse_events() {
        let mouse = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        };
        assert!(should_process_mouse_event(&mouse));
    }

    #[test]
    fn ignores_non_scroll_mouse_events() {
        let mouse = MouseEvent {
            kind: MouseEventKind::Moved,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        };
        assert!(!should_process_mouse_event(&mouse));
    }
}
