//! Modern data-driven in-game HUD over authoritative snapshot values.

mod action_timer;
mod brackets;
pub(crate) use brackets::{BracketInputs, Fighter};
mod data_source;
mod duel_panel;
pub(crate) mod force_flash;
use data_source::*;
pub(crate) mod enemy_info;
pub(crate) mod family;
pub(crate) mod icons;
pub(crate) mod identification;
mod info;
pub(crate) mod movement;
mod movement_keys;
pub(crate) mod options;
mod portrait;
mod radar;
pub(crate) use radar::{RadarAssets, RadarInputs};
mod lead_indicator;
pub(crate) use lead_indicator::LeadInputs;
mod rocket_lock;
pub(crate) use rocket_lock::RocketLockInputs;
mod crosshair_bars;
pub(crate) use crosshair_bars::CrosshairBarInputs;
mod meters;
mod pickup;
mod selection;
mod siege;
mod simple;
mod spectator;
mod vehicle;
mod vehicle_damage;
pub(crate) use siege::SiegeInputs;
pub(crate) use vehicle_damage::{Panel as DamagePanel, recovered as vehicle_damage_recovered};
pub(crate) mod targeting;
mod text_values;
pub(crate) mod tints;
mod update;
mod vote;
mod widgets;

use super::{Localization, TextVertex, UiFont};
use crate::{console::ViewerConsole, ui_renderer};
use jkr_client::pmove::MovementState;
use jkr_client::{
    ClientSession, HudDataSource as ClientHudData, TeamInfo, legacy_hud_data,
    legacy_predicted_hud_data, legacy_team_location,
};
use jkr_protocol::{GameState, PlayerState};
use jkr_ui::{
    DrawList, Easing, HudDataSource, HudLayoutDocument, Insets, LayoutContext, LayoutEngine,
    LayoutKind, LayoutScratch, TextId, Theme, Tween, Vec2, Widget, WidgetId, WidgetTree,
};
use std::path::Path;

const WIDGET_LIMIT: usize = 64;
/// Room for every widget plus the radar: its two-stage background and ~75 marks.
const DRAW_LIMIT: usize = 480;
/// Commands of the part under a zoom mask: tints, two passes of the rocket lock, meters.
const UNDER_SCOPE_LIMIT: usize = 128;
const DEFAULT_LAYOUT: &str = include_str!("../assets/hud/default.json");
/// The weapon name stays fully visible this long after a switch, then fades.
const WEAPON_HOLD_MS: u64 = 2_000;
const WEAPON_FADE_MS: u64 = 600;
/// The newest obituary stays this long at the top left, then fades.
const KILL_HOLD_MS: u64 = 2_500;
const KILL_FADE_MS: u64 = 700;

/// Opacity of a transient that holds for `hold` ms and fades over `fade` ms.
fn transient_alpha(age_ms: u64, hold: u64, fade: u64) -> f32 {
    if age_ms <= hold {
        1.0
    } else {
        1.0 - (age_ms - hold).min(fade) as f32 / fade as f32
    }
}
const CLASSIC_LAYOUT: &str = include_str!("../assets/hud/classic.json");

/// Independent visibility switches used by widget predicates and the HUD shader.
#[derive(Clone, Copy)]
pub(crate) struct HudVisibility {
    pub(crate) hud: bool,
    pub(crate) status: bool,
    pub(crate) weapon: bool,
    pub(crate) crosshair: bool,
    pub(crate) crosshair_names: bool,
    pub(crate) timer: bool,
    pub(crate) lagometer: bool,
    /// Team-status visibility, independent of the other HUD widgets.
    pub(crate) team_overlay: bool,
    /// The third-person ground HUD stands in for health, shield, Force and
    /// stance this frame (`crate::ground_hud`), so those widgets hide.
    pub(crate) ground_hud: bool,
}

