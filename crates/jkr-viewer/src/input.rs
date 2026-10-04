//! Bind-command interpretation for gameplay input.

use jkr_protocol::UserCommand;
pub(crate) mod motion;

mod selection_commands;
pub(crate) mod settings;
mod state;
pub(crate) mod view_authority;

pub(crate) mod generic_commands;
mod key_events;
pub(crate) mod keys;

pub(crate) use generic_commands::{GENCMD_SABER_ATTACK_CYCLE, GENCMD_SABER_SWITCH};

/// Logical buttons consumed by user-command generation and free-camera motion.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum GameButton {
    Forward,
    Back,
    MoveLeft,
    MoveRight,
    Up,
    Down,
    Speed,
    Scores,
    Left,
    Right,
    Lookup,
    Lookdown,
    Strafe,
    Mlook,
    Button(u8),
}

/// One-shot action emitted by a non-button bind command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InputAction {
    MessageMode(bool),
    /// Whisper to the crosshair player, or last attacker when true.
    TargetMessage(bool),
    ToggleCamera,
    SaberToggle,
    SaberStyle,
    Weapon(u8),
    WeaponCycle(i8),
    /// Local force/inventory cycling; never a reliable server command.
    SelectionCycle(bool, i8),
    RequestScores,
    TeamMenu,
    Vote(bool),
    /// A `genCmds_t` value for the next user command (`force_throw`, `taunt`, ...).
    GenericCommand(u8),
    /// `centerview`: level the pitch (`codemp/client/cl_input.cpp:797`).
    CenterView,
    /// Apply the current cl_freelook value when the viewer handles mouse-look release.
    MlookReleased,
    /// `-grapple`: on JA+ servers EternalJK also taps `+use` to let go of the hook.
    GrappleReleased,
}

/// Current logical gameplay-input state.
pub(crate) struct GameplayInput {
    /// Snapshot angle baseline, independent of button/focus resets.
    pub(crate) view_authority: view_authority::Authority,
    /// Selection survives acknowledgement/replay and ordinary input releases.
    pub(crate) selection: jkr_client::selection::Selection,
    held: [state::KeyState; 32],
    pub(crate) motion: motion::Motion,
    /// `cl_run`: the walk key toggles walking instead of running.
    always_run: bool,
    focused: bool,
    reset_after_buffer: bool,
}

impl Default for GameplayInput {
    /// `cl_run` defaults to 1 (`codemp/client/cl_main.cpp:2773`).
    fn default() -> Self {
        Self {
            view_authority: view_authority::Authority::default(),
            selection: jkr_client::selection::Selection::default(),
            held: [state::KeyState::default(); 32],
            motion: motion::Motion::default(),
            always_run: true,
            focused: true,
            reset_after_buffer: false,
        }
    }
}

impl GameplayInput {
    /// Window focus belongs to the shell, so carry it across GPU world installs.
    pub(crate) fn inherit_focus(&mut self, previous: &Self) {
        self.focused = previous.focused;
        self.reset_after_buffer = previous.reset_after_buffer;
    }

    /// Losing focus releases held keys as `Key_ClearStates` does; holds typed at
    /// the console last until their `-` command.
    pub(crate) fn focus(&mut self, focused: bool) {
        self.focused = focused;
        if !focused {
            self.release_keys();
            self.reset_after_buffer = true;
        }
    }

    /// Focus can change after a bind was queued but before its script executes.
    /// Drop that frame's key actions too, including a quick out-and-back.
    pub(crate) fn finish_buffered_input(&mut self) -> bool {
        let reset = std::mem::take(&mut self.reset_after_buffer) || !self.focused;
        if reset {
            self.release_keys();
        }
        reset
    }

    pub(crate) fn accepts_keyboard(&self, synthetic: bool) -> bool {
        self.focused && !synthetic
    }

    /// Drop every held input, including holds typed at the console.
    pub(crate) fn clear(&mut self) {
        self.held = [state::KeyState::default(); 32];
        self.motion.clear();
    }

