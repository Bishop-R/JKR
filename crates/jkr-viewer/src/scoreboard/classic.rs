//! Classic scoreboard: the retail layout as EternalJK-derived clients draw it.
//!
//! The reference is `CG_DrawOldScoreboard` and `CG_DrawClientScore` in JoF
//! EternalJK's `codemp/cgame/cg_scoreboard.c` (OpenJK lineage). Positions are
//! those of the 640x480 virtual screen: the name at x 100, score, ping and time
//! at 55/70/85 % and the client ID at 97 % of the 440-unit score line (CTF:
//! score, captures, assists, defends, ping, time at 47/59/66/73/80/90 %), the
//! header bar at y 81, rows from y 118 in 25-unit rows, 15-unit interleaved rows
//! once more than 12 players are listed (or always with `cg_smallScoreboard`,
//! and in CTF), and 12-unit rows with the header moved up from 20 clients.
//! Team games list the leading team first over translucent team bands, then the
//! spectators; free-for-all lists the players, then the spectators. Your row is
//! highlighted in your rank's colour, and is added at the bottom when the list
//! does not reach it.
//!
//! Names and headings are retail `FONT_MEDIUM` (`ergoec`) text, numbers
//! `FONT_SMALL` (`ocr_a`): sizes here keep those fonts' capital heights, and
//! the numbers are plain digit strings so the game-font option can draw each
//! in its retail font. Horizontal positions spread up to 1.25x on wide screens,
//! as EternalJK's widescreen correction does, while text keeps its proportions.
//!
//! Beyond retail: the board fades in and out ([`super::motion`]), rows glide to
//! their new places when the order changes, pings are coloured by quality, and
//! rows alternate a faint stripe for reading across.

use super::ScoreRow;
use super::icons::HeadIcons;
use super::motion::Motion;
use super::style::ClassicOptions;
use crate::menu_widgets::MenuCanvas;
use jkr_ui::{Color, DrawCommand, FontWeight, Gradient, Rect, TextAlign, TextureId};

/// `GT_DUEL`, `GT_POWERDUEL`, `GT_TEAM`, `GT_SIEGE`, `GT_CTF`.
const DUEL: i32 = 3;
const POWER_DUEL: i32 = 4;
const TEAM: i32 = 6;
const SIEGE: i32 = 7;
const CTF: i32 = 8;
/// `TEAM_FREE`, `TEAM_RED`, `TEAM_BLUE`, `TEAM_SPECTATOR`.
const FREE: u8 = 0;
const RED: u8 = 1;
const BLUE: u8 = 2;
const SPECTATOR: u8 = 3;
/// `RANK_TIED_FLAG` in `persistant[PERS_RANK]`.
const RANK_TIED: u32 = 0x4000;
/// `SB_SCORELINE_X`, `SB_SCORELINE_WIDTH`.
const LINE_X: f32 = 100.0;
const LINE_WIDTH: f32 = 440.0;
/// `SB_HEADER`, `SB_TOP`.
const HEADER_Y: f32 = 86.0;
const TOP_Y: f32 = 118.0;
/// `BIGCHAR_HEIGHT`, the gap between groups.
const GROUP_GAP: f32 = 16.0;
/// Rows stop above this virtual line (retail stops at 420; the classic style
/// keeps the HUD clear but uses the room retail left empty).
const BOTTOM_Y: f32 = 470.0;
/// Most players listed at the large row size (`SB_MAXCLIENTS_NORMAL`).
const LARGE_ROWS: usize = 12;
/// Clients from which every metric shrinks (`cgs.numClients >= 20`).
const MANY_CLIENTS: usize = 20;
/// Inter's capital height as a share of its line box; text sizes are given
/// as line boxes, so a retail capital height `c` is drawn at `c / CAP`.
const CAP: f32 = 0.6;
/// Retail capital heights at scale 1: `ergoec` and `ocr_a` `H`.
const MEDIUM_CAP: f32 = 13.0;
const SMALL_CAP: f32 = 12.0;

