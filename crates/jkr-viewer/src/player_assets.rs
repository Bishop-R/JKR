//! Loading of deformable player appearances from the mounted VFS.

use super::*;
use std::collections::HashMap;

/// Parsed `.gla` skeletons keyed by animation path. Every humanoid player
/// model shares `_humanoid.gla` (tens of MB), so the menu's stage keeps
/// one parsed copy across model switches instead of re-reading it.
#[derive(Default)]
pub(crate) struct GlaCache {
    animations: HashMap<String, Arc<Gla>>,
    audio: HashMap<(String, String), AnimationAudio>,
}

type AnimationAudio = (
    Arc<jkr_client::animation_events::Events>,
    Arc<crate::audio::SoundPrefetch>,
);

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
    pub(super) sequence: AnimationSequence,
    pub(crate) events: Arc<jkr_client::animation_events::Events>,
    pub(crate) event_sounds: Option<Arc<crate::audio::SoundPrefetch>>,
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
    skeleton_matches(&mesh, &animation, directory, &animation_path)?;
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
    // A world loader shares the immutable sound table and encoded assets across
    // appearances using the same event file/skeleton, just like the GLA itself.
    let event_directory = if vfs.contains(&format!("{directory}/animevents.cfg"))? {
        directory
    } else {
        animation_directory
    };
    let audio_key = (event_directory.to_owned(), animation_directory.to_owned());
    let (events, event_sounds) = cache
        .audio
        .entry(audio_key)
        .or_insert_with(|| {
            let events = Arc::new(jkr_client::animation_events::Events::load(
                vfs,
                directory,
                animation_directory,
                &config,
            ));
            let sounds = Arc::new(crate::audio::SoundPrefetch::paths(vfs, events.paths()));
            (events, sounds)
        })
        .clone();
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
        events,
        event_sounds: Some(event_sounds),
        sequence,
        origin,
        yaw: camera_yaw + std::f32::consts::PI,
        vehicle: vehicle_kind,
    })
}

/// A mesh skinned against a skeleton with a different bone count cannot be
/// posed. rd-vanilla refuses such a model when it loads (`R_LoadMDXM`), and the
/// client then uses its fallback; refusing it here keeps one bad custom model on
/// a server from failing the whole map load at its first skinning instead.
fn skeleton_matches(
    mesh: &Glm,
    animation: &Gla,
    directory: &str,
    animation_path: &str,
) -> Result<(), Box<dyn Error>> {
    if mesh.bone_count == animation.bones.len() {
        return Ok(());
    }
    Err(format!(
        "{directory}/model.glm has {} bones but its skeleton {animation_path} has {}",
        mesh.bone_count,
        animation.bones.len()
    )
    .into())
}

#[cfg(test)]
mod skeleton_tests {
    use super::skeleton_matches;
    use jkr_model::{Gla, GlaBone, Glm};

    fn bones(count: usize) -> Vec<GlaBone> {
        (0..count)
            .map(|index| GlaBone {
                name: format!("bone{index}"),
                flags: 0,
                parent: index.checked_sub(1),
                base_pose: [[0.0; 4]; 3],
                inverse_base_pose: [[0.0; 4]; 3],
                children: Vec::new(),
            })
            .collect()
    }

    fn pair(mesh_bones: usize, skeleton_bones: usize) -> (Glm, Gla) {
        let mesh = Glm {
            name: "model".into(),
            animation_name: "models/players/_humanoid/_humanoid".into(),
            bone_count: mesh_bones,
            hierarchy: Vec::new(),
            lods: Vec::new(),
        };
        let skeleton = Gla {
            name: "_humanoid".into(),
            scale: 1.0,
            bones: bones(skeleton_bones),
            frames: Vec::new(),
            compressed_bones: Vec::new(),
        };
        (mesh, skeleton)
    }

    #[test]
    fn a_mesh_matching_its_skeleton_loads() {
        let (mesh, skeleton) = pair(53, 53);
        assert!(skeleton_matches(&mesh, &skeleton, "models/players/kyle", "a.gla").is_ok());
    }

    #[test]
    fn a_bone_count_mismatch_is_a_load_error() {
        let (mesh, skeleton) = pair(72, 53);
        let error = skeleton_matches(&mesh, &skeleton, "models/players/custom", "a.gla")
            .unwrap_err()
            .to_string();
        assert!(error.contains("72 bones"), "{error}");
        assert!(error.contains("has 53"), "{error}");
    }
}
