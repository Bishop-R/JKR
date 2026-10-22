//! jaPRO's player statistics (`pers.stats`, `ps.fd.suicides`) and the table `printStats`
//! shows (`PrintStats`, `g_main.c:2071-2250`): kills and deaths, damage given and taken
//! (capped at what the victim had left), team damage, the team powers given, drain on
//! enemies against drain on teammates, and in CTF the captures, returns and carrier kills.

/// A player's statistics, kept for the match.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// `pers.stats.kills`: kills of players and NPCs (not of teammates nor itself).
    pub kills: i32,
    /// `ps.fd.suicides`.
    pub suicides: i32,
    /// `pers.stats.teamKills`.
    pub team_kills: i32,
    /// `damageGiven`, `damageTaken`, `teamDamageGiven`.
    pub damage_given: i32,
    pub damage_taken: i32,
    pub team_damage_given: i32,
    /// `teamHealGiven`, `teamEnergizeGiven`: what team heal and energize gave, by the
    /// amount each teammate was given.
    pub team_heal_given: i32,
    pub team_energize_given: i32,
    /// `enemyDrainDamage`, `teamDrainDamage`: drain on enemies and on teammates.
    pub enemy_drain: i32,
    pub team_drain: i32,
}

/// A blow's damage as the statistics count it: health and armour together, no more than
/// the victim had (`take + asave`, capped at `health + armor`).
pub fn counted(dealt: i32, health: i32, armor: i32) -> i32 {
    dealt.min(health + armor)
}

/// Which of the table's columns show (`PrintStats`' conditions).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Columns {
    /// Not in CTF: net kills, net damage, damage per death.
    pub net: bool,
    /// Team FFA with friendly fire: team damage.
    pub team_damage: bool,
    /// CTF or capture the ysalamiri: captures, returns, carrier kills.
    pub ctf: bool,
    /// Not on a saber-only server: accuracy.
    pub accuracy: bool,
    /// Team heal and energize allowed, outside the gun game.
    pub team_powers: bool,
    /// Drain allowed with friendly fire, outside the gun game: drain accuracy.
    pub drain: bool,
    /// CTF counts kills and deaths without suicides.
    pub ctf_counting: bool,
}

/// One player's row.
#[derive(Clone, Copy, Debug)]
pub struct Row<'a> {
    /// `PERS_SCORE`, `PERS_KILLED`.
    pub score: i32,
    pub killed: i32,
    pub stats: Stats,
    /// Shots and hits (`accuracy_shots`, `accuracy_hits`).
    pub shots: i32,
    pub hits: i32,
    /// `teamState`: `captures`, `flagrecovery`, `fragcarrier`.
    pub captures: i32,
    pub returns: i32,
    pub carrier_kills: i32,
    /// Minutes since it entered.
    pub minutes: i32,
    pub name: &'a [u8],
}

/// `msg[1024-128]`: a print stays below this many bytes.
const PRINT: usize = 896;