/// Match facts shown in the header.
pub(super) struct ClassicHeader<'a> {
    pub(super) hostname: &'a str,
    pub(super) max_clients: i32,
    pub(super) gametype: i32,
    pub(super) fraglimit: i32,
    pub(super) team_scores: [i32; 2],
    pub(super) local: LocalStatus,
    /// Who last killed you, while you are dead.
    pub(super) killer: Option<&'a str>,
}

/// The viewing player's state, from the latest snapshot.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct LocalStatus {
    pub(super) client: u16,
    pub(super) team: u8,
    /// `persistant[PERS_RANK]`, with `RANK_TIED_FLAG`.
    pub(super) rank: u32,
    /// `persistant[PERS_SCORE]`.
    pub(super) score: i32,
    /// `stats[STAT_CLIENTS_READY]`.
    pub(super) ready: u32,
    pub(super) intermission: bool,
}

/// Flag icons from the HUD's item icons.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct FlagIcons {
    pub(super) red: Option<TextureId>,
    pub(super) blue: Option<TextureId>,
    pub(super) neutral: Option<TextureId>,
}

/// Row size family.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RowSize {
    /// 25-unit rows.
    Large,
    /// 15-unit interleaved rows.
    Small,
    /// 12-unit rows from [`MANY_CLIENTS`] clients, header moved up.
    Many,
}

impl RowSize {
    /// `CG_DrawOldScoreboard`'s choice for `listed` rows.
    pub(super) fn choose(listed: usize, small: bool, gametype: i32) -> Self {
        if listed >= MANY_CLIENTS {
            Self::Many
        } else if small || gametype == CTF || listed > LARGE_ROWS {
            Self::Small
        } else {
            Self::Large
        }
    }

    fn height(self) -> f32 {
        match self {
            Self::Large => 25.0,
            Self::Small => 15.0,
            Self::Many => 12.0,
        }
    }

    /// `CG_DrawClientScore`'s text scale.
    fn scale(self) -> f32 {
        match self {
            Self::Large => 1.0,
            Self::Small => 0.75,
            Self::Many => 0.65,
        }
    }

    /// Team band padding above and below its rows.
    fn borders(self) -> (f32, f32) {
        match self {
            Self::Large => (8.0, 8.0),
            Self::Small => (8.0, 16.0),
            Self::Many => (4.0, 8.0),
        }
    }

    fn gap(self) -> f32 {
        if self == Self::Many {
            GROUP_GAP - 8.0
        } else {
            GROUP_GAP
        }
    }
}

/// Where every listed row goes, in virtual units; fixed storage.
pub(super) struct Plan {
    pub(super) size: RowSize,
    /// `(row index, y)` in drawing order.
    pub(super) placed: [(u8, f32); 33],
    pub(super) len: usize,
    /// `(team, y, height)` of up to two team bands.
    pub(super) bands: [(u8, f32, f32); 2],
    pub(super) band_len: usize,
    /// The local row was appended below the list.
    pub(super) local_appended: bool,
}

/// Lay out the rows: groups in retail order, then your row if not reached.
pub(super) fn plan(rows: &[ScoreRow], header: &ClassicHeader<'_>, small: bool) -> Plan {
    let size = RowSize::choose(rows.len(), small, header.gametype);
    let line = size.height();
    let (top_border, bottom_border) = size.borders();
    let team_game = header.gametype >= TEAM;
    let red_leads = header.team_scores[0] >= header.team_scores[1];
    let groups: [u8; 3] = if !team_game {
        [FREE, SPECTATOR, u8::MAX]
    } else if red_leads {
        [RED, BLUE, SPECTATOR]
    } else {
        [BLUE, RED, SPECTATOR]
    };
    let mut out = Plan {
        size,
        placed: [(0, 0.0); 33],
        len: 0,
        bands: [(0, 0.0, 0.0); 2],
        band_len: 0,
        local_appended: false,
    };
    // Many clients: retail starts at `SB_TOP - 62`, under the moved-up bar's
    // lower edge; start 2 units below it instead.
    let mut y = if size == RowSize::Many {
        TOP_Y - 57.0
    } else {
        TOP_Y
    };
    if team_game {
        y += line / 2.0;
    }
    let mut local_listed = false;
    for team in groups {
        if team == u8::MAX {
            continue;
        }
        let start = out.len;
        for (index, row) in rows.iter().enumerate() {
            // Free-for-all lists everyone who is not spectating as players.
            let member = if team == FREE {
                row.team != SPECTATOR
            } else {
                row.team == team
            };
            if !member || y + line > BOTTOM_Y || out.len >= 32 {
                continue;
            }
            out.placed[out.len] = (index as u8, y);
            out.len += 1;
            local_listed |= u16::from(row.client_num) == header.local.client;
            y += line;
        }
        let count = out.len - start;
        if count == 0 {
            continue;
        }
        if (team == RED || team == BLUE) && out.band_len < 2 {
            let top = out.placed[start].1 - top_border;
            out.bands[out.band_len] = (team, top, count as f32 * line + bottom_border);
            out.band_len += 1;
        }
        y += size.gap();
    }
    // `CG_DrawOldScoreboard`: your row at the bottom when the list stops short
    // of it (not with the many-clients layout or at intermission).
    if !local_listed
        && size != RowSize::Many
        && !header.local.intermission
        && y + line <= BOTTOM_Y + GROUP_GAP
        && let Some(index) = rows
            .iter()
            .position(|row| u16::from(row.client_num) == header.local.client)
    {
        out.placed[out.len] = (index as u8, y);
        out.len += 1;
        out.local_appended = true;
    }
    out
}

