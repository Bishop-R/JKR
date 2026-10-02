//! Application routing for local console connection commands.

use super::*;

impl GpuState {
    /// Route console input, then apply any connection action after releasing
    /// the console/session borrow.
    pub(crate) fn route_console_key(&mut self, event: &KeyEvent) -> bool {
        let consumed = self
            .console
            .as_mut()
            .is_some_and(|console| console.handle_key(event, self.live_session.as_mut()));
        let action = self
            .console
            .as_mut()
            .and_then(console::ViewerConsole::take_connection_action);
        if let Some(action) = action {
            self.apply_console_connection_action(action);
        }
        if consumed
            && self
                .console
                .as_ref()
                .is_some_and(console::ViewerConsole::is_open)
        {
            self.gameplay_input.release_keys();
            self.release_pointer();
        }
        consumed
    }

    pub(crate) fn apply_console_connection_action(&mut self, action: console::ConnectionAction) {
        match action {
            console::ConnectionAction::Connect(address) => self.begin_address_join(address),
            console::ConnectionAction::Disconnect => self.disconnect_to_menu(),
            console::ConnectionAction::Reconnect => self.reconnect_last(),
        }
    }
}

pub(crate) fn reconnect_target(last: Option<&str>) -> Result<String, &'static str> {
    last.map(str::to_owned)
        .ok_or("No previous server address to reconnect")
}