    /// Release keys when the console, a menu or chat takes the keyboard, as
    /// `Key_ClearStates` does, keeping holds typed at the console such as `+button12`.
    pub(crate) fn release_keys(&mut self) {
        for state in &mut self.held {
            state.release_keys();
        }
        self.motion.clear();
    }

    /// Set `button` in the next user command only, like a `+` and `-` one frame apart.
    pub(crate) fn tap(&mut self, button: GameButton) {
        self.held[button.slot()].pressed = true;
    }

    /// Latch `cl_run` (retail default 1); see `user_command`.
    pub(crate) fn set_always_run(&mut self, always_run: bool) {
        self.always_run = always_run;
    }

    pub(crate) fn held(&self, button: GameButton) -> bool {
        self.held[button.slot()].active
    }

    /// Return whether a buffered console command belongs to gameplay input.
    pub(crate) fn recognizes(command: &str) -> bool {
        let mut words = command.split_ascii_whitespace();
        let Some(name) = words.next().map(str::to_ascii_lowercase) else {
            return false;
        };
        button_for_command(&name).is_some()
            || generic_commands::generic_command(&name).is_some()
            || matches!(
                name.as_str(),
                "messagemode"
                    | "messagemode2"
                    | "messagemode3"
                    | "messagemode4"
                    | "togglecamera"
                    | "sabertoggle"
                    | "saberstyle"
                    | "centerview"
                    | "weapnext"
                    | "weapprev"
                    | "forcenext"
                    | "forceprev"
                    | "invnext"
                    | "invprev"
                    | "teammenu"
                    | "joinmenu"
                    | "vote"
                    | "weapon"
            )
    }

