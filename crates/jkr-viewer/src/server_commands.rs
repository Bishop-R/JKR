//! Presentation-side consumers for typed reliable server-command actions.

use super::Localization;
use crate::chat::ChatOverlay;
use crate::console::ViewerConsole;
use jkr_client::{BaseServerCommandEvent, ClientSession, LegacyWorldAdapter};
use std::time::Instant;

/// Drain reliable text/UI/presentation actions without retaining frame state.
pub(super) fn consume(
    session: &mut ClientSession,
    localization: &Localization,
    chat: Option<&mut ChatOverlay>,
    mut console: Option<&mut ViewerConsole>,
    mut adapter: Option<&mut LegacyWorldAdapter>,
    watch: &mut crate::clientinfo_refresh::ClientInfoWatch,
) {
    if let Some(chat) = chat {
        consume_messages(session, localization, chat, console.as_deref_mut());
    } else {
        session.drain_events().for_each(drop);
    }

    while let Some(event) = session.pop_base_command_event() {
        match event {
            BaseServerCommandEvent::UnknownCommand(name) => {
                if console
                    .as_ref()
                    .is_some_and(|c| c.integer_cvar("developer").unwrap_or(0) != 0)
                {
                    crate::log::progress(format_args!("debug: unhandled server command {name}"));
                }
            }
            BaseServerCommandEvent::ForceRank(update) => {
                if let Some(console) = &mut console {
                    console.apply_force_rank(update, session.game_state(), Instant::now());
                }
            }
            BaseServerCommandEvent::RestoreGhoul2(restore) => {
                if restore.immediate_body.is_none()
                    && let Some(adapter) = &mut adapter
                {
                    adapter.restore_client_ghoul(session.game_state(), restore);
                }
            }
            BaseServerCommandEvent::CopyBody(body) => {
                if let Some(adapter) = &mut adapter {
                    adapter.copy_body_identity(&body);
                }
                watch
                    .body_commands
                    .push(BaseServerCommandEvent::CopyBody(body));
            }
            BaseServerCommandEvent::KillGhoul2(number) => {
                if let Some(adapter) = &mut adapter {
                    adapter.kill_body_identity(number);
                }
                watch
                    .body_commands
                    .push(BaseServerCommandEvent::KillGhoul2(number));
            }
            BaseServerCommandEvent::SiegeClassSelect => {
                if let Some(console) = &mut console {
                    console.request_siege_class();
                }
            }
            BaseServerCommandEvent::SiegeProfileMenu => {
                if let Some(console) = &mut console {
                    console.request_siege_profile();
                }
            }
        }
    }
    if let Some(console) = &mut console {
        console.flush_userinfo(session, Instant::now());
    }
}

/// Receive communication from the remote endpoint without touching local actors.
pub(crate) fn consume_messages(
    session: &mut ClientSession,
    localization: &Localization,
    chat: &mut ChatOverlay,
    mut console: Option<&mut ViewerConsole>,
) {
    chat.update_roster(session.game_state());
    chat.configure(console.as_deref());
    for event in session.drain_events() {
        if let Some(console) = &mut console {
            console.notify_server_text(&event.kind, &event.text);
        }
        let text = localization.translate(&event.text);
        if let Some(console) = &mut console {
            console.log_chat(&event.kind, &text);
        }
        // stderr, not stdout: the launcher captures only stderr, so anything
        // the server tells us -- a drop reason above all -- was being written
        // where nobody would ever read it.
        crate::log::progress(format_args!(
            "server {:?}: {}",
            event.kind,
            text.replace('\n', " "),
        ));
        let now = Instant::now();
        if event.kind == jkr_client::ServerEventKind::Print {
            if let Some(console) = &mut console {
                console.push_log(text);
            }
        } else {
            chat.receive(event.kind, text, event.sender, now);
        }
    }
}