/// The 640x480 virtual screen fitted to the window: heights follow the
/// window height, horizontal positions spread up to 1.25x on wide windows.
#[derive(Clone, Copy, Debug)]
struct Frame {
    left: f32,
    top: f32,
    unit: f32,
    spread: f32,
}

impl Frame {
    fn new(viewport: [f32; 2], drop: f32) -> Self {
        let unit = (viewport[1] / 480.0).max(0.01);
        let spread = (viewport[0] / (640.0 * unit)).clamp(0.5, 1.25);
        Self {
            left: (viewport[0] - 640.0 * unit * spread) * 0.5,
            top: drop * unit,
            unit,
            spread,
        }
    }

    fn x(self, v: f32) -> f32 {
        self.left + v * self.unit * self.spread
    }

    fn y(self, v: f32) -> f32 {
        self.top + v * self.unit
    }

    fn width(self, v: f32) -> f32 {
        v * self.unit * self.spread
    }

    fn height(self, v: f32) -> f32 {
        v * self.unit
    }

    fn rect(self, x: f32, y: f32, width: f32, height: f32) -> Rect {
        Rect::new(self.x(x), self.y(y), self.width(width), self.height(height))
    }

    /// Line box for a retail capital height `cap` (virtual units).
    fn text(self, cap: f32) -> f32 {
        cap / CAP * self.unit
    }
}

/// Column x positions (virtual) for one game type.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Columns {
    pub(super) name: f32,
    pub(super) score: f32,
    /// CTF captures, assists, defends.
    pub(super) ctf: Option<[f32; 3]>,
    pub(super) ping: f32,
    pub(super) time: f32,
    pub(super) id: f32,
}

impl Columns {
    pub(super) fn for_gametype(gametype: i32) -> Self {
        let at = |share: f32| LINE_X + share * LINE_WIDTH;
        if gametype == CTF {
            Self {
                name: LINE_X,
                score: at(0.47),
                ctf: Some([at(0.59), at(0.66), at(0.73)]),
                ping: at(0.80),
                time: at(0.90),
                id: at(0.97),
            }
        } else {
            Self {
                name: LINE_X,
                score: at(0.55),
                ctf: None,
                ping: at(0.70),
                time: at(0.85),
                id: at(0.97),
            }
        }
    }
}

/// Ping colour: green when good, through yellow and orange, red when poor.
pub(super) fn ping_color(ping: i32) -> Color {
    match ping {
        ..80 => Color::new(0.55, 1.0, 0.55, 1.0),
        80..150 => Color::new(1.0, 0.92, 0.45, 1.0),
        150..250 => Color::new(1.0, 0.65, 0.3, 1.0),
        _ => Color::new(1.0, 0.38, 0.35, 1.0),
    }
}

