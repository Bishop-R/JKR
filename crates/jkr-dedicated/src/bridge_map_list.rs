//! jaPRO's `amListMaps` on this server (`Cmd_AmMapList_f`, `g_cmds.c:2843-2890`, the
//! list-maps admin power): every map the server has beyond the base game's, sorted, in
//! green and yellow in turn, wrapped as jaPRO wraps them, then how many. jaPRO reads at
//! most 4 KiB of names and 512 maps; here every map is listed.

use super::bridge_cheats::NO_INTERMISSION;
use super::*;

/// `defaultMaps`: the base game's maps, single player's among them, which are not listed.
const BASE_MAPS: [&str; 57] = [
    "academy1",
    "academy2",
    "academy3",
    "academy4",
    "academy5",
    "academy6",
    "hoth2",
    "hoth3",
    "kor1",
    "kor2",
    "mp/ctf1",
    "mp/ctf2",
    "mp/ctf3",
    "mp/ctf4",
    "mp/ctf5",
    "mp/duel1",
    "mp/duel10",
    "mp/duel2",
    "mp/duel3",
    "mp/duel4",
    "mp/duel5",
    "mp/duel6",
    "mp/duel7",
    "mp/duel8",
    "mp/duel9",
    "mp/ffa1",
    "mp/ffa2",
    "mp/ffa3",
    "mp/ffa4",
    "mp/ffa5",
    "mp/siege_desert",
    "mp/siege_hoth",
    "mp/siege_korriban",
    "t1_danger",
    "t1_fatal",
    "t1_inter",
    "t1_rail",
    "t1_sour",
    "t1_surprise",
    "t2_dpred",
    "t2_rancor",
    "t2_rogue",
    "t2_trip",
    "t2_wedge",
    "t3_bounty",
    "t3_byss",
    "t3_hevil",
    "t3_rift",
    "t3_stamp",
    "taspir1",
    "taspir2",
    "vjun1",
    "vjun2",
    "vjun3",
    "yavin1",
    "yavin1b",
    "yavin2",
];
/// A line wraps once it holds this many bytes of entries.
const LINE: usize = 192;
/// A print stays below this many bytes.
const PRINT: usize = 512;

impl NativeGame {
    /// `amListMaps` from `client`; whether `command` was it.
    pub(super) fn amlistmaps_command(&mut self, client: usize, command: &[u8]) -> bool {
        if !command.eq_ignore_ascii_case(b"amlistmaps") {
            return false;
        }
        if !self.passes_gates(client, command, NO_INTERMISSION)
            || !self.admin_allowed(client, crate::admin::A_LISTMAPS, "amListMaps")
        {
            return true;
        }
        let mut maps: Vec<String> = self
            .config_files
            .list_files("maps", ".bsp")
            .into_iter()
            .map(|name| name.strip_suffix(".bsp").unwrap_or(&name).to_owned())
            .collect();
        if maps.is_empty() {
            return true;
        }
        maps.sort();
        let mut buffer = String::from("   ");
        let (mut count, mut base) = (0, 0);
        for (index, map) in maps.iter().enumerate() {
            if BASE_MAPS
                .iter()
                .any(|known| known.eq_ignore_ascii_case(map))
            {
                base += 1;
                continue;
            }
            let colour = if index % 2 == 1 { '2' } else { '3' };
            let mut entry = format!(" ^{colour}{map:<32}    ");
            if count >= LINE {
                entry = format!("\n   {entry}");
                count = 0;
            }
            if buffer.len() + entry.len() >= PRINT {
                self.told.push(Told::One(
                    client,
                    format!("print \"{buffer}\"").into_bytes(),
                ));
                buffer.clear();
            }
            count += entry.len();
            buffer.push_str(&entry);
        }
        self.told.push(Told::One(
            client,
            format!("print \"{buffer}\n\"").into_bytes(),
        ));
        self.told.push(Told::One(
            client,
            format!("print \"^5{} maps listed\n\"", maps.len() - base).into_bytes(),
        ));
        true
    }
}