impl HudVisibility {
    pub(crate) fn from_console(console: Option<&ViewerConsole>) -> Self {
        let enabled = |name, fallback| {
            console
                .and_then(|console| console.bool_cvar(name))
                .unwrap_or(fallback)
        };
        let hud = enabled("cg_drawHud", true) && enabled("cg_draw2D", true);
        Self {
            hud,
            status: hud && enabled("cg_drawStatus", true),
            weapon: hud && enabled("cg_drawWeapon", true),
            crosshair: hud && enabled("cg_crosshair", true),
            crosshair_names: hud && enabled("cg_drawCrosshairNames", true),
            timer: hud && enabled("cg_drawTimer", false),
            lagometer: hud && enabled("cg_lagometer", false),
            team_overlay: hud
                && console
                    .and_then(|c| c.integer_cvar("cg_drawteamoverlay"))
                    .unwrap_or(0)
                    > 0,
            ground_hud: false,
        }
    }

    /// Intermission hides every HUD layer.
    pub(crate) const HIDDEN: Self = Self {
        hud: false,
        status: false,
        weapon: false,
        crosshair: false,
        crosshair_names: false,
        timer: false,
        lagometer: false,
        team_overlay: false,
        ground_hud: false,
    };
}

/// Allocation-free-per-frame widget state for the local player's HUD.
pub(crate) struct HudOverlay {
    pub(crate) action_remaining: Option<f32>,
    /// Map-lifetime textures and sampled icon presentation policy.
    pub(crate) icons: icons::Icons,
    /// The last item picked up: its icon and the crosshair's swell.
    pub(crate) pickup: pickup::Pickup,
    /// The piloted vehicle's HUD.
    pub(crate) vehicle: vehicle::Panel,
    /// The duel spectator's panel.
    /// `ps.fallingToDeath`: when the player began falling to death, 0 when not.
    falling_since: i32,
    pub(crate) vehicle_damage: vehicle_damage::DamageHud,
    pub(crate) brackets: brackets::Brackets,
    pub(crate) movement_keys: movement_keys::MovementKeys,
    pub(crate) force_flash: force_flash::ForceFlash,
    simple: simple::SimpleHud,
    pub(crate) duel_panel: duel_panel::DuelPanel,
    pub(crate) tints: tints::State,
    /// World-projected labels, sharing the chat roster's retained names.
    pub(crate) identification: identification::State,
    guides: movement::Guides,
    family: family::Policy,
    pub(crate) targeting: targeting::State,
    pub(crate) enemy_info: enemy_info::State,
    score_text: String,
    /// `CG_DrawMiniScoreboard`'s "Red: x Blue: y", empty when it does not show.
    mini_scores: String,
    snapshot_text: String,

    inventory_bits: u32,
    selector: Option<jkr_client::selection::SelectionView>,

    values: Option<ClientHudData>,
    health: String,
    armor: String,
    force: String,
    weapon: String,
    ammo: String,
    health_value: String,
    armor_value: String,
    force_value: String,
    weapon_value: String,
    ammo_value: String,
    style_value: String,
    /// HUD time of the last weapon change; drives the weapon-name fade.
    weapon_shown_ms: Option<u64>,
    weapon_alpha: f32,
    team_revision: u64,
    team_side: u8,
    team_rows: [String; 8],
    team_names: [String; 8],
    team_locations: [String; 8],
    team_stats: [String; 8],
    team_gear: [String; 8],
    team_len: usize,
    vote_active: bool,
    team_vote_active: bool,
    vote_heading: String,
    vote_text: String,
    team_vote_heading: String,
    team_vote_text: String,
    vote_keys: String,
    yes_keys: String,
    no_keys: String,
    kill_rows: [String; 8],
    kill_len: usize,
    kill_alpha: f32,
    crosshair_name: String,
    speed: options::Speed,
    crosshair_alpha: f32,
    crosshair_teammate: bool,
    match_timer: String,
    warmup_text: String,
    /// What a spectator, or a follower, is told.
    spectator: spectator::Lines,
    /// The Siege round clock and respawn wave.
    siege: siege::State,
    /// The radar of team games and vehicles.
    radar: radar::State,
    rocket_lock: rocket_lock::State,
    /// jaPRO's `cg_leadIndicator`.
    lead_indicator: lead_indicator::State,
    /// The health, ammunition and hack bars under the crosshair.
    crosshair_bars: crosshair_bars::State,
    /// The jetpack, cloak and E-Web meters at the right edge.
    meters: meters::State,
    interrupted: bool,
    lagometer: jkr_client::LagometerSamples,
    default_document: HudLayoutDocument,
    classic_document: HudLayoutDocument,
    override_document: Option<HudLayoutDocument>,
    tree: WidgetTree,
    scratch: LayoutScratch,
    draw_list: DrawList,
    /// What stock draws before a zoom mask covers it: the screen tints, the rocket lock and the
    /// fuel and E-Web meters.
    under_scope: DrawList,
    theme: Theme,
    ratios: [Tween; 3],
    displayed_ratios: [f32; 3],
}

