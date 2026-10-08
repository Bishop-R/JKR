//! NPC voices. An NPC names up to four sound sets in its entity state (`csSounds_Std`,
//! `csSounds_Combat`, `csSounds_Extra`, `csSounds_Jedi`), each a `CS_SOUNDS` slot holding
//! `*$<set>`; the client loads every name of the matching list from
//! `sound/chars/<set>/misc/` (`CG_HandleNPCSounds`, `CG_RegisterCustomSounds`,
//! `cg_servercmds.c:412-660`). A custom sound the NPC makes (`*pain50`, `*anger2`,
//! `*detected4`...) is looked up as `CG_CustomSound` looks it up for an NPC
//! (`cg_players.c`): the first position, in all lists at once, holding the name wins, and
//! the duel list (in duels), whose sounds an NPC never has, wins too — so in a duel an
//! NPC's anger, victory, taunts, deflections and gloats are silent, as in stock.

/// `cg_customSoundNames`, `cg_customCombatSoundNames`, `cg_customExtraSoundNames` and
/// `cg_customJediSoundNames`, in their order.
const LISTS: [&[&str]; 4] = [
    &[
        "death1", "death2", "death3", "jump1", "pain25", "pain50", "pain75", "pain100", "falling1",
        "choke1", "choke2", "choke3", "gasp", "land1", "taunt",
    ],
    &[
        "anger1", "anger2", "anger3", "victory1", "victory2", "victory3", "confuse1", "confuse2",
        "confuse3", "pushed1", "pushed2", "pushed3", "choke1", "choke2", "choke3", "ffwarn",
        "ffturn",
    ],
    &[
        "chase1",
        "chase2",
        "chase3",
        "cover1",
        "cover2",
        "cover3",
        "cover4",
        "cover5",
        "detected1",
        "detected2",
        "detected3",
        "detected4",
        "detected5",
        "lost1",
        "outflank1",
        "outflank2",
        "escaping1",
        "escaping2",
        "escaping3",
        "giveup1",
        "giveup2",
        "giveup3",
        "giveup4",
        "look1",
        "look2",
        "sight1",
        "sight2",
        "sight3",
        "sound1",
        "sound2",
        "sound3",
        "suspicious1",
        "suspicious2",
        "suspicious3",
        "suspicious4",
        "suspicious5",
    ],
    &[
        "combat1",
        "combat2",
        "combat3",
        "jdetected1",
        "jdetected2",
        "jdetected3",
        "taunt1",
        "taunt2",
        "taunt3",
        "jchase1",
        "jchase2",
        "jchase3",
        "jlost1",
        "jlost2",
        "jlost3",
        "deflect1",
        "deflect2",
        "deflect3",
        "gloat1",
        "gloat2",
        "gloat3",
        "pushfail",
    ],
];

/// `cg_customDuelSoundNames`: searched before the NPC lists in duels.
const DUEL: &[&str] = &[
    "anger1", "anger2", "anger3", "victory1", "victory2", "victory3", "taunt1", "taunt2", "taunt3",
    "deflect1", "deflect2", "deflect3", "gloat1", "gloat2", "gloat3",
];

/// `GT_DUEL`, `GT_POWERDUEL`.
const GT_DUEL: i32 = 3;
const GT_POWERDUEL: i32 = 4;
const MAX_SOUNDS: usize = 256;

/// The voice events, `EV_ANGER1` (116) to `EV_PUSHFAIL` (190), and the custom sound
/// each plays (`CG_EntityEvent`, `cg_event.c`), or `None` for another event.
pub fn voice_event_name(event: u16) -> Option<&'static str> {
    const NAMES: [&str; 75] = [
        "anger1",
        "anger2",
        "anger3",
        "victory1",
        "victory2",
        "victory3",
        "confuse1",
        "confuse2",
        "confuse3",
        "pushed1",
        "pushed2",
        "pushed3",
        "choke1",
        "choke2",
        "choke3",
        "ffwarn",
        "ffturn",
        "chase1",
        "chase2",
        "chase3",
        "cover1",
        "cover2",
        "cover3",
        "cover4",
        "cover5",
        "detected1",
        "detected2",
        "detected3",
        "detected4",
        "detected5",
        "lost1",
        "outflank1",
        "outflank2",
        "escaping1",
        "escaping2",
        "escaping3",
        "giveup1",
        "giveup2",
        "giveup3",
        "giveup4",
        "look1",
        "look2",
        "sight1",
        "sight2",
        "sight3",
        "sound1",
        "sound2",
        "sound3",
        "suspicious1",
        "suspicious2",
        "suspicious3",
        "suspicious4",
        "suspicious5",
        "combat1",
        "combat2",
        "combat3",
        "jdetected1",
        "jdetected2",
        "jdetected3",
        "taunt1",
        "taunt2",
        "taunt3",
        "jchase1",
        "jchase2",
        "jchase3",
        "jlost1",
        "jlost2",
        "jlost3",
        "deflect1",
        "deflect2",
        "deflect3",
        "gloat1",
        "gloat2",
        "gloat3",
        "pushfail",
    ];
    event
        .checked_sub(116)
        .and_then(|index| NAMES.get(usize::from(index)))
        .copied()
}

