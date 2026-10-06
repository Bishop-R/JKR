//! The `ent->flags` bits a player's cheats set (`g_local.h`), `Cmd_God_f` and
//! `Cmd_Notarget_f` (`g_cmds.c:404-445`). `ClientSpawn` clears every flag
//! (`g_client.c:3372`), so both last one life.

/// `FL_GODMODE`: `G_Damage` spares the player (`g_combat.c:4855`).
pub const FL_GODMODE: u32 = 0x10;
/// `FL_NOTARGET`: NPCs, turrets and sentries do not pick the player.
pub const FL_NOTARGET: u32 = 0x20;

/// Flip `flag` in `flags` and say which way it went, as the commands print it:
/// `godmode ON`, `notarget OFF`.
pub fn toggle(flags: &mut u32, flag: u32, name: &str) -> Vec<u8> {
    *flags ^= flag;
    let state = if *flags & flag != 0 { "ON" } else { "OFF" };
    format!("print \"{name} {state}\n\"").into_bytes()
}