/// `CG_PlaceString`: "1st" in blue, "2nd" red, "3rd" yellow, "Tied for".
fn place(rank: u32, out: &mut impl std::fmt::Write) {
    let tied = rank & RANK_TIED != 0;
    let rank = (rank & !RANK_TIED) as i32;
    if tied {
        let _ = out.write_str("Tied for ");
    }
    let suffix = match (rank % 100, rank % 10) {
        (11..=13, _) => "th",
        (_, 1) => "st",
        (_, 2) => "nd",
        (_, 3) => "rd",
        _ => "th",
    };
    let color = match rank {
        1 => "^4",
        2 => "^1",
        3 => "^3",
        _ => "",
    };
    let _ = write!(out, "{color}{rank}{suffix}^7");
}

const WHITE: Color = Color::new(1.0, 1.0, 1.0, 1.0);

/// Build the classic board into `ui`.
#[allow(clippy::too_many_arguments)]
pub(super) fn build(
    ui: &mut MenuCanvas,
    rows: &[ScoreRow],
    header: &ClassicHeader<'_>,
    options: ClassicOptions,
    icons: &HeadIcons,
    flags: FlagIcons,
    motion: &mut Motion,
    viewport: [f32; 2],
) {
    ui.begin_transparent(viewport);
    let opacity = motion.opacity();
    let frame = Frame::new(viewport, -(1.0 - opacity) * 8.0);
    ui.push_opacity(opacity);
    let layout = plan(rows, header, options.small);
    let many = layout.size == RowSize::Many;
    let columns = Columns::for_gametype(header.gametype);
    header_lines(ui, frame, rows, header, options, many);
    header_bar(ui, frame, header, columns, options, many);
    for &(team, y, height) in &layout.bands[..layout.band_len] {
        let color = if team == RED {
            Color::new(1.0, 0.2, 0.2, 0.33)
        } else {
            Color::new(0.2, 0.2, 1.0, 0.33)
        };
        ui.accent_bar(
            frame.rect(LINE_X - 5.0, y, LINE_WIDTH + 10.0, height),
            color,
        );
    }
    let line = layout.size.height();
    for (order, &(index, target)) in layout.placed[..layout.len].iter().enumerate() {
        let row = &rows[usize::from(index)];
        let y = if layout.local_appended && order + 1 == layout.len {
            target
        } else {
            motion.row(row.client_num, target)
        };
        if order % 2 == 1 {
            ui.accent_bar(
                frame.rect(LINE_X - 5.0, y + 2.0, LINE_WIDTH + 10.0, line),
                Color::new(1.0, 1.0, 1.0, 0.035),
            );
        }
        let draw = RowDraw {
            frame,
            columns,
            size: layout.size,
            options,
            header,
            icon: icons.texture(row.client_num),
            flags,
        };
        draw.row(ui, row, y);
    }
    if rows.is_empty() {
        ui.text_aligned(
            "Requesting scores...",
            frame.rect(LINE_X, TOP_Y, LINE_WIDTH, 20.0),
            frame.text(MEDIUM_CAP * 0.9),
            Color::new(1.0, 1.0, 1.0, 0.7),
            FontWeight::Regular,
            0.0,
            TextAlign::Start,
        );
    }
    ui.pop_opacity();
}