/// Where `CG_CustomSound` finds `name` for an NPC: the list and position, or `None` when
/// no list has it or the duel list takes it first.
fn slot(name: &str, gametype: i32) -> Option<(usize, usize)> {
    let duel = matches!(gametype, GT_DUEL | GT_POWERDUEL);
    let longest = LISTS.iter().map(|list| list.len()).max().unwrap_or(0);
    for position in 0..longest {
        if LISTS[0].get(position) == Some(&name) {
            return Some((0, position));
        }
        if duel && DUEL.get(position) == Some(&name) {
            return None;
        }
        for (list, names) in LISTS.iter().enumerate().skip(1) {
            if names.get(position) == Some(&name) {
                return Some((list, position));
            }
        }
    }
    None
}

/// One sound set's sounds, by list and position.
struct Set {
    name: String,
    sounds: [Vec<Option<u16>>; 4],
}

/// The NPC sound sets the `CS_SOUNDS` table names, loaded once each.
pub(crate) struct NpcVoices {
    sets: Vec<Set>,
    /// The set each `CS_SOUNDS` slot names.
    slots: Box<[Option<u16>; MAX_SOUNDS]>,
    gametype: i32,
}

impl NpcVoices {
    pub(crate) fn new(gametype: i32) -> Self {
        Self {
            sets: Vec::new(),
            slots: Box::new([None; MAX_SOUNDS]),
            gametype,
        }
    }

    /// `CS_SOUNDS` slot `slot` now holds `path`; a `*$<set>` loads that set once, each
    /// name that exists through `intern`.
    pub(crate) fn set_slot(
        &mut self,
        slot: usize,
        path: &str,
        exists: impl Fn(&str) -> bool,
        mut intern: impl FnMut(&str) -> u16,
    ) {
        let Some(entry) = self.slots.get_mut(slot) else {
            return;
        };
        let Some(set) = path.strip_prefix("*$").filter(|set| !set.is_empty()) else {
            *entry = None;
            return;
        };
        if let Some(known) = self
            .sets
            .iter()
            .position(|known| known.name.eq_ignore_ascii_case(set))
        {
            *entry = Some(known as u16);
            return;
        }
        let sounds = LISTS.map(|names| {
            names
                .iter()
                .map(|name| {
                    let base = format!("sound/chars/{set}/misc/{name}");
                    ["wav", "mp3"]
                        .iter()
                        .map(|extension| format!("{base}.{extension}"))
                        .find(|path| exists(path))
                        .map(|path| intern(&path))
                })
                .collect()
        });
        *entry = Some(self.sets.len() as u16);
        self.sets.push(Set {
            name: set.to_owned(),
            sounds,
        });
    }

    /// The sound an NPC whose `csSounds_*` are `slots` makes for `name` (without `*`),
    /// none when its sets lack it.
    pub(crate) fn sound(&self, slots: [u16; 4], name: &str) -> Option<u16> {
        let (list, position) = slot(name, self.gametype)?;
        let set = (*self.slots.get(usize::from(slots[list]))?)?;
        *self.sets.get(usize::from(set))?.sounds[list].get(position)?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_resolve_as_stock_resolves_them() {
        // The standard list's chokes come first; a stormtrooper's detections are extras.
        assert_eq!(slot("choke2", 0), Some((0, 10)));
        assert_eq!(slot("pushed3", 0), Some((1, 11)));
        assert_eq!(slot("detected4", 0), Some((2, 11)));
        assert_eq!(slot("pushfail", 0), Some((3, 21)));
        assert_eq!(slot("anger1", 0), Some((1, 0)));
        // In a duel the duel list takes an NPC's anger and gloats, which it never loads.
        assert_eq!(slot("anger1", GT_DUEL), None);
        assert_eq!(slot("gloat3", GT_POWERDUEL), None);
        assert_eq!(slot("pain50", GT_DUEL), Some((0, 5)));
        assert_eq!(slot("nothing", 0), None);
        assert_eq!(voice_event_name(116), Some("anger1"));
        assert_eq!(voice_event_name(133), Some("chase1"));
        assert_eq!(voice_event_name(146), Some("lost1"));
        assert_eq!(voice_event_name(152), Some("giveup1"));
        assert_eq!(voice_event_name(181), Some("jlost1"));
        assert_eq!(voice_event_name(190), Some("pushfail"));
        assert_eq!(voice_event_name(191), None);
        assert_eq!(voice_event_name(115), None);
    }

    #[test]
    fn a_set_loads_once_and_serves_its_npcs() {
        let mut voices = NpcVoices::new(0);
        let mut interned = Vec::new();
        let exists = |path: &str| path.ends_with("/anger2.mp3") || path.ends_with("/pain25.wav");
        voices.set_slot(7, "*$stormtrooper", exists, |path| {
            interned.push(path.to_owned());
            interned.len() as u16 - 1
        });
        voices.set_slot(9, "*$StormTrooper", |_| true, |_| unreachable!());
        assert_eq!(
            interned,
            [
                "sound/chars/stormtrooper/misc/pain25.wav",
                "sound/chars/stormtrooper/misc/anger2.mp3"
            ]
        );
        assert_eq!(voices.sound([7, 9, 0, 0], "pain25"), Some(0));
        assert_eq!(voices.sound([7, 9, 0, 0], "anger2"), Some(1));
        assert_eq!(voices.sound([7, 9, 0, 0], "anger1"), None);
        // An NPC without a combat set has no anger.
        assert_eq!(voices.sound([7, 0, 0, 0], "anger2"), None);
    }
}