    /// Build the protocol-26 command represented by the logical bind state.
    /// Intermission is deliberately NOT sanitized here: the stock client's
    /// `CL_CreateCmd` sends the raw input and `codemp/game/g_active.c:838-843`
    /// (`ClientIntermissionThink`) latches `BUTTON_ATTACK|BUTTON_USE_HOLDABLE`
    /// to mark the client ready to exit; bg_pmove ignores the movement.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn user_command(
        &self,
        server_time: i32,
        camera_pitch: f32,
        camera_yaw: f32,
        delta_angles: [i32; 3],
        weapon: u8,
        force_selection: u8,
        generic_command: u8,
    ) -> UserCommand {
        let mut buttons = 0_u16;
        for (index, state) in self.held[..16].iter().enumerate() {
            if state.active || state.pressed {
                buttons |= 1 << index;
            }
        }
        // `CL_KeyMove` (`codemp/client/cl_input.cpp:888-895`): running when
        // `in_speed.active ^ cl_run->integer`, otherwise `BUTTON_WALKING` with
        // `movespeed` 46 instead of 127. With `cl_run 1` the walk key walks.
        let walking = !(self.held(GameButton::Speed) ^ self.always_run);
        let movespeed = if walking { 46 } else { 127 };
        buttons = (buttons & !16) | u16::from(walking) * 16;
        UserCommand {
            server_time,
            angles: [
                super::angle_to_short(-camera_pitch).wrapping_sub(delta_angles[0]),
                super::angle_to_short(camera_yaw).wrapping_sub(delta_angles[1]),
                0_i32.wrapping_sub(delta_angles[2]),
            ],
            forward_move: self.axis(
                GameButton::Forward,
                GameButton::Back,
                movespeed,
                self.motion.forward,
            ),
            right_move: self.axis(
                GameButton::MoveRight,
                GameButton::MoveLeft,
                movespeed,
                self.motion.side
                    + if self.held(GameButton::Strafe) {
                        movespeed as f32
                            * (self.fraction(GameButton::Right) - self.fraction(GameButton::Left))
                    } else {
                        0.0
                    },
            ),
            up_move: self.axis(GameButton::Up, GameButton::Down, movespeed, 0.0),
            buttons,
            weapon,
            force_selection: self.selection.force.unwrap_or(force_selection),
            inventory_selection: self.selection.inventory.unwrap_or(255),
            generic_command,
            ..UserCommand::default()
        }
    }

    /// Apply one command resolved by the bind table.
    pub(crate) fn apply(&mut self, command: &str) -> Option<InputAction> {
        if !self.focused {
            return None;
        }
        let mut words = command.split_ascii_whitespace();
        let name = words.next()?.to_ascii_lowercase();
        if let Some(button) = button_for_command(&name) {
            let pressed = name.starts_with('+');
            let key = words.next().and_then(|word| word.parse().ok());
            let time = words
                .next()
                .and_then(|word| word.parse().ok())
                .unwrap_or(self.motion.now);
            let changed = self.held[button.slot()].event(pressed, key, time);
            if !pressed && button == GameButton::Mlook {
                return Some(InputAction::MlookReleased);
            }
            if name == "-grapple" {
                return Some(InputAction::GrappleReleased);
            }
            return (changed && pressed && button == GameButton::Scores)
                .then_some(InputAction::RequestScores);
        }
        if let Some(value) = generic_commands::generic_command(&name) {
            return Some(InputAction::GenericCommand(value));
        }
        match name.as_str() {
            "messagemode" => Some(InputAction::MessageMode(false)),
            "messagemode2" => Some(InputAction::MessageMode(true)),
            "messagemode3" => Some(InputAction::TargetMessage(false)),
            "messagemode4" => Some(InputAction::TargetMessage(true)),
            "togglecamera" => Some(InputAction::ToggleCamera),
            "sabertoggle" => Some(InputAction::SaberToggle),
            "saberstyle" => Some(InputAction::SaberStyle),
            "centerview" => Some(InputAction::CenterView),
            "weapnext" => Some(InputAction::WeaponCycle(1)),
            "weapprev" => Some(InputAction::WeaponCycle(-1)),
            "forcenext" => Some(InputAction::SelectionCycle(false, 1)),
            "forceprev" => Some(InputAction::SelectionCycle(false, -1)),
            "invnext" => Some(InputAction::SelectionCycle(true, 1)),
            "invprev" => Some(InputAction::SelectionCycle(true, -1)),
            "teammenu" | "joinmenu" => Some(InputAction::TeamMenu),
            "vote" => match words.next().map(str::to_ascii_lowercase).as_deref() {
                Some("yes" | "y" | "1") => Some(InputAction::Vote(true)),
                Some("no" | "n" | "0") => Some(InputAction::Vote(false)),
                _ => None,
            },
            "weapon" => words
                .next()
                .and_then(|word| word.parse::<u8>().ok())
                .map(InputAction::Weapon),
            _ => None,
        }
    }
}

fn button_for_command(command: &str) -> Option<GameButton> {
    let name = command.strip_prefix(['+', '-'])?;
    if let Some(index) = name
        .strip_prefix("button")
        .and_then(|n| n.parse::<u8>().ok())
    {
        return (index < 16).then_some(GameButton::Button(index));
    }
    match name {
        "left" => Some(GameButton::Left),
        "right" => Some(GameButton::Right),
        "lookup" => Some(GameButton::Lookup),
        "lookdown" => Some(GameButton::Lookdown),
        "strafe" => Some(GameButton::Strafe),
        "mlook" => Some(GameButton::Mlook),
        "forward" => Some(GameButton::Forward),
        "back" => Some(GameButton::Back),
        "moveleft" => Some(GameButton::MoveLeft),
        "moveright" => Some(GameButton::MoveRight),
        "moveup" => Some(GameButton::Up),
        "movedown" => Some(GameButton::Down),
        "speed" => Some(GameButton::Speed),
        "attack" => Some(GameButton::Button(0)),
        "altattack" => Some(GameButton::Button(7)),
        "use" => Some(GameButton::Button(5)),
        // `+useforce` is the stock name; `+force` is the pre-step-73e spelling.
        "useforce" | "force" => Some(GameButton::Button(9)),
        "force_grip" => Some(GameButton::Button(6)),
        "force_lightning" => Some(GameButton::Button(10)),
        "force_drain" => Some(GameButton::Button(11)),
        // EternalJK cg_consolecmds.c:1003-1015 (`+grapple`): `+button12`, the
        // JA+/JaPRO grapple hook.
        "grapple" => Some(GameButton::Button(12)),
        "scores" => Some(GameButton::Scores),
        _ => None,
    }
}

