//! The vehicle table: which model and skin a `$<vehicle>` NPC wears.
//!
//! `BG_VehicleLoadParms` (`bg_vehicleLoad.c`) concatenates every `ext_data/vehicles/*.veh`
//! and `BG_VehicleGetIndex` finds a vehicle by its `name` key without regard to case.
//! cgame then loads `models/players/<model>/model.glm` with `model_<skin>.skin`, or
//! `model_default.skin` when the vehicle names no skin (`cg_players.c:8782-8791`).
use jkr_vfs::VirtualFileSystem;
use std::collections::HashMap;

/// A vehicle's `type` key, which decides how much of its angles the model root takes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum VehicleKind {
    /// Animals, fliers and untyped vehicles: upright, like any other non-humanoid.
    #[default]
    Upright,
    Speeder,
    Fighter,
    Walker,
}

/// Model directory, skin variant and kind of one vehicle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VehicleLook {
    pub(crate) directory: String,
    pub(crate) variant: String,
    pub(crate) kind: VehicleKind,
}

/// The look of vehicle `name`, if any mounted `.veh` file defines it. A dozen small text
/// files are read when a vehicle is loaded, which happens when one first appears, never
/// per frame; every actor loading path goes through here, so none needs a table of its own.
pub(crate) fn look(vfs: &VirtualFileSystem, name: &str) -> Option<VehicleLook> {
    let mut table = HashMap::new();
    for path in vfs.paths() {
        let lower = path.as_str().to_ascii_lowercase();
        if !lower.starts_with("ext_data/vehicles/")
            || !lower.ends_with(".veh")
            || lower["ext_data/vehicles/".len()..].contains('/')
        {
            continue;
        }
        if let Ok(Some(asset)) = vfs.read(path.as_str()) {
            parse(&String::from_utf8_lossy(&asset.bytes), &mut table);
        }
    }
    table.remove(&name.to_ascii_lowercase())
}

/// Add the vehicles of one `.veh` text: `Block { key value ... }`, `//` comments, quoted
/// values. The first definition of a name wins, as the first index match does in stock.
fn parse(text: &str, table: &mut HashMap<String, VehicleLook>) {
    let mut tokens = Vec::new();
    for line in text.lines() {
        let line = line.split("//").next().unwrap_or("");
        let mut rest = line.trim_start();
        while !rest.is_empty() {
            let (token, after) = match rest.strip_prefix('"') {
                Some(quoted) => quoted.split_once('"').unwrap_or((quoted, "")),
                None => rest.split_at(rest.find(char::is_whitespace).unwrap_or(rest.len())),
            };
            tokens.push(token);
            rest = after.trim_start();
        }
    }
    let mut index = 0;
    while index + 1 < tokens.len() {
        if tokens[index + 1] != "{" {
            index += 1;
            continue;
        }
        let block = tokens[index];
        let (mut name, mut model, mut skin, mut kind) = (block, "", "", VehicleKind::default());
        index += 2;
        while index < tokens.len() && tokens[index] != "}" {
            let value = tokens.get(index + 1).copied().unwrap_or("");
            match tokens[index].to_ascii_lowercase().as_str() {
                "name" => name = value,
                "model" => model = value,
                "skin" => skin = value,
                "type" => {
                    kind = match value.to_ascii_uppercase().as_str() {
                        "VH_SPEEDER" => VehicleKind::Speeder,
                        "VH_FIGHTER" => VehicleKind::Fighter,
                        "VH_WALKER" => VehicleKind::Walker,
                        _ => VehicleKind::Upright,
                    }
                }
                _ => {}
            }
            index += 2;
        }
        index += 1;
        if model.is_empty() {
            continue;
        }
        // A `|` list registers as a multi-part skin whose first part is `model_<first>`;
        // the other parts name files vehicles do not have, so the first is what shows.
        let variant = skin
            .split('|')
            .next()
            .filter(|first| !first.is_empty())
            .unwrap_or("default");
        table
            .entry(name.to_ascii_lowercase())
            .or_insert_with(|| VehicleLook {
                directory: format!("models/players/{model}"),
                variant: variant.to_owned(),
                kind,
            });
    }
}
