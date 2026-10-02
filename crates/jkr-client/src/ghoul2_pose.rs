//! BaseJKA Ghoul2 pose policy at the compatibility boundary.
//!
//! Bone names belong here rather than in the engine-generic skeleton
//! evaluator.  The policy mirrors `CG_SetLerpFrameAnimation` in
//! `codemp/cgame/cg_players.c:2938-3016`.

use jkr_model::{
    AnimationConfig, AnimationSequence, BoneAngleCommand, BoneAngleMode, BoneAnimationCommand,
    BoneAxis, BoneOverridePose, Gla, ModelError, OverrideEndBehavior,
};
use jkr_runtime::{AnimationState, AnimationTrackState};

/// Bone names and interpolation policy used by BaseJKA humanoid actors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LegacyGhoul2PosePolicy;

impl LegacyGhoul2PosePolicy {
    /// The legs animation override is installed on this root bone.
    pub const LEGS_ROOT: &'static str = "model_root";

    /// The torso animation override is installed here and inherited by all
    /// descendants through Ghoul2's parent timing propagation.
    pub const TORSO_ROOT: &'static str = "lower_lumbar";

    /// Humanoid cgame explicitly gives this non-torso-subtree bone the torso
    /// track whenever it updates a torso animation.
    pub const TORSO_DIRECT_OVERRIDES: [&'static str; 1] = ["Motion"];

    /// Convert an animation.cfg sequence into signed frames per 50 milliseconds.
    pub fn animation_speed(sequence: &AnimationSequence, speed_milli: u16) -> f32 {
        let fps = sequence.frames_per_second;
        let frame_lerp_millis = if fps.is_sign_negative() {
            (1_000.0 / fps).floor()
        } else {
            (1_000.0 / fps).ceil()
        };
        (50.0 / frame_lerp_millis) * f32::from(speed_milli) / 1_000.0
    }
}

/// The bones only humanoid skeletons carry (`cg_players.c:7246-7271`).
#[derive(Clone, Copy, Debug)]
struct HumanoidBones {
    motion: usize,
    angle_bones: [usize; 5],
}

/// Per-actor BaseJKA controller translating cgame track changes into
/// engine-generic captured-frame bone overrides.
#[derive(Clone, Debug)]
pub struct LegacyGhoul2Animator {
    pose: BoneOverridePose,
    legs_root: usize,
    /// `None` is cgame's `noLumbar` (`cg_players.c:7277-7280`): the torso
    /// track is never installed.
    torso_root: Option<usize>,
    /// Humanoid-only spine bones (`localAnimIndex <= 1` in cgame). `None`
    /// for other skeletons: no `Motion` override, no player-angle commands.
    humanoid: Option<HumanoidBones>,
    applied_lower: Option<AnimationTrackState>,
    applied_upper: Option<AnimationTrackState>,
    /// The override installed for `applied_lower`, kept so the legs frame
    /// can be queried without touching the evaluated pose.
    lower_command: Option<BoneAnimationCommand>,
    /// The override installed for `applied_upper`, kept so the torso frame
    /// can be queried without touching the evaluated pose.
    upper_command: Option<BoneAnimationCommand>,
}

impl LegacyGhoul2Animator {
    /// Resolve cgame's override bones and allocate fixed state.
    ///
    /// Only the legs root is required. A skeleton without `lower_lumbar` is
    /// cgame's `noLumbar` case; one without the five spine bones and
    /// `Motion` is a non-humanoid (`localAnimIndex > 1`, e.g. the rancor),
    /// which cgame animates on `model_root` alone and never hands to
    /// `CG_G2PlayerAngles` (`cg_players.c:4274`).
    pub fn new(animation: &Gla) -> Result<Self, ModelError> {
        let find = |name: &str| {
            animation
                .bones
                .iter()
                .position(|bone| bone.name.eq_ignore_ascii_case(name))
        };
        let legs_root = find(LegacyGhoul2PosePolicy::LEGS_ROOT)
            .ok_or_else(|| ModelError::invalid(0, "legs root bone is missing"))?;
        let torso_root = find(LegacyGhoul2PosePolicy::TORSO_ROOT);
        let humanoid = (|| {
            Some(HumanoidBones {
                motion: find(LegacyGhoul2PosePolicy::TORSO_DIRECT_OVERRIDES[0])?,
                angle_bones: [
                    torso_root?,
                    find("upper_lumbar")?,
                    find("thoracic")?,
                    find("cervical")?,
                    find("cranium")?,
                ],
            })
        })();
        Ok(Self {
            pose: BoneOverridePose::new(animation.bones.len()),
            legs_root,
            torso_root,
            humanoid,
            applied_lower: None,
            applied_upper: None,
            lower_command: None,
            upper_command: None,
        })
    }

    /// `true` when the skeleton carries the humanoid spine cgame drives with
    /// player angles; non-humanoids take their yaw straight from the entity.
    pub fn humanoid(&self) -> bool {
        self.humanoid.is_some()
    }