/// "Killed by", the player count line and the place or team-lead line.
fn header_lines(
    ui: &mut MenuCanvas,
    frame: Frame,
    rows: &[ScoreRow],
    header: &ClassicHeader<'_>,
    options: ClassicOptions,
    many: bool,
) {
    let scale = if many { 0.85 } else { 1.0 };
    let size = frame.text(MEDIUM_CAP * scale);
    let first = frame.rect(0.0, if many { 2.0 } else { 40.0 }, 640.0, 22.0 * scale);
    let second = frame.rect(0.0, if many { 18.0 } else { 60.0 }, 640.0, 22.0 * scale);
    let centered = |ui: &mut MenuCanvas, args: std::fmt::Arguments<'_>, rect: Rect| {
        ui.text_fmt_aligned(
            args,
            rect,
            size,
            WHITE,
            FontWeight::Semibold,
            0.0,
            TextAlign::Center,
        );
    };
    let count = |team: u8| rows.iter().filter(|row| row.team == team).count();
    let team_game = header.gametype >= TEAM;
    if let Some(killer) = header.killer {
        centered(ui, format_args!("Killed by {killer}"), first);
    } else if team_game {
        let (red, blue) = (count(RED), count(BLUE));
        let (mine, theirs) = match header.local.team {
            BLUE => (blue, red),
            RED => (red, blue),
            _ if header.team_scores[0] < header.team_scores[1] => (blue, red),
            _ => (red, blue),
        };
        centered(ui, format_args!("{mine} vs. {theirs}"), first);
    } else if options.player_count != 0 {
        let (players, spectators) = (count(FREE), count(SPECTATOR));
        let clients = players + spectators;
        let max = header.max_clients;
        if options.player_count >= 2 {
            centered(
                ui,
                format_args!(
                    "Clients: {clients}/{max} ({players} players, {spectators} spectators)"
                ),
                first,
            );
        } else {
            centered(
                ui,
                format_args!(
                    "{} ^7{clients}/{max} ({players} players, {spectators} spectators)",
                    header.hostname
                ),
                first,
            );
        }
    }
    if header.gametype == POWER_DUEL {
        return;
    }
    if !team_game {
        if header.local.team != SPECTATOR {
            let mut placed = PlaceText::default();
            place(header.local.rank + 1, &mut placed);
            centered(
                ui,
                format_args!(
                    "{} place (of {}) with {}",
                    placed.as_str(),
                    rows.iter().filter(|row| row.has_score).count(),
                    header.local.score
                ),
                second,
            );
        }
    } else if header.gametype != SIEGE {
        let [red, blue] = header.team_scores;
        if red == blue {
            centered(ui, format_args!("Teams are tied at {red}"), second);
        } else if red > blue {
            centered(ui, format_args!("Red leads, {red} / {blue}"), second);
        } else {
            centered(ui, format_args!("Blue leads, {blue} / {red}"), second);
        }
    }
}

/// The `menu_buttonback` bar behind the column names: a dark blue band whose
/// ends fade to black, drawn from gradients.
fn header_bar(
    ui: &mut MenuCanvas,
    frame: Frame,
    header: &ClassicHeader<'_>,
    columns: Columns,
    options: ClassicOptions,
    many: bool,
) {
    let (y, height) = if many {
        (HEADER_Y - 42.0 - 5.0, 20.0)
    } else {
        (HEADER_Y - 5.0, 40.0)
    };
    let (x, width) = (LINE_X - 40.0, LINE_WIDTH + 80.0);
    let core = Color::new(0.0, 0.165, 0.39, 0.85);
    let edge = Color::new(0.0, 0.0, 0.0, 0.0);
    let gradient = |ui: &mut MenuCanvas, rect: Rect, start: Color, end: Color, vertical: bool| {
        let _ = ui.draw_list_mut().push(DrawCommand::GradientRect {
            rect,
            radius: 0.0,
            gradient: Gradient {
                start,
                end,
                vertical,
            },
        });
    };
    let fade_in = width * 0.18;
    let fade_out = width * 0.06;
    gradient(ui, frame.rect(x, y, fade_in, height), edge, core, false);
    ui.accent_bar(
        frame.rect(x + fade_in, y, width - fade_in - fade_out, height),
        core,
    );
    gradient(
        ui,
        frame.rect(x + width - fade_out, y, fade_out, height),
        core,
        edge,
        false,
    );
    let shade = Color::new(0.0, 0.0, 0.0, 0.55);
    gradient(
        ui,
        frame.rect(x, y, width, height * 0.22),
        shade,
        edge,
        true,
    );
    gradient(
        ui,
        frame.rect(x, y + height * 0.78, width, height * 0.22),
        edge,
        shade,
        true,
    );
    let scale = if many { 0.75 } else { 1.0 };
    let size = frame.text(MEDIUM_CAP * scale);
    // Centred in the bar (retail paints them along its top edge, under the
    // picture's dark rim).
    let text_y = y + (height - 22.0 * scale) * 0.5;
    let label = |ui: &mut MenuCanvas, text: &str, at: f32| {
        ui.text_aligned(
            text,
            frame.rect(at, text_y, 120.0, 22.0 * scale),
            size,
            WHITE,
            FontWeight::Semibold,
            0.0,
            TextAlign::Start,
        );
    };
    label(ui, "Name", columns.name);
    if (header.gametype == DUEL && header.fraglimit > 0) || header.gametype == POWER_DUEL {
        label(ui, "W/L", columns.score);
    } else {
        label(ui, "Score", columns.score);
    }
    if let Some([captures, assists, defends]) = columns.ctf {
        label(ui, "C", captures);
        label(ui, "A", assists);
        label(ui, "D", defends);
    }
    label(ui, "Ping", columns.ping);
    label(ui, "Time", columns.time);
    if options.client_ids {
        label(ui, "Id", columns.id);
    }
}