impl super::GpuState {
    pub(crate) fn apply_input_action(&mut self, action: Option<InputAction>) {
        match action {
            Some(InputAction::TargetMessage(attacker)) => self.targeted_chat(attacker),
            Some(InputAction::MessageMode(team)) if self.live_session.is_some() => {
                self.gameplay_input.release_keys();
                self.chat.open(team);
                self.sync_cursor_policy();
            }
            Some(InputAction::ToggleCamera) => self.third_person = !self.third_person,
            Some(InputAction::SaberToggle) => {
                self.pending_generic_command = GENCMD_SABER_SWITCH;
            }
            Some(InputAction::SaberStyle) => {
                self.pending_generic_command = GENCMD_SABER_ATTACK_CYCLE;
            }
            Some(InputAction::GenericCommand(value)) => self.pending_generic_command = value,
            // Camera pitch is already delta-adjusted; wire encoding subtracts delta_angles.
            // Thus physical pitch zero encodes -SHORT2ANGLE(delta), cl_input.cpp:797.
            Some(InputAction::CenterView) => self.camera_pitch = 0.0,
            Some(InputAction::MlookReleased) => {
                if self
                    .console
                    .as_ref()
                    .map_or(!self.gameplay_input.motion.freelook, |console| {
                        !console.bool_cvar("cl_freelook").unwrap_or(true)
                    })
                {
                    self.camera_pitch = 0.0;
                }
            }
            Some(InputAction::GrappleReleased) => {
                if self.live_session.as_ref().is_some_and(|session| {
                    matches!(
                        session.compat_profile(),
                        jkr_client::CompatProfile::JaPlus { .. }
                    )
                }) {
                    self.gameplay_input.tap(GameButton::Button(5));
                }
            }
            Some(InputAction::Weapon(weapon)) => self.select_weapon(weapon),
            Some(InputAction::WeaponCycle(direction)) => self.cycle_weapon(direction),
            Some(InputAction::SelectionCycle(inventory, direction)) => {
                let snapshot = self
                    .live_session
                    .as_ref()
                    .map(|s| s.latest_snapshot())
                    .or_else(|| self.demo_session.as_ref().map(|s| s.latest_snapshot()));
                if let Some(snapshot) = snapshot {
                    let use_held = self.gameplay_input.held(GameButton::Button(5));
                    let time = if self.live_session.is_some() {
                        self.server_clock.server_time(std::time::Instant::now())
                    } else {
                        snapshot.server_time
                    };
                    self.gameplay_input.selection.cycle(
                        Some(&snapshot.player),
                        time,
                        inventory,
                        direction,
                        use_held,
                    );
                }
            }
            Some(InputAction::RequestScores) => {
                if let Some(session) = self.communication_session_mut()
                    && let Err(error) = session.send_reliable_command(b"score")
                {
                    eprintln!("failed to request scoreboard: {error}");
                }
            }
            Some(InputAction::TeamMenu) if self.live_session.is_some() => {
                self.release_pointer();
                self.game_menu = true;
                self.game_menu_page = crate::ingame_menu::Page::Team;
                self.game_menu_row = 0;
            }
            Some(InputAction::Vote(yes)) => {
                let command: &[u8] = if yes { b"vote yes" } else { b"vote no" };
                if let Some(session) = &mut self.live_session
                    && let Err(error) = session.send_reliable_command(command)
                {
                    eprintln!("failed to cast vote: {error}");
                }
            }
            Some(InputAction::MessageMode(_) | InputAction::TeamMenu) | None => {}
        }
    }

