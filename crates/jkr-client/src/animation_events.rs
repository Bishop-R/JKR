//! Sounds keyed to animation frames: a skeleton's `animevents.cfg` and the
//! cgame pass that plays them as a player's legs and torso reach their frames.
//!
//! Parsing follows `BG_ParseAnimationEvtFile`/`ParseAnimationEvtBlock`
//! (`codemp/game/bg_panimate.c:1782-2290`): an `UPPEREVENTS` block for the
//! torso and a `LOWEREVENTS` block for the legs, frames given as offsets into
//! the named animation of the skeleton's `animation.cfg`, a later line on the
//! same frame and event type replacing the earlier one. Playback follows
//! `CG_TriggerAnimSounds`/`CG_PlayerAnimEvents`/`CG_PlayerAnimEventDo`
//! (`codemp/cgame/cg_players.c:2609-3090`).
//!
//! Only the sound events are kept: `AEV_SOUND`, `AEV_SOUNDCHAN` and the saber
//! swing and spin sounds the parser turns `saberhup`/`saberspin` lines into.
//! Footsteps, effects, fire and move events still take their slots (and the
//! 300-event limit) but play nothing here. Custom `*` sounds play nothing, as in
//! codemp, which registers them as sound 0. A saber's own `swingSound`/
//! `spinSound` overrides are not applied: the default sounds play.

use crate::LegacyRandom;
use jkr_audio::{Attenuation, ChannelId, PlayRequest, SourceId};
use jkr_model::AnimationConfig;

/// `MAX_ANIM_EVENTS` (`bg_public.h:308`): slots per block.
pub const LEGACY_ANIMATION_EVENT_LIMIT: usize = 300;
/// `MAX_RANDOM_ANIM_SOUNDS` (`bg_public.h:323`): variants per sound line.
const RANDOM_SOUNDS: i32 = 4;

/// `CHAN_*` (`q_shared.h:864-879`) an `AEV_SOUNDCHAN` line may name.
const CHAN_AUTO: u32 = 0;
const CHAN_WEAPON: u32 = 2;
const CHAN_VOICE: u32 = 3;
const CHAN_VOICE_ATTEN: u32 = 4;
const CHAN_BODY: u32 = 6;
const CHAN_ANNOUNCER: u32 = 9;
const CHAN_VOICE_GLOBAL: u32 = 12;

/// `animEventType_t` (`bg_public.h:333-345`), in `animEventTypeTable` order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EventType {
    Sound,
    Footstep,
    Effect,
    Fire,
    Move,
    SoundChannel,
    SaberSwing,
    SaberSpin,
}

const EVENT_TYPES: [(&str, Option<EventType>); 9] = [
    ("AEV_NONE", None),
    ("AEV_SOUND", Some(EventType::Sound)),
    ("AEV_FOOTSTEP", Some(EventType::Footstep)),
    ("AEV_EFFECT", Some(EventType::Effect)),
    ("AEV_FIRE", Some(EventType::Fire)),
    ("AEV_MOVE", Some(EventType::Move)),
    ("AEV_SOUNDCHAN", Some(EventType::SoundChannel)),
    ("AEV_SABER_SWING", Some(EventType::SaberSwing)),
    ("AEV_SABER_SPIN", Some(EventType::SaberSpin)),
];

/// What a sound event plays.
#[derive(Clone, Debug, PartialEq)]
pub enum LegacyAnimationSound {
    /// `AEV_SOUND`/`AEV_SOUNDCHAN`: one of up to four files at random, on
    /// `channel`, from the player.
    File { paths: Vec<String>, channel: u32 },
    /// A `saberhup` line (`AEV_SABER_SWING`): a random swing of `weight`
    /// 0 (fast, `saberhup1-3`), 1 (medium, `4-6`) or 2 (strong, `7-9`).
    SaberSwing { weight: u8 },
    /// A `saberspin` line (`AEV_SABER_SPIN`): 0 `saberspinoff`, 1 `saberspin`,
    /// 2-4 `saberspin1-3`, 5 one of `saberspin1-3` at random.
    SaberSpin { kind: u8 },
    /// Any other event, or a custom `*` sound: plays nothing.
    Silent,
}

/// One slot of an `UPPEREVENTS`/`LOWEREVENTS` block.
#[derive(Clone, Debug, PartialEq)]
pub struct LegacyAnimationEvent {
    /// Absolute skeleton frame: the animation's first frame plus the offset.
    pub key_frame: i32,
    /// Percent chance to play; 0 always plays.
    pub probability: i32,
    pub sound: LegacyAnimationSound,
    kind: EventType,
}