/// Shared per-row drawing state.
struct RowDraw<'a> {
    frame: Frame,
    columns: Columns,
    size: RowSize,
    options: ClassicOptions,
    header: &'a ClassicHeader<'a>,
    icon: Option<TextureId>,
    flags: FlagIcons,
}

impl RowDraw<'_> {
    fn row(&self, ui: &mut MenuCanvas, row: &ScoreRow, y: f32) {
        let frame = self.frame;
        let line = self.size.height();
        let scale = self.size.scale();
        let local = u16::from(row.client_num) == self.header.local.client;
        if local {
            ui.accent_bar(
                frame.rect(LINE_X - 5.0, y + 2.0, LINE_WIDTH + 10.0, line),
                self.highlight(),
            );
        }
        // Flag carriers show the flag they hold before the row.
        let flag = if row.powerups & (1 << 6) != 0 {
            self.flags.neutral
        } else if row.powerups & (1 << 4) != 0 {
            self.flags.red
        } else if row.powerups & (1 << 5) != 0 {
            self.flags.blue
        } else {
            None
        };
        if let Some(texture) = flag {
            let side = if self.size == RowSize::Large {
                line
            } else {
                line.min(15.0)
            };
            // Just left of the score line (retail's x 32 drifts far from the
            // row once positions spread on wide screens).
            let _ = ui.draw_list_mut().push(DrawCommand::TexturedQuad {
                rect: Rect::new(
                    frame.x(LINE_X - 8.0 - side / frame.spread),
                    frame.y(y + 2.0),
                    frame.height(side),
                    frame.height(side),
                ),
                texture,
                color: WHITE,
            });
        }
        let mut name_x = self.columns.name;
        if self.options.icons {
            let (side, shift) = if self.size == RowSize::Large {
                (25.0, 24.0)
            } else {
                (line.min(15.0), line.min(15.0) - 3.0)
            };
            if let Some(texture) = self.icon {
                let _ = ui.draw_list_mut().push(DrawCommand::TexturedQuad {
                    rect: Rect::new(
                        frame.x(name_x - 5.0),
                        frame.y(y + 2.0),
                        frame.height(side),
                        frame.height(side),
                    ),
                    texture,
                    color: WHITE,
                });
            }
            // A square offset, as EternalJK's `widthRatioCoef` keeps it.
            name_x += shift / frame.spread;
        }
        let name_size = frame.text(MEDIUM_CAP * 0.9 * scale);
        let number_size = frame.text(SMALL_CAP * scale);
        let text_rect = |x: f32, width: f32| frame.rect(x, y, width, line + 2.0);
        ui.text_aligned(
            &row.name,
            text_rect(name_x, self.columns.score - name_x - 8.0),
            name_size,
            WHITE,
            FontWeight::Semibold,
            0.0,
            TextAlign::Start,
        );
        let numbers = |ui: &mut MenuCanvas, args: std::fmt::Arguments<'_>, x: f32, color: Color| {
            ui.text_fmt_aligned(
                args,
                text_rect(x, 60.0),
                number_size,
                color,
                FontWeight::Regular,
                0.0,
                TextAlign::Start,
            );
        };
        let ctf = self.columns.ctf;
        let gametype = self.header.gametype;
        if !row.has_score {
            for x in [self.columns.score, self.columns.ping, self.columns.time] {
                numbers(ui, format_args!("N/A"), x, WHITE);
            }
        } else if row.ping == -1 {
            // Still connecting.
            let extra = ctf.unwrap_or([f32::NAN; 3]);
            for x in [self.columns.score, self.columns.ping, self.columns.time]
                .into_iter()
                .chain(extra.into_iter().filter(|x| !x.is_nan()))
            {
                numbers(ui, format_args!("-"), x, WHITE);
            }
        } else {
            if row.team != SPECTATOR || gametype == DUEL || gametype == POWER_DUEL {
                if (gametype == DUEL && self.header.fraglimit > 0) || gametype == POWER_DUEL {
                    let (wins, losses) = row.record.unwrap_or((0, 0));
                    numbers(
                        ui,
                        format_args!("{wins}/{losses}"),
                        self.columns.score,
                        WHITE,
                    );
                } else if let Some([captures, assists, defends]) = ctf {
                    numbers(ui, format_args!("{}", row.score), self.columns.score, WHITE);
                    numbers(ui, format_args!("{}", row.captures), captures, WHITE);
                    numbers(ui, format_args!("{}", row.assists), assists, WHITE);
                    numbers(ui, format_args!("{}", row.defends), defends, WHITE);
                } else if let Some(deaths) = row.deaths.filter(|_| gametype != DUEL) {
                    numbers(
                        ui,
                        format_args!("{}/{deaths}", row.score),
                        self.columns.score,
                        WHITE,
                    );
                } else {
                    numbers(ui, format_args!("{}", row.score), self.columns.score, WHITE);
                }
            }
            if row.bot {
                numbers(
                    ui,
                    format_args!("BOT"),
                    self.columns.ping,
                    Color::new(0.8, 0.8, 0.8, 1.0),
                );
            } else {
                numbers(
                    ui,
                    format_args!("{}", row.ping),
                    self.columns.ping,
                    ping_color(row.ping),
                );
            }
            numbers(ui, format_args!("{}", row.time), self.columns.time, WHITE);
        }
        if self.options.client_ids {
            numbers(
                ui,
                format_args!("{}", row.client_num),
                self.columns.id,
                WHITE,
            );
        }
        if self.header.local.intermission && self.header.local.ready & (1 << row.client_num) != 0 {
            ui.text_aligned(
                "READY",
                frame.rect(LINE_X - 64.0, y + 2.0, 60.0, line),
                frame.text(MEDIUM_CAP * 0.7 * scale),
                WHITE,
                FontWeight::Semibold,
                0.0,
                TextAlign::Start,
            );
        }
    }

    /// `CG_DrawClientScore`'s highlight: 1st blue, 2nd red, 3rd yellow, else
    /// grey (always grey in team games and for spectators).
    fn highlight(&self) -> Color {
        let local = self.header.local;
        let rank = if local.team == SPECTATOR || self.header.gametype >= TEAM {
            u32::MAX
        } else {
            local.rank & !RANK_TIED
        };
        let [r, g, b] = match rank {
            0 => [0.0, 0.0, 0.7],
            1 => [0.7, 0.0, 0.0],
            2 => [0.7, 0.7, 0.0],
            _ => [0.7, 0.7, 0.7],
        };
        Color::new(r, g, b, 0.7)
    }
}

