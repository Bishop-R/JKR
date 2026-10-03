//! Loading of deformable player appearances from the mounted VFS.

use super::*;
use jkr_client::LegacyAnimationEvents;
use std::collections::HashMap;

/// Parsed `.gla` skeletons keyed by animation path. Every humanoid player
/// model shares `_humanoid.gla` (tens of MB), so the menu's stage keeps
/// one parsed copy across model switches instead of re-reading it. The
/// skeleton's `animevents.cfg` table is kept the same way.
#[derive(Default)]
pub(crate) struct GlaCache {
    animations: HashMap<String, Arc<Gla>>,
    /// By `Glm::animation_name`; `None` for a skeleton without a file.
    animation_events: HashMap<String, Option<Arc<LegacyAnimationEvents>>>,
}

impl GlaCache {
    fn get(
        &mut self,
        path: &str,
        bytes: impl FnOnce() -> Result<Vec<u8>, Box<dyn Error>>,
    ) -> Result<Arc<Gla>, Box<dyn Error>> {
        if let Some(animation) = self.animations.get(path) {
            return Ok(animation.clone());
        }
        let animation = Arc::new(Gla::parse(&bytes()?)?);
        self.animations.insert(path.to_owned(), animation.clone());
        Ok(animation)
    }

    fn animation_events(
        &mut self,
        vfs: &VirtualFileSystem,
        skeleton: &str,
        config: &AnimationConfig,
    ) -> Option<Arc<LegacyAnimationEvents>> {
        if let Some(table) = self.animation_events.get(skeleton) {
            return table.clone();
        }
        let table = read_animation_events(vfs, skeleton, config).map(Arc::new);
        self.animation_events
            .insert(skeleton.to_owned(), table.clone());
        table
    }
}

/// Read `skeleton`'s `animevents.cfg` (and its includes) against `config`.
/// The file is the one beside the skeleton (`models/players/_humanoid/` for
/// every humanoid), as `CG_G2EvIndexForModel` picks it
/// (`codemp/cgame/cg_players.c`). Read with the model, so drawing an actor
/// never touches the file system for its events.
fn read_animation_events(
    vfs: &VirtualFileSystem,
    skeleton: &str,
    config: &AnimationConfig,
) -> Option<LegacyAnimationEvents> {
    let directory = skeleton
        .rsplit_once('/')
        .map_or("", |(directory, _)| directory);
    let read = |path: &str| {
        let bytes = vfs.read(path).ok().flatten()?.bytes;
        Some(String::from_utf8_lossy(&bytes).into_owned())
    };
    let text = read(&format!("{directory}/animevents.cfg"))?;
    Some(LegacyAnimationEvents::parse(&text, config, &mut |name| {
        read(&format!("models/players/{name}/animevents.cfg"))
    }))
}

/// Split a legacy `model` cvar (`kyle/default`, `jedi_hm/head_a1|torso_a1|lower_a1`)
/// into the model directory under `models/players` and the skin variant.
pub(crate) fn split_model_cvar(value: &str) -> (String, &str) {
    let (directory, variant) = value.split_once('/').unwrap_or((value, "default"));
    let variant = if variant.is_empty() {
        "default"
    } else {
        variant
    };
    (format!("models/players/{directory}"), variant)
}

#[derive(Clone)]
pub(super) struct PlayerPreview {
    pub(super) mesh: Glm,
    pub(super) animation: Arc<Gla>,
    pub(super) skin: Skin,
    pub(super) config: Arc<AnimationConfig>,
    /// The skeleton's `animevents.cfg`, parsed at load; `None` without one.
    pub(super) animation_events: Option<Arc<LegacyAnimationEvents>>,
    pub(super) sequence: AnimationSequence,
    pub(super) origin: [f32; 3],
    pub(super) yaw: f32,
    /// Set when the appearance named a vehicle (`$<vehicle>`).
    pub(super) vehicle: Option<crate::vehicle_assets::VehicleKind>,
}

pub(super) fn load_player_preview(
    vfs: &VirtualFileSystem,
    directory: &str,
    camera_origin: [f32; 3],
    camera_yaw: f32,
) -> Result<PlayerPreview, Box<dyn Error>> {
    load_player_appearance(vfs, directory, "default", camera_origin, camera_yaw)
}

pub(super) fn load_player_appearance(
    vfs: &VirtualFileSystem,
    directory: &str,
    variant: &str,
    camera_origin: [f32; 3],
    camera_yaw: f32,
) -> Result<PlayerPreview, Box<dyn Error>> {
    let mut cache = GlaCache::default();
    load_player_appearance_with(
        vfs,
        directory,
        variant,
        camera_origin,
        camera_yaw,
        &mut cache,
    )
}