/// A skeleton's parsed `animevents.cfg`: the torso's and the legs' events.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LegacyAnimationEvents {
    upper: Vec<LegacyAnimationEvent>,
    lower: Vec<LegacyAnimationEvent>,
}

/// The saber swing sounds, `saberhup1-9` (`cg_players.c:2650-2664`).
const SABER_SWINGS: [&str; 9] = [
    "sound/weapons/saber/saberhup1.wav",
    "sound/weapons/saber/saberhup2.wav",
    "sound/weapons/saber/saberhup3.wav",
    "sound/weapons/saber/saberhup4.wav",
    "sound/weapons/saber/saberhup5.wav",
    "sound/weapons/saber/saberhup6.wav",
    "sound/weapons/saber/saberhup7.wav",
    "sound/weapons/saber/saberhup8.wav",
    "sound/weapons/saber/saberhup9.wav",
];

/// The saber spin sounds by spin kind 0-4 (`cg_players.c:2686-2704`).
const SABER_SPINS: [&str; 5] = [
    "sound/weapons/saber/saberspinoff.wav",
    "sound/weapons/saber/saberspin.wav",
    "sound/weapons/saber/saberspin1.wav",
    "sound/weapons/saber/saberspin2.wav",
    "sound/weapons/saber/saberspin3.wav",
];

impl LegacyAnimationEvents {
    /// Parse `text`, the `animevents.cfg` of a skeleton whose `animation.cfg`
    /// is `config`. `include` returns the text of `models/players/<name>/`'s
    /// file for an `include <name>` line, parsed into the same blocks.
    pub fn parse(
        text: &str,
        config: &AnimationConfig,
        include: &mut dyn FnMut(&str) -> Option<String>,
    ) -> Self {
        let mut events = Self::default();
        events.parse_into(text, config, include, 0);
        events
    }

    fn parse_into(
        &mut self,
        text: &str,
        config: &AnimationConfig,
        include: &mut dyn FnMut(&str) -> Option<String>,
        depth: usize,
    ) {
        let mut tokens = Tokens::new(text);
        loop {
            let token = tokens.next();
            if token.is_empty() {
                break;
            }
            if token.eq_ignore_ascii_case("include") {
                let name = tokens.next();
                // codemp recurses without a bound; a file including itself
                // would never return there.
                if depth < 8
                    && let Some(text) = include(name)
                {
                    self.parse_into(&text, config, include, depth + 1);
                }
            } else if token.eq_ignore_ascii_case("UPPEREVENTS") {
                parse_block(&mut tokens, config, &mut self.upper);
            } else if token.eq_ignore_ascii_case("LOWEREVENTS") {
                parse_block(&mut tokens, config, &mut self.lower);
            }
        }
    }

    /// The torso's (`true`) or the legs' events, in slot order.
    pub fn track(&self, torso: bool) -> &[LegacyAnimationEvent] {
        if torso { &self.upper } else { &self.lower }
    }

    /// Every sound file an event may play, for registration up front.
    pub fn for_each_sound_path(&self, mut visit: impl FnMut(&str)) {
        for event in self.upper.iter().chain(&self.lower) {
            match &event.sound {
                LegacyAnimationSound::File { paths, .. } => {
                    paths.iter().for_each(|path| visit(path));
                }
                LegacyAnimationSound::SaberSwing { weight } => {
                    let first = usize::from(*weight) * 3;
                    SABER_SWINGS[first..first + 3]
                        .iter()
                        .for_each(|path| visit(path));
                }
                LegacyAnimationSound::SaberSpin { kind: 5 } => {
                    SABER_SPINS[2..].iter().for_each(|path| visit(path));
                }
                LegacyAnimationSound::SaberSpin { kind } => {
                    visit(SABER_SPINS[usize::from(*kind).min(4)]);
                }
                LegacyAnimationSound::Silent => {}
            }
        }
    }
}

