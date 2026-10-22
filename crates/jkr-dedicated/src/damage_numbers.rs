//! jaPRO's damage numbers (`g_damageNumbers`, `g_combat.c:5793-5845`): what one player's
//! blow did to another, health and armour together, told as the mode says. 1: a console
//! line; 2: the attacker's running total in the centre; 3: a score plum over the target;
//! 4: a chat line; 5: the line and the total; 6: the line and the plum; 7: the total only
//! kept; 8: a console line to everyone. Spectators following the attacker see its total in
//! the centre, in every mode but the plums'.

/// A player's running total of damage given (`totalDamage`, `lastDamageTime`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Total {
    total: i32,
    last: i32,
}

impl Total {
    /// A blow of `dealt` at `now`: the total begins again after three seconds without one.
    pub fn add(&mut self, dealt: i32, now: i32) {
        if self.last < now - 3_000 {
            self.total = dealt;
        } else {
            self.total += dealt;
        }
        self.last = now;
    }

    pub fn get(self) -> i32 {
        self.total
    }
}

/// What one blow's numbers come to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Told {
    /// Server commands for the attacker.
    pub attacker: Vec<Vec<u8>>,
    /// A server command for everyone.
    pub everyone: Option<Vec<u8>>,
    /// A score plum over the target, showing `dealt`.
    pub plum: bool,
    /// The centre print for the attacker's followers.
    pub followers: Option<Vec<u8>>,
}

/// Whether the mode keeps a running total.
pub fn keeps_total(mode: i32) -> bool {
    matches!(mode, 2 | 5 | 7)
}

/// The numbers for a blow of `dealt` in mode `mode`, the attacker's total `total` (as kept
/// so far), on `target` by `attacker` (their names as shown); `npc_target` when the target
/// is an NPC, whose mode-4 line jaPRO prints rather than sends as chat.
pub fn told(
    mode: i32,
    dealt: i32,
    total: i32,
    target: &[u8],
    attacker: &[u8],
    npc_target: bool,
) -> Told {
    let line = |prefix: &[u8], end: &[u8]| {
        [
            prefix,
            format!("^3{dealt} ^7damage given to (").as_bytes(),
            target,
            b"^7)",
            end,
        ]
        .concat()
    };
    let centre = format!("cp \"{total}{}\"", "\n".repeat(12)).into_bytes();
    let mut told = Told::default();
    if matches!(mode, 1 | 5 | 6) {
        told.attacker.push(line(b"print \"", b"\n\""));
    }
    if mode == 8 {
        told.everyone = Some(line(
            &[b"print \"^3".as_slice(), attacker, b" ^7did "].concat(),
            b"\n\"",
        ));
    }
    if matches!(mode, 2 | 5) {
        told.attacker.push(centre.clone());
    }
    if mode == 4 && npc_target {
        told.attacker.push(line(b"print \"", b"\n\""));
    } else if mode == 4 {
        told.attacker.push(line(b"chat \"", b"\""));
    }
    told.plum = matches!(mode, 3 | 6);
    if mode != 0 && !told.plum {
        told.followers = Some(centre);
    }
    told
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A total begins again after three quiet seconds; each mode tells what jaPRO's does.
    #[test]
    fn blows_are_told_as_the_mode_says() {
        let mut total = Total::default();
        total.add(30, 10_000);
        total.add(12, 12_000);
        assert_eq!(total.get(), 42);
        total.add(5, 15_001);
        assert_eq!(total.get(), 5);
        let one = told(1, 30, 0, b"Tavion", b"Kyle", false);
        assert_eq!(
            one.attacker,
            [b"print \"^330 ^7damage given to (Tavion^7)\n\"".to_vec()]
        );
        assert!(one.everyone.is_none() && !one.plum);
        assert_eq!(
            one.followers,
            Some(format!("cp \"0{}\"", "\n".repeat(12)).into_bytes())
        );
        let five = told(5, 30, 42, b"Tavion", b"Kyle", false);
        assert_eq!(five.attacker.len(), 2);
        assert_eq!(
            five.attacker[1],
            format!("cp \"42{}\"", "\n".repeat(12)).into_bytes()
        );
        assert_eq!(
            told(4, 7, 0, b"Tavion", b"Kyle", false).attacker,
            [b"chat \"^37 ^7damage given to (Tavion^7)\"".to_vec()]
        );
        assert_eq!(
            told(4, 7, 0, b"reborn", b"Kyle", true).attacker,
            [b"print \"^37 ^7damage given to (reborn^7)\n\"".to_vec()]
        );
        assert_eq!(
            told(8, 7, 0, b"Tavion", b"Kyle", false).everyone,
            Some(b"print \"^3Kyle ^7did ^37 ^7damage given to (Tavion^7)\n\"".to_vec())
        );
        let six = told(6, 7, 0, b"Tavion", b"Kyle", false);
        assert!(six.plum && six.followers.is_none() && six.attacker.len() == 1);
        assert_eq!(told(0, 7, 0, b"Tavion", b"Kyle", false), Told::default());
    }
}