/// Pixel-space geometry consumed by the existing fullscreen HUD renderer.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct HudLayout {
    pub(crate) health_bar: [f32; 4],
    pub(crate) armor_bar: [f32; 4],
    pub(crate) force_bar: [f32; 4],
}

/// The status widgets' bindings (`CG_DrawStats`), which a fall to death's fade covers.
fn status_binding(binding: Option<&str>) -> bool {
    binding.is_some_and(|binding| {
        ["health", "armor", "force", "weapon", "ammo", "style"]
            .iter()
            .any(|status| binding.starts_with(status))
    })
}

impl HudOverlay {
    pub(crate) fn new() -> Self {
        let override_document = crate::platform::user_config_file()
            .ok()
            .and_then(|path| path.parent().map(|parent| parent.join("hud.json")))
            .and_then(|path| load_override(&path));
        Self {
            action_remaining: None,
            icons: icons::Icons::default(),
            pickup: pickup::Pickup::default(),
            vehicle: vehicle::Panel::default(),
            falling_since: 0,
            vehicle_damage: Default::default(),
            brackets: Default::default(),
            movement_keys: Default::default(),
            force_flash: Default::default(),
            simple: Default::default(),
            duel_panel: duel_panel::DuelPanel::default(),
            tints: tints::State::default(),
            guides: movement::Guides::default(),
            family: family::Policy::default(),
            targeting: targeting::State::default(),
            enemy_info: enemy_info::State::default(),
            identification: identification::State::default(),
            score_text: String::with_capacity(80),
            mini_scores: String::with_capacity(48),
            snapshot_text: String::with_capacity(96),

            inventory_bits: 0,
            values: None,
            health: String::with_capacity(24),
            armor: String::with_capacity(24),
            force: String::with_capacity(24),
            weapon: String::with_capacity(40),
            ammo: String::with_capacity(24),
            health_value: String::with_capacity(12),
            armor_value: String::with_capacity(12),
            force_value: String::with_capacity(12),
            weapon_value: String::with_capacity(32),
            ammo_value: String::with_capacity(12),
            style_value: String::with_capacity(12),
            weapon_shown_ms: None,
            weapon_alpha: 0.0,
            selector: None,

            team_revision: 0,
            team_side: 0,
            team_rows: std::array::from_fn(|_| String::with_capacity(96)),
            team_names: std::array::from_fn(|_| String::with_capacity(32)),
            team_locations: std::array::from_fn(|_| String::with_capacity(32)),
            team_stats: std::array::from_fn(|_| String::with_capacity(16)),
            team_gear: std::array::from_fn(|_| String::with_capacity(256)),
            team_len: 0,
            vote_active: false,
            team_vote_active: false,
            vote_heading: String::with_capacity(64),
            vote_text: String::with_capacity(160),
            team_vote_heading: String::with_capacity(64),
            team_vote_text: String::with_capacity(160),
            vote_keys: String::with_capacity(96),
            yes_keys: String::with_capacity(48),
            no_keys: String::with_capacity(48),
            kill_rows: std::array::from_fn(|_| String::with_capacity(128)),
            kill_len: 0,
            kill_alpha: 0.0,
            crosshair_name: String::with_capacity(64),
            speed: options::Speed::default(),
            crosshair_alpha: 0.0,
            crosshair_teammate: false,
            match_timer: String::with_capacity(16),
            warmup_text: String::with_capacity(48),
            spectator: spectator::Lines::new(),
            siege: siege::State::default(),
            radar: radar::State::default(),
            rocket_lock: rocket_lock::State::default(),
            lead_indicator: lead_indicator::State::default(),
            crosshair_bars: crosshair_bars::State::default(),
            meters: meters::State::default(),
            interrupted: false,
            lagometer: jkr_client::LagometerSamples::new(),
            default_document: HudLayoutDocument::from_json(DEFAULT_LAYOUT)
                .expect("bundled modern HUD document is valid"),
            classic_document: HudLayoutDocument::from_json(CLASSIC_LAYOUT)
                .expect("bundled classic HUD document is valid"),
            override_document,
            tree: WidgetTree::new(WIDGET_LIMIT),
            scratch: LayoutScratch::new(WIDGET_LIMIT),
            draw_list: DrawList::new(DRAW_LIMIT),
            under_scope: DrawList::new(UNDER_SCOPE_LIMIT),
            theme: Theme::default(),
            ratios: [
                Tween::settled(1.0),
                Tween::settled(0.0),
                Tween::settled(1.0),
            ],
            displayed_ratios: [1.0, 0.0, 1.0],
        }
    }