    /// Install cgame's five ordinary-player POSTMULT angle commands.
    pub fn set_player_angles(
        &mut self,
        animation: &Gla,
        angles: crate::LegacyPlayerAngleSample,
    ) -> Result<(), ModelError> {
        let Some(humanoid) = self.humanoid else {
            return Ok(());
        };
        if !angles.bone_angles_active {
            self.clear_player_angles();
            return Ok(());
        }
        for (bone, values) in humanoid.angle_bones.into_iter().zip([
            angles.lower_lumbar,
            angles.upper_lumbar,
            angles.thoracic,
            angles.cervical,
            angles.cranium,
        ]) {
            self.pose.set_bone_angles(
                animation,
                bone,
                BoneAngleCommand {
                    angles_degrees: values,
                    mode: BoneAngleMode::PostMultiply,
                    up: BoneAxis::PositiveX,
                    left: BoneAxis::NegativeY,
                    forward: BoneAxis::NegativeZ,
                },
            )?;
        }
        Ok(())
    }

    /// `CG_G2ServerBoneAngles` (`cg_players.c:3960-4011`): one bone a server turns for
    /// the clients, `G2API_SetBoneAngles` with `BONE_ANGLES_POSTMULT` about the axes
    /// `orient` packs (forward in bits 0-2, right in 3-5, up in 6-8, as `Eorientations`).
    /// The engine blends such a turn in over 100 ms; it is installed at once here.
    pub fn set_server_bone_angles(
        &mut self,
        animation: &Gla,
        bone: usize,
        angles: [f32; 3],
        orient: u32,
    ) -> Result<(), ModelError> {
        let axis = |bits: u32| match bits & 7 {
            1 => Some(BoneAxis::PositiveX),
            2 => Some(BoneAxis::PositiveZ),
            3 => Some(BoneAxis::PositiveY),
            4 => Some(BoneAxis::NegativeX),
            5 => Some(BoneAxis::NegativeZ),
            6 => Some(BoneAxis::NegativeY),
            _ => None,
        };
        // An axis of `ORIGIN` (or beyond the enum) names no turn a skeleton can make.
        let (Some(up), Some(left), Some(forward)) =
            (axis(orient >> 6), axis(orient >> 3), axis(orient))
        else {
            return Ok(());
        };
        let command = BoneAngleCommand {
            angles_degrees: angles,
            mode: BoneAngleMode::PostMultiply,
            up,
            left,
            forward,
        };
        self.pose.set_bone_angles(animation, bone, command)
    }

    /// Clear all player-angle commands for root-locked/ragdoll policy paths.
    pub fn clear_player_angles(&mut self) {
        let Some(humanoid) = self.humanoid else {
            return;
        };
        for bone in humanoid.angle_bones {
            self.pose.clear_bone_angles(bone);
        }
    }

    /// Apply SetBoneAnim changes and evaluate the inherited pose at `time_millis`.
    pub fn evaluate<'a>(
        &'a mut self,
        animation: &Gla,
        config: &AnimationConfig,
        state: AnimationState,
        time_millis: i64,
    ) -> Result<&'a mut [[[f32; 4]; 3]], ModelError> {
        if (self.applied_lower.is_none() || time_millis >= state.lower.started_at_millis)
            && self.applied_lower != Some(state.lower)
        {
            let command = command_for_track(animation, config, state.lower, self.applied_lower)?;
            self.pose
                .set_bone_animation(animation, self.legs_root, command)?;
            self.applied_lower = Some(state.lower);
            self.lower_command = Some(command);
        }
        if let Some(torso_root) = self.torso_root
            && (self.applied_upper.is_none() || time_millis >= state.upper.started_at_millis)
            && self.applied_upper != Some(state.upper)
        {
            let command = command_for_track(animation, config, state.upper, self.applied_upper)?;
            self.pose
                .set_bone_animation(animation, torso_root, command)?;
            if let Some(humanoid) = self.humanoid {
                self.pose
                    .set_bone_animation(animation, humanoid.motion, command)?;
            }
            self.applied_upper = Some(state.upper);
            self.upper_command = Some(command);
        }
        self.pose.evaluate(animation, time_millis)
    }

    /// Query the existing Motion override before the frame's new animation commands.
    /// cg_players.c:9217,9399 calls angles before CG_PlayerAnimation; ci clips are the
    /// previous installed tracks. cl_cgameapi.cpp:501-506 actually reconstructs, but
    /// skips the multiplayer 90-degree bolt rotation. Only the joint ancestor chain
    /// is needed here (tr_ghoul2.cpp:3125-3131; G2_API.cpp:2059-2063 row normalization).
    pub fn player_motion_angles(
        &mut self,
        animation: &Gla,
        time_millis: i64,
    ) -> Result<Option<[f32; 3]>, ModelError> {
        let (Some(lower), Some(upper)) = (self.applied_lower, self.applied_upper) else {
            return Ok(None);
        };
        if !crate::player_angle_rules::correction_tracks(lower.clip, upper.clip) {
            return Ok(None);
        }
        let Some(humanoid) = self.humanoid else {
            return Ok(None);
        };
        let matrix = self
            .pose
            .sample_joint(animation, humanoid.motion, time_millis)?;
        // tr_ghoul2.cpp:130-137,3165-3197,3410-3414: Ghoul2's root identity
        // is a +90-degree Z basis. The generic evaluator intentionally omits it.
        // This is distinct from the API bolt-column swap disabled by NoRecNoRot.
        let matrix = [matrix[1].map(|v| -v), matrix[0], matrix[2]];
        Ok(Some(crate::player_angles::motion_angles(matrix)))
    }

    /// Fractional frame of the torso override at `time_millis`, as
    /// `G2API_GetBoneFrame(lower_lumbar)` reports it (`G2_bones.cpp:899-901`:
    /// `float(currentFrame) + lerp`). `None` before the first torso override.
    pub fn torso_frame(&self, animation: &Gla, time_millis: i64) -> Option<f32> {
        command_frame(animation, self.upper_command?, time_millis)
    }

    /// The legs (`model_root`) and torso (`lower_lumbar`) tracks at
    /// `time_millis`: each installed clip with its fractional frame, as
    /// `CG_TriggerAnimSounds` reads them through `G2API_GetBoneFrame`
    /// (`cg_players.c:3048-3090`). The torso is `None` for a `noLumbar`
    /// skeleton, and either is `None` before its first override.
    pub fn track_frames(&self, animation: &Gla, time_millis: i64) -> [Option<(usize, f32)>; 2] {
        [self.lower_command, self.upper_command].map(|command| {
            let command = command?;
            Some((
                command.clip,
                command_frame(animation, command, time_millis)?,
            ))
        })
    }

    /// Addresses of fixed override/evaluation storage for allocation gates.
    pub fn storage_addresses(&self) -> [usize; 6] {
        self.pose.storage_addresses()
    }

    /// Composed skinning matrices retained by the most recent successful evaluation.
    pub fn matrices(&self) -> &[[[f32; 4]; 3]] {
        self.pose.matrices()
    }

    /// Local matrices from the most recent pose evaluation.
    pub fn local_matrices(&self) -> &[[[f32; 4]; 3]] {
        self.pose.local_matrices()
    }
}