/// `ParseAnimationEvtBlock`: each block refills slots from the first, so a
/// second block of the same kind overwrites the first's leading slots.
fn parse_block(
    tokens: &mut Tokens<'_>,
    config: &AnimationConfig,
    slots: &mut Vec<LegacyAnimationEvent>,
) {
    loop {
        let token = tokens.next();
        if token.is_empty() || token == "{" {
            break;
        }
    }
    let mut last = 0usize;
    loop {
        if last >= LEGACY_ANIMATION_EVENT_LIMIT {
            // codemp drops the game here; keep what was read and skip the rest.
            skip_block(tokens);
            return;
        }
        let token = tokens.next();
        if token.is_empty() || token == "}" {
            return;
        }
        // An unknown animation, or one this skeleton lacks: skip the line.
        let Some(sequence) = config.get(token) else {
            tokens.skip_line();
            continue;
        };
        let first_frame = sequence.first_frame as i32;
        let kind = tokens.next();
        let Some(kind) = EVENT_TYPES
            .iter()
            .find(|(name, _)| kind.eq_ignore_ascii_case(name))
            .and_then(|(_, kind)| *kind)
        else {
            continue;
        };
        let key_frame = first_frame.wrapping_add(atoi(tokens.next()));
        let current = slots
            .iter()
            .position(|slot| slot.key_frame == key_frame && slot.kind == kind)
            .unwrap_or(last);
        let event = match kind {
            EventType::Sound | EventType::SoundChannel => {
                let channel = if kind == EventType::SoundChannel {
                    channel(tokens.next())
                } else {
                    CHAN_AUTO
                };
                sound_event(tokens, key_frame, kind, channel)
            }
            EventType::Footstep | EventType::Fire => {
                tokens.next();
                tokens.next();
                silent(key_frame, kind)
            }
            EventType::Effect | EventType::Move => {
                tokens.next();
                tokens.next();
                tokens.next();
                silent(key_frame, kind)
            }
            // Declared directly these take no data: codemp skips the line
            // without advancing to the next slot.
            EventType::SaberSwing | EventType::SaberSpin => {
                tokens.skip_line();
                continue;
            }
        };
        if current < slots.len() {
            slots[current] = event;
        } else {
            slots.push(event);
        }
        if current == last {
            last += 1;
        }
    }
}

fn skip_block(tokens: &mut Tokens<'_>) {
    loop {
        let token = tokens.next();
        if token.is_empty() || token == "}" {
            return;
        }
    }
}

fn silent(key_frame: i32, kind: EventType) -> LegacyAnimationEvent {
    LegacyAnimationEvent {
        key_frame,
        probability: 0,
        sound: LegacyAnimationSound::Silent,
        kind,
    }
}

/// The `AEV_SOUND` data: path, variant range and chance, then the parser's
/// saber swing/spin substitution (`bg_panimate.c:1934-2020`).
fn sound_event(
    tokens: &mut Tokens<'_>,
    key_frame: i32,
    kind: EventType,
    channel: u32,
) -> LegacyAnimationEvent {
    let path = tokens.next().replace('\\', "/").to_ascii_lowercase();
    let lowest = atoi(tokens.next());
    let mut highest = atoi(tokens.next());
    let probability = atoi(tokens.next());
    let mut event = LegacyAnimationEvent {
        key_frame,
        probability,
        sound: LegacyAnimationSound::Silent,
        kind,
    };
    if path.starts_with("sound/weapons/saber/saberhup") {
        event.kind = EventType::SaberSwing;
        event.sound = LegacyAnimationSound::SaberSwing {
            weight: match lowest {
                ..4 => 0,
                4..7 => 1,
                _ => 2,
            },
        };
        return event;
    }
    if path.starts_with("sound/weapons/saber/saberspin") {
        event.kind = EventType::SaberSpin;
        let spin = match path.as_bytes().get(29) {
            Some(b'o') => 0,
            Some(b'1') => 2,
            Some(b'2') => 3,
            Some(b'3') => 4,
            Some(b'%') => 5,
            _ => 1,
        };
        event.sound = LegacyAnimationSound::SaberSpin { kind: spin };
        return event;
    }
    if path.starts_with('*') {
        return event;
    }
    let paths = if lowest != 0 && highest != 0 {
        if highest - lowest >= RANDOM_SOUNDS {
            highest = lowest + RANDOM_SOUNDS - 1;
        }
        (lowest..=highest)
            .map(|variant| path.replacen("%d", &variant.to_string(), 1))
            .collect()
    } else {
        vec![path]
    };
    if !paths.is_empty() {
        event.sound = LegacyAnimationSound::File { paths, channel };
    }
    event
}

/// The `AEV_SOUNDCHAN` channel names; anything else is `CHAN_AUTO`.
fn channel(name: &str) -> u32 {
    [
        ("CHAN_VOICE_ATTEN", CHAN_VOICE_ATTEN),
        ("CHAN_VOICE_GLOBAL", CHAN_VOICE_GLOBAL),
        ("CHAN_ANNOUNCER", CHAN_ANNOUNCER),
        ("CHAN_BODY", CHAN_BODY),
        ("CHAN_WEAPON", CHAN_WEAPON),
        ("CHAN_VOICE", CHAN_VOICE),
    ]
    .iter()
    .find(|(known, _)| name.eq_ignore_ascii_case(known))
    .map_or(CHAN_AUTO, |(_, channel)| *channel)
}