    /// Build, lay out and retain the HUD widget draw list. `user_scale` is
    /// the player's `cg_hudScale` on top of the resolution-derived scale.
    pub(crate) fn layout(
        &mut self,
        font: &UiFont,
        viewport: [f32; 2],
        user_scale: f32,
        visibility: HudVisibility,
        time_ms: u64,
    ) -> HudLayout {
        self.displayed_ratios = self.ratios.map(|tween| tween.sample(time_ms));
        self.weapon_alpha = self.weapon_shown_ms.map_or(0.0, |shown| {
            transient_alpha(
                time_ms.saturating_sub(shown),
                WEAPON_HOLD_MS,
                WEAPON_FADE_MS,
            )
        });
        let modern = font.is_modern();
        let document = if modern {
            self.override_document
                .as_ref()
                .unwrap_or(&self.default_document)
        } else {
            &self.classic_document
        };
        // `CG_Draw2D`: a free spectator sees no status, weapon or ammo, only what it is
        // told and the crosshair; nor does a dead player, nor a vehicle's pilot whose
        // vehicle hides its rider (its own HUD stands in).
        let status_allowed = visibility.status && !self.spectator.hides_status();
        let visibility = if self.spectator.hides_status() || self.vehicle.hides_status() {
            HudVisibility {
                status: false,
                weapon: false,
                ..visibility
            }
        } else {
            visibility
        };
        // Stock's simple HUD stands in for the status widgets only (`cg_hudFiles`).
        let simple = self.family.simple && visibility.status;
        let data = WidgetData {
            visibility: if simple {
                HudVisibility {
                    status: false,
                    weapon: false,
                    ..visibility
                }
            } else {
                visibility
            },
            ratios: self.displayed_ratios,
            health: &self.health,
            armor: &self.armor,
            force: &self.force,
            weapon: &self.weapon,
            ammo: &self.ammo,
            health_value: &self.health_value,
            armor_value: &self.armor_value,
            force_value: &self.force_value,
            weapon_value: &self.weapon_value,
            ammo_value: &self.ammo_value,
            style: !self.style_value.is_empty(),
            weapon_alpha: self.weapon_alpha,
            team_len: self.team_len,
            vote_active: self.vote_active,
            team_vote_active: self.team_vote_active,
            kill_len: self.kill_len,
            crosshair_name: !self.crosshair_name.is_empty(),
            timer: !self.match_timer.is_empty(),
            warmup: !self.warmup_text.is_empty(),
            spectator: self.spectator.spectating(),
            follow: self.spectator.following(),
            siege_clock: (!self.siege.clock.is_empty()).then_some(self.siege.clock_own),
            siege_respawn: !self.siege.respawn.is_empty(),
            interrupted: self.interrupted,
        };
        let mut visible = [false; WIDGET_LIMIT];
        for (index, widget) in document.widgets.iter().take(WIDGET_LIMIT).enumerate() {
            visible[index] = visibility.hud
                && widget.visibility.evaluate(&data)
                && self.family.visible(widget.binding.as_deref());
        }
        self.tree.clear();
        for (index, widget) in document.widgets.iter().take(WIDGET_LIMIT).enumerate() {
            let _ = self.tree.add(Widget {
                id: WidgetId(index as u32),
                parent: None,
                layout: LayoutKind::Anchored {
                    anchor: widget.anchor,
                    offset: widget.offset,
                },
                size: widget.size,
                visible: visible[index],
                focusable: false,
                scrollable: false,
                opacity: 1.0,
                z: widget.layer,
            });
        }
        let dpi_scale =
            (viewport[1] / 1_080.0).clamp(2.0 / 3.0, 4.0 / 3.0) * user_scale.clamp(0.25, 2.0);
        let upper_right_bottom =
            self.upper_right_stack()[2] * (viewport[1] / 1080.0).clamp(0.6, 2.5);
        let rectangles = LayoutEngine.layout(
            &self.tree,
            LayoutContext {
                viewport_physical: Vec2::new(viewport[0], viewport[1]),
                dpi_scale,
                safe_area: Insets::all(0.0),
            },
            &mut self.scratch,
        );
        self.draw_list.clear();
        self.under_scope.clear();
        self.tints.emit(&mut self.under_scope, viewport);
        // Stock draws the rocket lock straight after its screen tints, under the rest.
        self.rocket_lock.emit(&mut self.under_scope, &self.radar);
        self.lead_indicator.emit(&mut self.draw_list);
        let mut output = HudLayout::default();
        let low_health = self.values.is_some_and(|value| value.health <= 25);
        let low_ammo = self
            .values
            .is_some_and(|value| value.ammo.is_some_and(|ammo| ammo <= 5));
        let pulse = Tween::pulse(0.68, 1.0, time_ms, self.theme.motion.slow);
        self.force_flash.update(time_ms, visibility.status);
        let force_flash = self.force_flash.red(time_ms);
        // `CG_Draw2D`: falling to death, the screen goes black over half `FALL_FADE_TIME`,
        // over the status. Text is drawn above every shape here, so what the black covers in
        // stock fades with it instead.
        let fall_fade = if visibility.hud && self.falling_since != 0 {
            ((time_ms as f32 - self.falling_since as f32) / 1_500.0).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let covered = jkr_ui::DrawCommand::PushOpacity(1.0 - fall_fade);
        for (index, (widget, rect)) in document.widgets.iter().zip(rectangles).enumerate() {
            if !visible[index] {
                continue;
            }
            let fading = fall_fade > 0.0 && status_binding(widget.binding.as_deref());
            if fading {
                let _ = self.draw_list.push(covered.clone());
            }
            widgets::emit(
                &mut self.draw_list,
                self.theme,
                widget,
                if widget.binding.as_deref() == Some("team_rows") {
                    let mut rect = self.family.team_rect(*rect, viewport);
                    if self.family.team[1] == 0.0 {
                        rect.y = rect.y.max(upper_right_bottom);
                    }
                    rect
                } else {
                    *rect
                },
                &widgets::EmitContext {
                    data: &data,
                    family: self.family,
                    targeting: self.targeting.policy,
                    icons: &self.icons,
                    viewport,
                    dpi_scale,
                    hero_scale: (viewport[1] / 1080.0).clamp(0.6, 2.5)
                        * user_scale.clamp(0.25, 2.0),
                    low_health,
                    low_ammo,
                    force_flash,
                    pulse,
                    team_side: self.team_side,
                    team_len: self.team_len,
                    kill_len: self.kill_len,
                    kill_alpha: self.kill_alpha,
                    crosshair_alpha: self.crosshair_alpha,
                    crosshair_teammate: self.crosshair_teammate,
                    lagometer: &self.lagometer,
                },
                &mut output,
            );
            if fading {
                let _ = self.draw_list.push(jkr_ui::DrawCommand::PopOpacity);
            }
        }
        let _ = self.draw_list.push(covered);
        if status_allowed {
            self.vehicle
                .emit(&self.icons, &mut self.draw_list, viewport);
            self.vehicle_damage.emit(&mut self.draw_list, viewport);
        }
        if visibility.hud {
            self.duel_panel.emit(&mut self.draw_list, viewport);
        }
        if simple {
            self.simple.emit(&mut self.draw_list, viewport, force_flash);
        }
        self.pickup.update(time_ms);
        self.targeting.swell = self.pickup.crosshair_scale(time_ms);
        // `CG_DrawPickupItem`, with the status: for the living, in the game.
        if visibility.status
            && let Some((item, alpha)) = self.pickup.icon(time_ms)
        {
            self.icons
                .pickup(&mut self.draw_list, viewport, item, alpha);
        }
        let _ = self.draw_list.push(jkr_ui::DrawCommand::PopOpacity);
        if fall_fade > 0.0 {
            let _ = self.draw_list.push(jkr_ui::DrawCommand::SolidRect {
                rect: jkr_ui::Rect::new(0.0, 0.0, viewport[0], viewport[1]),
                color: jkr_ui::Color::new(0.0, 0.0, 0.0, fall_fade),
            });
        }
        if visibility.crosshair {
            let crosshair = self.targeting.parameters(viewport);
            self.crosshair_bars.emit(
                &mut self.draw_list,
                viewport,
                crosshair,
                self.icons.hacker,
            );
        }
        if visibility.hud {
            action_timer::emit(
                &mut self.draw_list,
                self.action_remaining,
                viewport,
                user_scale,
            );
            // Stock draws these before the zoom mask too (`cg_draw.c:8008-8025`).
            self.meters.emit(&mut self.under_scope, viewport);
            self.speed.emit(&mut self.draw_list, self.theme, viewport);
            selection::emit(
                &mut self.draw_list,
                self.selector
                    .filter(|s| !s.inventory || self.family.inventory),
                self.theme,
                viewport,
                user_scale,
            );
            self.emit_family(viewport);
            self.icons
                .emit(&mut self.draw_list, viewport, visibility, self.family.upper);
            self.guides.emit(&mut self.draw_list, viewport);
            self.movement_keys
                .emit(&mut self.draw_list, viewport, self.theme.accent);
            self.brackets.emit(&mut self.draw_list, viewport);
        }
        output
    }

    pub(crate) fn displayed_ratios(&self) -> [f32; 3] {
        self.displayed_ratios
    }

    pub(crate) fn append(
        &self,
        _visibility: HudVisibility,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        _layout: HudLayout,
        _scale: f32,
        viewport: [f32; 2],
    ) {
        ui_renderer::append_text_commands(
            &self.draw_list,
            |id| self.resolve_text(id),
            vertices,
            font,
            viewport,
        );
    }

    pub(crate) fn draw_list(&self) -> &DrawList {
        &self.draw_list
    }

    /// The part of the HUD a zoom mask covers, drawn before it.
    pub(crate) fn under_scope_list(&self) -> &DrawList {
        &self.under_scope
    }
}