/// The table's prints: the header, then a row per player, yellow and green in turn,
/// each column padded to its label's width.
pub fn table(columns: Columns, rows: &[Row<'_>]) -> Vec<Vec<u8>> {
    let space = "   ";
    let label = |name: &str| format!("{name}{space}");
    let mut labels: Vec<String> = vec![label("Score"), label("Kills"), label("Deaths")];
    if columns.net {
        labels.push(label("Net"));
    }
    labels.push(label("Dmg Given"));
    labels.push(label("Dmg Taken"));
    if columns.net {
        labels.push(label("Net Dmg"));
        labels.push(label("Dmg/Death"));
    }
    if columns.team_damage {
        labels.push(label("Team Dmg"));
    } else if columns.ctf {
        labels.push(label("Caps"));
        labels.push(label("Rets"));
        labels.push(label("Carrier Kills"));
    }
    if columns.accuracy {
        labels.push(label("Acc"));
    }
    if columns.team_powers {
        labels.push(label("T Energize"));
        labels.push(label("T Heal"));
    }
    if columns.drain {
        labels.push(label("Drain Acc"));
    }
    labels.push(label("Time"));
    let name_label = format!("Name{space}\n");
    let mut prints = Vec::new();
    let mut message: Vec<u8> = b"\n^5".to_vec();
    for text in &labels {
        message.extend_from_slice(text.as_bytes());
    }
    message.extend_from_slice(name_label.as_bytes());
    for (index, row) in rows.iter().enumerate() {
        let stats = row.stats;
        let (kills, deaths, net) = if columns.ctf_counting {
            (
                stats.kills,
                row.killed - stats.suicides,
                stats.kills - row.killed + stats.suicides,
            )
        } else {
            let kills = row.score + stats.team_kills + stats.suicides;
            (kills, row.killed, kills - row.killed)
        };
        let per_death = if row.killed != 0 {
            stats.damage_given / row.killed
        } else {
            stats.damage_given
        };
        let accuracy = if row.shots != 0 {
            (100.0 * row.hits as f32 / row.shots as f32) as i32
        } else {
            0
        };
        let drained = stats.enemy_drain + stats.team_drain;
        let drain_ratio = if drained != 0 {
            (100.0 * stats.enemy_drain as f32 / drained as f32) as i32
        } else {
            0
        };
        let mut values: Vec<i32> = vec![row.score, kills, deaths];
        if columns.net {
            values.push(net);
        }
        values.push(stats.damage_given);
        values.push(stats.damage_taken);
        if columns.net {
            values.push(stats.damage_given - stats.damage_taken - stats.team_damage_given);
            values.push(per_death);
        }
        if columns.team_damage {
            values.push(stats.team_damage_given);
        } else if columns.ctf {
            values.extend([row.captures, row.returns, row.carrier_kills]);
        }
        if columns.accuracy {
            values.push(accuracy);
        }
        if columns.team_powers {
            values.push(stats.team_energize_given);
            values.push(stats.team_heal_given);
        }
        if columns.drain {
            values.push(drain_ratio);
        }
        values.push(row.minutes);
        let mut line: Vec<u8> = if index % 2 == 0 { b"^3" } else { b"^2" }.to_vec();
        for (value, text) in values.iter().zip(&labels) {
            line.extend_from_slice(format!("{value:<width$}", width = text.len()).as_bytes());
        }
        line.extend_from_slice(b"^7");
        line.extend_from_slice(row.name);
        let padding = name_label.len().saturating_sub(row.name.len());
        line.extend(std::iter::repeat_n(b' ', padding));
        line.push(b'\n');
        if message.len() + line.len() >= PRINT {
            prints.push(print(&message));
            message.clear();
        }
        message.extend_from_slice(&line);
    }
    prints.push(print(&message));
    prints
}

/// `print "<text>"`.
fn print(text: &[u8]) -> Vec<u8> {
    [&b"print \""[..], text, b"\""].concat()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_lines_up_under_its_labels() {
        let columns = Columns {
            net: true,
            team_damage: false,
            ctf: false,
            accuracy: true,
            team_powers: false,
            drain: false,
            ctf_counting: false,
        };
        let row = Row {
            score: 7,
            killed: 3,
            stats: Stats {
                kills: 7,
                suicides: 1,
                damage_given: 900,
                damage_taken: 400,
                ..Stats::default()
            },
            shots: 40,
            hits: 10,
            captures: 0,
            returns: 0,
            carrier_kills: 0,
            minutes: 5,
            name: b"Kyle",
        };
        let prints = table(columns, &[row]);
        let text = String::from_utf8(prints.concat()).unwrap();
        assert!(text.contains("Score   Kills   Deaths   Net   Dmg Given   Dmg Taken   Net Dmg   Dmg/Death   Acc   Time   Name   \n"));
        // Kills count the suicide back (7 + 0 + 1), net 8 - 3, 900 / 3 a death, 25%.
        assert!(text.contains("^37       8       3        5     900         400         500       300         25    5      ^7Kyle"));
        assert_eq!(counted(120, 30, 25), 55);
        assert_eq!(counted(20, 30, 25), 20);
    }
}