    fn cycle_weapon(&mut self, direction: i8) {
        let current = self.selected_weapon.unwrap_or_else(|| {
            self.live_session
                .as_ref()
                .map_or(3, |session| session.latest_snapshot().player.weapon())
        });
        let Some(session) = &self.live_session else {
            return;
        };
        let inventory =
            jkr_client::LegacyWeaponInventory::from_player_state(&session.latest_snapshot().player);
        let selected = jkr_client::legacy_cycle_weapon(&inventory, current, direction);
        if selected != current {
            self.select_weapon_exact(selected);
        }
    }

    pub(super) fn select_weapon(&mut self, slot: u8) {
        let Some(session) = &self.live_session else {
            return;
        };
        // `CG_Weapon_f` uses slot 1 as the saber toggle when the saber is
        // already selected (`cg_weapons.c:1538-1545`).
        if slot == 1
            && session.latest_snapshot().player.weapon() == 3
            && session.latest_snapshot().player.weapon_time() < 1
        {
            self.pending_generic_command = GENCMD_SABER_SWITCH;
            return;
        }
        let inventory =
            jkr_client::LegacyWeaponInventory::from_player_state(&session.latest_snapshot().player);
        if let Some(weapon) = jkr_client::legacy_direct_weapon(
            &inventory,
            session.latest_snapshot().player.weapon(),
            slot,
        ) {
            self.select_weapon_exact(weapon);
        }
    }

    pub(super) fn select_weapon_exact(&mut self, weapon: u8) {
        if let Some(session) = &self.live_session {
            let inventory = jkr_client::LegacyWeaponInventory::from_player_state(
                &session.latest_snapshot().player,
            );
            if !jkr_client::legacy_weapon_selectable(&inventory, weapon) {
                return;
            }
            self.selected_weapon = Some(weapon);
            self.weapon_selected_at = Some(std::time::Instant::now());
            self.weapon_selection_label.clear();
            self.weapon_selection_label.push_str("^3");
            self.weapon_selection_label
                .push_str(crate::ingame_menu::weapon_name(weapon));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUTTON_12: u16 = 1 << 12;

    fn buttons(input: &GameplayInput) -> u16 {
        input.user_command(0, 0.0, 0.0, [0; 3], 0, 0, 0).buttons
    }

    #[test]
    fn typed_button_hold_survives_the_console_taking_the_keyboard() {
        let mut input = GameplayInput::default();
        assert!(GameplayInput::recognizes("+button12"));
        input.apply("+button12");
        // The console consumes the Enter release and clears key states.
        input.release_keys();
        assert_ne!(buttons(&input) & BUTTON_12, 0);
        input.apply("-button12");
        input.finish_command();
        assert_eq!(buttons(&input) & BUTTON_12, 0);
    }

    #[test]
    fn bound_button_is_released_with_the_keys() {
        let mut input = GameplayInput::default();
        input.apply("+button12 77 0");
        input.release_keys();
        input.finish_command();
        assert_eq!(buttons(&input) & BUTTON_12, 0);
    }

    #[test]
    fn grapple_holds_button_12_and_reports_its_release() {
        let mut input = GameplayInput::default();
        assert!(GameplayInput::recognizes("+grapple"));
        assert_eq!(input.apply("+grapple 77 0"), None);
        assert_ne!(buttons(&input) & BUTTON_12, 0);
        assert_eq!(
            input.apply("-grapple 77 10"),
            Some(InputAction::GrappleReleased)
        );
        input.finish_command();
        assert_eq!(buttons(&input) & BUTTON_12, 0);
    }

    #[test]
    fn tap_sets_a_button_for_one_command() {
        let mut input = GameplayInput::default();
        input.tap(GameButton::Button(5));
        assert_ne!(buttons(&input) & (1 << 5), 0);
        input.finish_command();
        assert_eq!(buttons(&input) & (1 << 5), 0);
    }
}