/// C `atoi`: an optional sign and leading digits, 0 for anything else.
fn atoi(token: &str) -> i32 {
    let bytes = token.trim_start().as_bytes();
    let (negative, digits) = match bytes.first() {
        Some(b'-') => (true, &bytes[1..]),
        Some(b'+') => (false, &bytes[1..]),
        _ => (false, bytes),
    };
    let value = digits
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .fold(0i32, |value, digit| {
            value.wrapping_mul(10).wrapping_add(i32::from(digit - b'0'))
        });
    if negative {
        value.wrapping_neg()
    } else {
        value
    }
}

/// `COM_Parse` tokens: whitespace-separated, `//` and `/* */` comments
/// skipped, `"quoted"` strings whole; the empty string at the end.
struct Tokens<'a> {
    text: &'a str,
    position: usize,
}

impl<'a> Tokens<'a> {
    fn new(text: &'a str) -> Self {
        Self { text, position: 0 }
    }

    fn next(&mut self) -> &'a str {
        let bytes = self.text.as_bytes();
        loop {
            while bytes
                .get(self.position)
                .is_some_and(|byte| byte.is_ascii_whitespace())
            {
                self.position += 1;
            }
            if bytes[self.position..].starts_with(b"//") {
                while bytes.get(self.position).is_some_and(|&byte| byte != b'\n') {
                    self.position += 1;
                }
            } else if bytes[self.position..].starts_with(b"/*") {
                self.position += 2;
                while self.position < bytes.len() && !bytes[self.position..].starts_with(b"*/") {
                    self.position += 1;
                }
                self.position = (self.position + 2).min(bytes.len());
            } else {
                break;
            }
        }
        if bytes.get(self.position) == Some(&b'"') {
            let start = self.position + 1;
            let end = bytes[start..]
                .iter()
                .position(|&byte| byte == b'"' || byte == b'\n')
                .map_or(bytes.len(), |offset| start + offset);
            self.position = (end + 1).min(bytes.len());
            return &self.text[start..end];
        }
        let start = self.position;
        while bytes
            .get(self.position)
            .is_some_and(|byte| !byte.is_ascii_whitespace())
        {
            self.position += 1;
        }
        &self.text[start..self.position]
    }

    /// `SkipRestOfLine`: drop what remains of the last token's line.
    fn skip_line(&mut self) {
        let bytes = self.text.as_bytes();
        while let Some(&byte) = bytes.get(self.position) {
            self.position += 1;
            if byte == b'\n' {
                return;
            }
        }
    }
}

/// How a track moved since the last frame it was seen on, for the range
/// test of `CG_PlayerAnimEvents`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LegacyFramePassage {
    pub old_frame: i32,
    pub frame: i32,
    /// `Some` when the track still plays the same animation: whether it
    /// plays backwards, and its `[first, first + count)` frames if it loops.
    pub same_animation: Option<(bool, Option<(i32, i32)>)>,
}

impl LegacyFramePassage {
    /// Whether an event on `key_frame` fires (`cg_players.c:2896-2960`): on
    /// the exact frame, or within three frames of a jump the same animation
    /// passed over, forwards, backwards or across its loop.
    pub fn fires(self, key_frame: i32) -> bool {
        let (old, frame) = (self.old_frame, self.frame);
        if key_frame == frame {
            return true;
        }
        if (old - frame).abs() <= 1 {
            return false;
        }
        let Some((backward, looping)) = self.same_animation else {
            return false;
        };
        if (old - key_frame).abs() > 3 && (frame - key_frame).abs() > 3 {
            return false;
        }
        let in_loop = looping.is_some_and(|(first, last)| key_frame >= first && key_frame < last);
        if backward {
            (old > key_frame && frame < key_frame) || (in_loop && old > key_frame && frame > old)
        } else {
            (old < key_frame && frame > key_frame) || (in_loop && old < key_frame && frame < old)
        }
    }
}

/// One sound to start for an actor: the file and its `CHAN_*` channel.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LegacyAnimationSoundStart<'a> {
    pub path: &'a str,
    pub channel: u32,
}