/// `float(currentFrame) + lerp` of an installed override at `time_millis`.
fn command_frame(animation: &Gla, command: BoneAnimationCommand, time_millis: i64) -> Option<f32> {
    let sample = BoneOverridePose::sample_command(animation, command, time_millis).ok()?;
    Some(sample.current_frame as f32 + sample.fraction)
}

fn command_for_track(
    animation: &Gla,
    config: &AnimationConfig,
    track: AnimationTrackState,
    previous: Option<AnimationTrackState>,
) -> Result<BoneAnimationCommand, ModelError> {
    if let Some(frame) = track.forced_frame {
        return Ok(BoneAnimationCommand {
            clip: track.clip,
            start_frame: frame as i32,
            end_frame: frame.saturating_add(1) as i32,
            speed: 1.0,
            time_millis: track.started_at_millis,
            set_frame: None,
            end_behavior: OverrideEndBehavior::Freeze,
            blend: previous.is_some(),
            blend_millis: 150,
        });
    }
    let sequence = crate::legacy_animation_name(track.clip)
        .and_then(|name| config.get(name))
        .or_else(|| config.get("BOTH_STAND1"))
        // A machine's table is short (a swoop has four sequences) and a custom NPC's may
        // lack what the server asks for. Its rest pose is the honest fallback; failing here
        // stopped the animation of every actor in the scene, not just this one.
        .or_else(|| config.get("ROOT"))
        .or_else(|| config.get_by_index(0))
        .ok_or_else(|| {
            ModelError::invalid(track.clip, "animation config has no sequence at all")
        })?;
    let speed = LegacyGhoul2PosePolicy::animation_speed(sequence, track.speed_milli);
    let (start_frame, end_frame) = if speed < 0.0 {
        (
            (sequence.first_frame + sequence.frame_count) as i32,
            sequence.first_frame as i32,
        )
    } else {
        (
            sequence.first_frame as i32,
            (sequence.first_frame + sequence.frame_count) as i32,
        )
    };
    let blend_millis = track.transition.map_or_else(
        || {
            crate::animation_selection::legacy_blend_millis(
                track.clip,
                previous.map(|old| old.clip),
                false,
            )
        },
        |transition| transition.blend_duration_millis,
    );
    let mut command = BoneAnimationCommand {
        clip: track.clip,
        start_frame,
        end_frame,
        speed,
        time_millis: track.started_at_millis,
        set_frame: None,
        end_behavior: if sequence.loop_frame != -1 {
            OverrideEndBehavior::Loop
        } else {
            OverrideEndBehavior::Freeze
        },
        blend: previous.is_some() && blend_millis != 0,
        blend_millis,
    };
    if track.phase_millis != 0 {
        let phase_command = BoneAnimationCommand {
            speed: LegacyGhoul2PosePolicy::animation_speed(sequence, 1_000),
            time_millis: 0,
            set_frame: None,
            blend: false,
            ..command
        };
        let phase =
            BoneOverridePose::sample_command(animation, phase_command, track.phase_millis.max(0))?;
        command.set_frame = Some(phase.current_frame as f32 + phase.fraction);
    }
    Ok(command)
}
