//! The boot map stays resident while the client is on a server, so the
//! standalone menu always comes back to the same map — with its authored
//! camera routes, stage and props — whatever map the server ran.
//!
//! A server world replaces the menu world as the current [`GpuState`] when it
//! is installed; the menu world is parked here instead of dropped and takes
//! over again as soon as the shell is back in the menu with nothing in
//! flight. Server worlds are dropped as before.

use crate::GpuState;

/// The parked menu world, if the client is currently on a server world.
pub(crate) struct Parked {
    world: Option<GpuState>,
}

impl Parked {
    pub(crate) const fn new() -> Self {
        Self { world: None }
    }

    /// Make `installed` the current world. The menu world it replaces is
    /// parked; a server world it replaces is dropped.
    pub(crate) fn install(&mut self, current: &mut GpuState, installed: GpuState) {
        let mut previous = std::mem::replace(current, installed);
        if previous.is_menu_world {
            // The world seen through the gate is the one just installed.
            previous.portal.aim(None, None);
            self.world = Some(previous);
        }
    }

    /// Hand the shell back to the parked menu world once the client has left
    /// the server and no map is loading. A join in flight comes along, so a
    /// reconnect from a server plays out in front of the menu map's gate.
    /// Returns whether the current world changed (the caller re-attaches
    /// audio).
    pub(crate) fn restore_if_idle(&mut self, current: &mut GpuState) -> bool {
        if current.is_menu_world || !current.shell_is_off_server() {
            return false;
        }
        let Some(mut menu) = self.world.take() else {
            return false;
        };
        current.hand_shell_to(&mut menu);
        if let Some(client_menu) = &mut menu.client_menu {
            client_menu.reset_gate();
        }
        menu.world_load_state = crate::session_transition::LoadStateMachine::new();
        menu.world_load_started = None;
        // The last server's map must not show through the gate of the next
        // join before the new server names its own.
        menu.world_load_map.clear();
        menu.live_map_installed = false;
        menu.resize(current.size);
        *current = menu;
        crate::log::progress(format_args!("menu world restored"));
        true
    }
}

impl GpuState {
    /// No session and no map load in flight, and the menu is up: the world
    /// only serves as the menu backdrop now.
    fn shell_is_off_server(&self) -> bool {
        !self.resident.exploring()
            && self.resident.session.is_none()
            && self.live_session.is_none()
            && self.demo_session.is_none()
            && self.world_load_task.is_none()
            && self.world_install_task.is_none()
            && !self.pending_map_reload
            && self
                .client_menu
                .as_ref()
                .is_some_and(crate::menu::ClientMenu::is_visible)
    }
}
