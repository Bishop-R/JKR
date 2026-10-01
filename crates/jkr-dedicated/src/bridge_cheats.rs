//! `t_use name` (`Cmd_TargetUse_f`, `g_cmds.c`), a cheat for a server with `sv_cheats`:
//! it fires a name as the player (`G_UseTargets2(ent, ent, name)`), as mappers test a
//! map's targets and scripts. (`setviewpos` is `bridge_teleport`'s.)

use super::*;

impl NativeGame {
    /// `ClientCommand` for `t_use`: whether `text` was it.
    pub(super) fn cheat_command(&mut self, client: usize, text: &[u8]) -> bool {
        let mut words = text
            .split(|byte| byte.is_ascii_whitespace())
            .filter(|word| !word.is_empty());
        if !words
            .next()
            .is_some_and(|command| command.eq_ignore_ascii_case(b"t_use"))
        {
            return false;
        }
        // `ClientCommand`'s gates (`g_cmds.c:3466-3480`).
        if !self.settings.cheats {
            self.told
                .push(Told::One(client, b"print \"@@@NOCHEATS\n\"".to_vec()));
            return true;
        }
        if self
            .peer(client)
            .is_none_or(|peer| peer.health <= 0 || !peer.playing())
        {
            self.told
                .push(Told::One(client, b"print \"@@@MUSTBEALIVE\n\"".to_vec()));
            return true;
        }
        // `Cmd_TargetUse_f`: needs a name.
        if let Some(name) = words.next() {
            let name = String::from_utf8_lossy(name).into_owned();
            self.fire_targets(&name, client, self.last_frame_time);
        }
        true
    }
}