/// Fixed buffer for the place text ("Tied for ^41st^7").
#[derive(Default)]
struct PlaceText {
    bytes: [u8; 32],
    len: usize,
}

impl PlaceText {
    fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..self.len]).unwrap_or("")
    }
}

impl std::fmt::Write for PlaceText {
    fn write_str(&mut self, value: &str) -> std::fmt::Result {
        let end = self.len + value.len();
        if end > self.bytes.len() {
            return Err(std::fmt::Error);
        }
        self.bytes[self.len..end].copy_from_slice(value.as_bytes());
        self.len = end;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(client: u8, team: u8) -> ScoreRow {
        ScoreRow {
            client_num: client,
            name: format!("P{client}"),
            team,
            score: 0,
            deaths: None,
            ping: 50,
            time: 1,
            has_score: true,
            bot: false,
            record: None,
            powerups: 0,
            defends: 0,
            assists: 0,
            captures: 0,
        }
    }

    fn header(gametype: i32, local: u16) -> ClassicHeader<'static> {
        ClassicHeader {
            hostname: "Host",
            max_clients: 32,
            gametype,
            fraglimit: 0,
            team_scores: [0, 0],
            local: LocalStatus {
                client: local,
                ..LocalStatus::default()
            },
            killer: None,
        }
    }