impl LegacyAnimationSoundStart<'_> {
    /// The mixer request for this sound from entity `source`, as
    /// `S_StartSound(NULL, entityNum, channel, sfx)`: at `origin` with the
    /// channel's falloff, or without falloff for the local player (`None`).
    /// codemp starts a saber swing at `pos.trBase` instead of on the entity;
    /// here every sound follows its actor.
    pub fn request(&self, source: u16, origin: Option<[f32; 3]>) -> PlayRequest {
        PlayRequest {
            origin,
            source: SourceId(u32::from(source)),
            channel: ChannelId(crate::sound_events::normalize_voice_channel(self.channel)),
            volume: 1.0,
            attenuation: if origin.is_some() {
                crate::sound_events::normal_attenuation(self.channel)
            } else {
                Attenuation::None
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct TrackMemory {
    clip: usize,
    frame: i32,
    seen: u32,
}

/// `cent->pe.legs.frame`/`torso.frame` for every entity, and the random
/// stream for chances and variants. Fixed storage: no per-frame allocation.
pub struct LegacyAnimationEventTracker {
    memory: Box<[[TrackMemory; 2]]>,
    frame: u32,
    random: LegacyRandom,
}

impl LegacyAnimationEventTracker {
    /// Memory for entity numbers below `entities`.
    pub fn new(entities: usize) -> Self {
        Self {
            memory: vec![[TrackMemory::default(); 2]; entities].into_boxed_slice(),
            frame: 1,
            random: LegacyRandom::seeded(0x5eed_a11e),
        }
    }

    /// Start a rendered frame. A track not observed in the previous frame
    /// starts over: its first frame back only records where it is.
    pub fn begin_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1).max(2);
    }

    /// `CG_TriggerAnimSounds` for one track of `entity`: `clip` and the
    /// fractional `frame` it shows now. Calls `start` for each sound due.
    #[allow(clippy::too_many_arguments)]
    pub fn observe(
        &mut self,
        entity: usize,
        torso: bool,
        clip: usize,
        frame: f32,
        events: &LegacyAnimationEvents,
        config: &AnimationConfig,
        mut start: impl FnMut(LegacyAnimationSoundStart<'_>),
    ) {
        let Some(slots) = self.memory.get_mut(entity) else {
            return;
        };
        let memory = &mut slots[usize::from(torso)];
        let current = frame.floor() as i32;
        let previous = *memory;
        *memory = TrackMemory {
            clip,
            frame: current,
            seen: self.frame,
        };
        if previous.seen != self.frame.wrapping_sub(1) || previous.frame == current {
            return;
        }
        let same_animation = (previous.clip == clip).then(|| {
            crate::legacy_animation_name(clip)
                .and_then(|name| config.get_exact(name))
                .map_or((false, None), |sequence| {
                    let first = sequence.first_frame as i32;
                    (
                        sequence.frames_per_second < 0.0,
                        (sequence.loop_frame != -1)
                            .then_some((first, first + sequence.frame_count as i32)),
                    )
                })
        });
        let passage = LegacyFramePassage {
            old_frame: previous.frame,
            frame: current,
            same_animation,
        };
        for event in events.track(torso) {
            if matches!(event.sound, LegacyAnimationSound::Silent)
                || !passage.fires(event.key_frame)
            {
                continue;
            }
            if event.probability != 0 && event.probability <= self.random.irand(0, 99) {
                continue;
            }
            if let Some(sound) = choose(&event.sound, &mut self.random) {
                start(sound);
            }
        }
    }
}

/// `CG_PlayerAnimEventDo`'s choice of file for one event.
fn choose<'a>(
    sound: &'a LegacyAnimationSound,
    random: &mut LegacyRandom,
) -> Option<LegacyAnimationSoundStart<'a>> {
    let (path, channel) = match sound {
        LegacyAnimationSound::File { paths, channel } => {
            let index = random.irand(0, paths.len() as i32 - 1);
            (paths.get(index as usize)?.as_str(), *channel)
        }
        LegacyAnimationSound::SaberSwing { weight } => {
            let index = i32::from(*weight) * 3 + random.irand(0, 2);
            (SABER_SWINGS[index as usize], CHAN_AUTO)
        }
        LegacyAnimationSound::SaberSpin { kind } => {
            let index = match kind {
                0..=4 => usize::from(*kind),
                _ => random.irand(2, 4) as usize,
            };
            (SABER_SPINS[index], CHAN_AUTO)
        }
        LegacyAnimationSound::Silent => return None,
    };
    Some(LegacyAnimationSoundStart { path, channel })
}

#[cfg(test)]
#[path = "animation_events_tests.rs"]
mod tests;