/// [`load_player_appearance`] sharing parsed skeletons through `cache`.
pub(super) fn load_player_appearance_with(
    vfs: &VirtualFileSystem,
    directory: &str,
    variant: &str,
    camera_origin: [f32; 3],
    camera_yaw: f32,
    cache: &mut GlaCache,
) -> Result<PlayerPreview, Box<dyn Error>> {
    // A vehicle names itself, not a model (`$<vehicle>`): the vehicle table knows what it wears.
    let vehicle = match jkr_client::legacy_vehicle_name(directory) {
        Some(name) => Some(
            crate::vehicle_assets::look(vfs, name)
                .ok_or_else(|| format!("no mounted .veh file defines vehicle {name:?}"))?,
        ),
        None => None,
    };
    let vehicle_kind = vehicle.as_ref().map(|look| look.kind);
    let (directory, variant) = vehicle.as_ref().map_or((directory, variant), |look| {
        (look.directory.as_str(), look.variant.as_str())
    });
    let directory = directory.trim_end_matches('/');
    let read = |path: &str| -> Result<Vec<u8>, Box<dyn Error>> {
        Ok(vfs
            .read(path)?
            .ok_or_else(|| format!("player preview asset {path:?} was not found"))?
            .bytes)
    };
    let mesh = Glm::parse(&read(&format!("{directory}/model.glm"))?)?;
    let animation_path = format!("{}.gla", mesh.animation_name);
    let animation = cache.get(&animation_path, || read(&animation_path))?;
    // CG_G2AnimEntModelLoad accepts skin handle 0. Machines such as the retail
    // sentry have no default .skin and use the GLM's embedded surface materials.
    // Named/multipart skins still follow the existing error/fallback policy.
    let skin = if variant == "default"
        && !vfs.contains(&format!("{directory}/model_default.skin"))?
    {
        Skin::default()
    } else if variant.contains('|') {
        let parts = variant.split('|').collect::<Vec<_>>();
        if parts.len() != 3 || parts.iter().any(|part| part.is_empty()) {
            return Err("multipart player skin must contain head, torso and lower variants".into());
        }
        let mut combined = Skin::default();
        for part in parts {
            combined.merge(Skin::parse(&read(&format!("{directory}/{part}.skin"))?)?);
        }
        combined
    } else {
        let requested = read(&format!("{directory}/model_{variant}.skin"));
        // BG_ValidateSkinForTeam (bg_misc.c:2687-2770) tries a custom
        // team suffix first, then the ordinary team skin if it is absent.
        let bytes = requested.or_else(|error| {
            let team = variant
                .strip_suffix("_red")
                .map(|_| "red")
                .or_else(|| variant.strip_suffix("_blue").map(|_| "blue"));
            match team {
                Some(team) => read(&format!("{directory}/model_{team}.skin")),
                None => Err(error),
            }
        })?;
        Skin::parse(&bytes)?
    };
    let animation_directory = mesh
        .animation_name
        .rsplit_once('/')
        .map_or("", |(directory, _)| directory);
    let config = AnimationConfig::parse(&read(&format!("{animation_directory}/animation.cfg"))?)?;
    let animation_events = cache.animation_events(vfs, &mesh.animation_name, &config);
    let sequence = config
        .get("BOTH_STAND1IDLE1")
        .or_else(|| config.get("BOTH_STAND1"))
        // Vehicles and other machines have no standing animation: a swoop's whole table is
        // an attack, two cinematics and its root pose. Any frame of theirs is a rest pose.
        .or_else(|| config.get("ROOT"))
        .or_else(|| config.get_by_index(0))
        .ok_or("player animation config has no sequence at all")?
        .clone();
    let forward = [camera_yaw.cos(), camera_yaw.sin()];
    let origin = [
        camera_origin[0] + forward[0] * 192.0,
        camera_origin[1] + forward[1] * 192.0,
        camera_origin[2] - 32.0,
    ];
    crate::log::progress(format_args!(
        concat!(
            "loaded player appearance {}/{}: {} mesh surfaces, ",
            "animation {} frames {}..{}"
        ),
        directory,
        variant,
        mesh.hierarchy.len(),
        sequence.name,
        sequence.first_frame,
        sequence.first_frame + sequence.frame_count
    ));
    Ok(PlayerPreview {
        mesh,
        animation,
        skin,
        config: Arc::new(config),
        animation_events,
        sequence,
        origin,
        yaw: camera_yaw + std::f32::consts::PI,
        vehicle: vehicle_kind,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = "BOTH_RUN1 10 20 0 20\n";

    #[test]
    fn animation_events_are_read_beside_the_skeleton_once() {
        let mut vfs = VirtualFileSystem::new();
        vfs.mount_memory(
            "test",
            [(
                "models/players/_humanoid/animevents.cfg",
                "UPPEREVENTS {\nBOTH_RUN1 AEV_SOUND 4 sound/player/roll1.wav 0 0 0\n}\n",
            )],
        )
        .unwrap();
        let config = AnimationConfig::parse(CONFIG.as_bytes()).unwrap();
        let mut cache = GlaCache::default();
        let skeleton = "models/players/_humanoid/_humanoid";
        let table = cache.animation_events(&vfs, skeleton, &config).unwrap();
        let mut paths = Vec::new();
        table.for_each_sound_path(|path| paths.push(path.to_owned()));
        assert_eq!(paths, ["sound/player/roll1.wav"]);
        let again = cache.animation_events(&vfs, skeleton, &config).unwrap();
        assert!(Arc::ptr_eq(&table, &again));
        assert!(
            cache
                .animation_events(&vfs, "models/players/rancor/rancor", &config)
                .is_none()
        );
    }
}
