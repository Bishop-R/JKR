//! Teammates' health and ammunition a Siege stat viewer is sent (`sxd`,
//! `CG_ParseSiegeExtendedData`, `cg_saga.c:991-1086`), for the bars `CG_DrawSiegeInfo`
//! (`cg_draw.c:7533-7660`) draws under the crosshair.

use crate::team_info::legacy_atoi;
use jkr_game_jka::npc_combat::AMMO_MAXIMA;
use jkr_game_jka::siege_class::EF_DOUBLE_AMMO;
use jkr_game_jka::weapon_data::legacy_weapon_data;

const MAX_CLIENTS: usize = 32;
/// `EF_DEAD`.
const EF_DEAD: u32 = 1 << 1;
/// A report this old no longer shows.
const STALE_MS: i32 = 10_000;

/// One client's latest report (`siegeExtended_t`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Report {
    health: i32,
    max_health: i32,
    ammo: i32,
    /// The weapon the ammunition counts for; -1 when it was out of that weapon's range.
    weapon: i32,
    updated_at: i32,
}

/// The latest report on each client.
#[derive(Clone, Debug, Default)]
pub struct SiegeExtended {
    reports: [Report; MAX_CLIENTS],
}

impl SiegeExtended {
    /// `sxd`: one `client|health|max|ammo` report per argument, received at `time` while
    /// each client's entity holds the weapon and `eFlags` that `holding` gives.
    pub fn apply(&mut self, arguments: &[Vec<u8>], time: i32, holding: impl Fn(u16) -> (u8, u32)) {
        for entry in arguments.iter().skip(1).filter(|entry| !entry.is_empty()) {
            // Up to four `|`-separated fields read with `atoi`; missing ones keep 1.
            let mut fields = [-1, 1, 1, 1];
            for (slot, field) in fields.iter_mut().zip(entry.split(|&byte| byte == b'|')) {
                *slot = legacy_atoi(field);
            }
            let [client, health, max_health, ammo] = fields;
            let Some(report) = usize::try_from(client)
                .ok()
                .and_then(|client| self.reports.get_mut(client))
            else {
                continue;
            };
            let (weapon, flags) = holding(client as u16);
            // The ammunition only counts for the weapon held while it is in range.
            let valid = (0..=ammo_maximum(weapon, flags)).contains(&ammo);
            *report = Report {
                health,
                max_health,
                ammo,
                weapon: if valid { i32::from(weapon) } else { -1 },
                updated_at: time,
            };
        }
    }

    /// The fractions `CG_DrawSiegeInfo` fills for `client` under the crosshair, from 0 to
    /// 1: its health of its maximum, and its ammunition of its weapon's maximum (whole for
    /// a weapon that uses none). `None` for the dead, a report for another weapon than the
    /// one it holds, and a report more than ten seconds old. JKR dates a report by the
    /// snapshot it came in, a moment ahead of the presented time, so only one dated well
    /// ahead (an earlier level's) counts as from the future.
    pub fn bars(&self, client: u16, weapon: u8, flags: u32, time: i32) -> Option<[f32; 2]> {
        let report = self.reports.get(usize::from(client))?;
        let age = time.wrapping_sub(report.updated_at);
        if !(-STALE_MS..=STALE_MS).contains(&age)
            || flags & EF_DEAD != 0
            || report.weapon != i32::from(weapon)
        {
            return None;
        }
        let fraction =
            |value: i32, maximum: i32| (value as f32 / maximum.max(1) as f32).clamp(0.0, 1.0);
        let costs = legacy_weapon_data(weapon)
            .is_some_and(|data| data.primary_cost != 0 || data.alternate_cost != 0);
        let ammo = if costs {
            fraction(report.ammo, ammo_maximum(weapon, flags))
        } else {
            1.0
        };
        Some([fraction(report.health, report.max_health), ammo])
    }
}

/// `ammoData[weaponData[weapon].ammoIndex].max`, doubled with `EF_DOUBLE_AMMO`.
fn ammo_maximum(weapon: u8, flags: u32) -> i32 {
    let maximum = legacy_weapon_data(weapon)
        .and_then(|data| AMMO_MAXIMA.get(data.ammo_index).copied())
        .unwrap_or(0);
    if flags & EF_DOUBLE_AMMO != 0 {
        maximum * 2
    } else {
        maximum
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WP_BLASTER: u8 = 5;
    const WP_SABER: u8 = 3;

    fn sxd(entries: &[&str]) -> Vec<Vec<u8>> {
        std::iter::once("sxd")
            .chain(entries.iter().copied())
            .map(|entry| entry.as_bytes().to_vec())
            .collect()
    }

    #[test]
    fn a_report_shows_health_and_ammunition_for_ten_seconds() {
        let mut data = SiegeExtended::default();
        data.apply(&sxd(&["3|50|100|150"]), 1_000, |_| (WP_BLASTER, 0));
        assert_eq!(data.bars(3, WP_BLASTER, 0, 2_000), Some([0.5, 0.5]));
        assert_eq!(data.bars(3, WP_BLASTER, 0, 11_000), Some([0.5, 0.5]));
        assert_eq!(data.bars(3, WP_BLASTER, 0, 11_001), None);
        assert_eq!(data.bars(4, WP_BLASTER, 0, 2_000), None);
    }

    #[test]
    fn a_report_counts_only_for_the_weapon_it_was_made_for() {
        let mut data = SiegeExtended::default();
        data.apply(&sxd(&["3|50|100|150"]), 1_000, |_| (WP_BLASTER, 0));
        assert_eq!(data.bars(3, WP_SABER, 0, 2_000), None);
        assert_eq!(data.bars(3, WP_BLASTER, EF_DEAD, 2_000), None);
        // More ammunition than the weapon holds: the report is kept, the bars are not.
        data.apply(&sxd(&["3|50|100|450"]), 1_000, |_| (WP_BLASTER, 0));
        assert_eq!(data.bars(3, WP_BLASTER, 0, 2_000), None);
        // Unless the class doubles it.
        data.apply(&sxd(&["3|50|100|450"]), 1_000, |_| {
            (WP_BLASTER, EF_DOUBLE_AMMO)
        });
        assert_eq!(
            data.bars(3, WP_BLASTER, EF_DOUBLE_AMMO, 2_000),
            Some([0.5, 0.75])
        );
    }

    #[test]
    fn a_weapon_without_ammunition_shows_a_full_bar() {
        let mut data = SiegeExtended::default();
        data.apply(&sxd(&["1|100|100|0", "40|1|1|1", "x"]), 0, |_| {
            (WP_SABER, 0)
        });
        assert_eq!(data.bars(1, WP_SABER, 0, 0), Some([1.0, 1.0]));
    }
}
