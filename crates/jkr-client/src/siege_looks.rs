//! What a siege class does to its players' looks on every client (`CG_NewClientInfo`,
//! `cg_players.c:1705-1740`): in siege, the class a player's string names (`siegeclass`)
//! forces its model and skin over the player's own — `cg_forceModel` included — and the
//! colour of each saber. The classes come from the game data (`ext_data/Siege/Classes`,
//! read by `CG_InitSiegeMode`); whoever holds the files reads them and hands the looks in.

use jkr_game_jka::siege_class::SiegeClass;
use jkr_protocol::GameState;
use jkr_runtime::Appearance;

const CS_SERVERINFO: usize = 0;
const CS_PLAYERS: usize = 1_131;
const GT_SIEGE: i32 = 7;

/// One class's looks: `model` (as the asset path, `models/players/<model>`), `skin`,
/// `sabercolor` and `saber2color` of its file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SiegeClassLook {
    pub model: Option<String>,
    pub skin: Option<String>,
    pub saber_colors: [Option<i32>; 2],
}

impl From<&SiegeClass> for SiegeClassLook {
    fn from(class: &SiegeClass) -> Self {
        Self {
            model: class
                .model
                .as_ref()
                .map(|model| format!("models/players/{model}")),
            skin: class.skin.clone(),
            saber_colors: [class.saber_color, class.saber2_color],
        }
    }
}

/// The classes' looks by name (`BG_SiegeFindClassByName`, case aside).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SiegeClassLooks {
    classes: Vec<(String, SiegeClassLook)>,
}

impl SiegeClassLooks {
    /// The looks of `classes`, each under its class's name.
    pub fn new(classes: &[SiegeClass]) -> Self {
        Self {
            classes: classes
                .iter()
                .map(|class| (class.name.clone(), SiegeClassLook::from(class)))
                .collect(),
        }
    }

    /// The look of the class client `client_num` plays, in a siege game only.
    pub fn of_client(&self, game_state: &GameState, client_num: u16) -> Option<&SiegeClassLook> {
        if self.classes.is_empty() {
            return None;
        }
        let server_info = game_state.config_string(CS_SERVERINFO)?;
        if crate::LegacyClientInfo::new(server_info).integer("g_gametype") != Some(GT_SIEGE) {
            return None;
        }
        let config = game_state.config_string(CS_PLAYERS + usize::from(client_num))?;
        let class = crate::LegacyClientInfo::new(config).text("siegeclass")?;
        self.classes
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(class))
            .map(|(_, look)| look)
    }

    /// `appearance` with the model and skin the class of `client_num` forces.
    pub fn dress(&self, game_state: &GameState, client_num: u16, appearance: &mut Appearance) {
        let Some(look) = self.of_client(game_state, client_num) else {
            return;
        };
        if let Some(model) = &look.model {
            appearance.model.clone_from(model);
        }
        if let Some(skin) = &look.skin {
            appearance.variant.clone_from(skin);
        }
    }

    /// The colour the class of `client_num` forces on saber `saber` (0 or 1), if any.
    pub fn saber_color(
        &self,
        game_state: &GameState,
        client_num: u16,
        saber: usize,
    ) -> Option<i32> {
        self.of_client(game_state, client_num)?
            .saber_colors
            .get(saber)
            .copied()
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(gametype: i32, player: &[u8]) -> GameState {
        let mut game = GameState::empty_local(0);
        let server_info = format!("\\g_gametype\\{gametype}").into_bytes();
        game.replace_config_string(CS_SERVERINFO, server_info)
            .unwrap();
        game.replace_config_string(CS_PLAYERS + 3, player.to_vec())
            .unwrap();
        game
    }

    fn looks() -> SiegeClassLooks {
        SiegeClassLooks {
            classes: vec![(
                "Jedi Demolitionist".to_owned(),
                SiegeClassLook {
                    model: Some("models/players/jedi".to_owned()),
                    skin: Some("j2".to_owned()),
                    saber_colors: [Some(4), None],
                },
            )],
        }
    }

    /// `CG_NewClientInfo`: in siege the class's model and skin and its first saber's colour;
    /// the second saber keeps the player's.
    #[test]
    fn a_siege_class_dresses_its_player() {
        let game = game(
            7,
            b"\\n\\a\\model\\kyle/default\\siegeclass\\jedi demolitionist",
        );
        let mut appearance = Appearance {
            model: "models/players/kyle".to_owned(),
            variant: "default".to_owned(),
        };
        looks().dress(&game, 3, &mut appearance);
        assert_eq!(appearance.model, "models/players/jedi");
        assert_eq!(appearance.variant, "j2");
        assert_eq!(looks().saber_color(&game, 3, 0), Some(4));
        assert_eq!(looks().saber_color(&game, 3, 1), None);
    }

    /// Outside siege the class names nothing.
    #[test]
    fn only_siege_dresses_players() {
        let game = game(
            8,
            b"\\n\\a\\model\\kyle/default\\siegeclass\\Jedi Demolitionist",
        );
        assert!(looks().of_client(&game, 3).is_none());
    }
}