    #[test]
    fn columns_follow_the_retail_score_line() {
        let close = |actual: &[f32], expected: &[f32]| {
            assert_eq!(actual.len(), expected.len());
            for (a, e) in actual.iter().zip(expected) {
                assert!((a - e).abs() < 1e-3, "{actual:?} != {expected:?}");
            }
        };
        let ffa = Columns::for_gametype(0);
        close(
            &[ffa.name, ffa.score, ffa.ping, ffa.time, ffa.id],
            &[100.0, 342.0, 408.0, 474.0, 526.8],
        );
        assert!(ffa.ctf.is_none());
        let ctf = Columns::for_gametype(CTF);
        close(&ctf.ctf.unwrap(), &[359.6, 390.4, 421.2]);
        close(&[ctf.score, ctf.ping, ctf.time], &[306.8, 452.0, 496.0]);
    }

    #[test]
    fn row_size_follows_player_count_and_options() {
        assert_eq!(RowSize::choose(12, false, 0), RowSize::Large);
        assert_eq!(RowSize::choose(13, false, 0), RowSize::Small);
        assert_eq!(RowSize::choose(4, true, 0), RowSize::Small);
        assert_eq!(RowSize::choose(4, false, CTF), RowSize::Small);
        assert_eq!(RowSize::choose(20, false, 0), RowSize::Many);
    }

    #[test]
    fn free_for_all_lists_players_then_spectators() {
        let rows = [row(0, FREE), row(4, SPECTATOR), row(2, FREE)];
        let plan = plan(&rows, &header(0, 2), false);
        let order: Vec<_> = plan.placed[..plan.len].iter().map(|p| p.0).collect();
        assert_eq!(order, [0, 2, 1]);
        assert_eq!(plan.placed[0].1, TOP_Y);
        assert_eq!(plan.placed[1].1, TOP_Y + 25.0);
        // Spectators after the group gap; no bands in free-for-all.
        assert_eq!(plan.placed[2].1, TOP_Y + 50.0 + GROUP_GAP);
        assert_eq!(plan.band_len, 0);
        assert!(!plan.local_appended);
    }

    #[test]
    fn team_games_list_the_leader_first_over_bands() {
        let rows = [row(0, RED), row(1, BLUE), row(2, BLUE), row(3, SPECTATOR)];
        let mut header = header(TEAM, 0);
        header.team_scores = [1, 5];
        let plan = plan(&rows, &header, false);
        let order: Vec<_> = plan.placed[..plan.len].iter().map(|p| p.0).collect();
        assert_eq!(order, [1, 2, 0, 3]);
        assert_eq!(plan.band_len, 2);
        let (team, top, height) = plan.bands[0];
        assert_eq!(
            (team, top, height),
            (BLUE, TOP_Y + 12.5 - 8.0, 2.0 * 25.0 + 8.0)
        );
        assert_eq!(plan.bands[1].0, RED);
    }

    #[test]
    fn many_clients_shrink_and_move_up() {
        let rows: Vec<_> = (0..24).map(|client| row(client, FREE)).collect();
        let plan = plan(&rows, &header(0, 0), false);
        assert_eq!(plan.size, RowSize::Many);
        assert_eq!(plan.len, 24);
        assert_eq!(plan.placed[0].1, TOP_Y - 57.0);
        assert!(plan.placed[23].1 + 12.0 <= BOTTOM_Y);
    }

    #[test]
    fn ids_and_pings_and_places() {
        assert_eq!(ping_color(40), Color::new(0.55, 1.0, 0.55, 1.0));
        assert_eq!(ping_color(400), Color::new(1.0, 0.38, 0.35, 1.0));
        let text = |rank| {
            let mut out = PlaceText::default();
            place(rank, &mut out);
            out.as_str().to_owned()
        };
        assert_eq!(text(1), "^41st^7");
        assert_eq!(text(2), "^12nd^7");
        assert_eq!(text(12), "12th^7");
        assert_eq!(text(22), "22nd^7");
        assert_eq!(text(1 | RANK_TIED), "Tied for ^41st^7");
    }
}
