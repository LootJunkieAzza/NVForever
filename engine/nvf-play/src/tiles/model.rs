//! The tile HUD's `nif` tiles: a model drawn in the menu (T12 leftovers, hud.md §14.12).
//!
//! - **Which models**: a model of flat pieces, read by the hand-built breath meter's reader
//!   (`crate::hud_breath::MeterModel::read_sequence`: each textured square of the model's
//!   x–z plane a picture turned to its angle). It is not the design note's "drawn to a
//!   texture" (hud-xml.md §5): no 3D model is rendered; a model with no flat pieces draws
//!   nothing and says so once [design]. The one vanilla `nif` tile, `BreathMeter`
//!   (`Interface\HUD\AirTimer01.NIF`), is such a model [observed: hud_breath.rs reads its
//!   17 pieces].
//! - **The file**: the tile's `filename` under `meshes\` ([`model_path`]) [observed: the
//!   hand-built meter reads `meshes\interface\hud\airtimer01.nif` for the file's
//!   `Interface\HUD\AirTimer01.NIF`; hypothesis: every `nif` tile's model is found so].
//! - **Where** ([`place`]): the model's origin at the tile's `x`, `y`, one model unit an
//!   interface unit, the model's z up the screen [hypothesis: `BreathMeter`'s read x is
//!   W/2 (hud.md §14) where the original's meter stands centred across the view, and the
//!   model's 16 ticks reach about 30 units round its origin, filling the tile's 64 x 64;
//!   the hand-built meter instead fits the model's bounds in its own box]. Not cut to the
//!   tile's box [design].
//! - **The sequence**: the tile's `animation` names it [documented: hud_main_menu.xml,
//!   `Forward`]; how far into it the model stands is [`FRACTION`], NVForever's own trait
//!   (0 the start, 1 the end; `super::vanilla::write_breath` writes the breath used). A
//!   node with vis keys shows as they say at that time (`MeterModel::lit`), the others
//!   always. [`animate`] sets the pieces each frame without spawning them again.
//! - Colour and alpha are the tile's, as every tile's (`super::draw::pieces`).

use crate::hud_breath::{place_in, spawn_pieces, MeterModel, Piece as ModelPiece};
use crate::script::ScriptState;
use bevy::prelude::*;
use fnv_ui::{TileId, Ui, Value};
use nvf_assets::GameData;
use std::collections::HashMap;
use std::sync::Arc;

/// The trait holding how far into its `animation` a `nif` tile's model stands, 0 to 1
/// [design: NVForever's own; the original's code drives the sequence in a way no trait of
/// the file shows].
pub const FRACTION: &str = "_NVFSequenceFraction";

/// A `nif` tile's model file: its `filename` under `meshes\`, in small letters with
/// backslashes.
pub fn model_path(filename: &str) -> String {
    format!("meshes\\{}", filename.trim().replace('/', "\\").trim_start_matches('\\').to_ascii_lowercase())
}

/// How far into its sequence a tile's model stands ([`FRACTION`], 0 without it).
pub fn fraction(ui: &Ui, tile: TileId) -> f64 {
    ui.tile(tile).get(FRACTION).map(Value::num).filter(|f| f.is_finite()).map_or(0.0, |f| f.clamp(0.0, 1.0))
}

/// The sequence's time for a fraction of it, seconds.
pub fn sequence_time(model: &MeterModel, fraction: f64) -> f32 {
    fraction.clamp(0.0, 1.0) as f32 * model.length
}

/// Which of the model's pieces show at a fraction of its sequence, in the model's order.
pub fn shown_pieces(model: &MeterModel, fraction: f64) -> Vec<bool> {
    let t = sequence_time(model, fraction);
    model.pieces.iter().map(|p| model.lit(&p.node, t)).collect()
}

/// A piece's centre and size in its tile's box, interface units from the tile's top left
/// ([hypothesis]: the module's "Where").
pub fn place(piece: &ModelPiece) -> (Vec2, Vec2) {
    place_in(piece, Vec2::ZERO, Vec2::ZERO, 1.0)
}

/// The models read so far, by file and sequence; `None` for one that could not be read.
#[derive(Default)]
pub(crate) struct Models(HashMap<(String, Option<String>), Option<Arc<MeterModel>>>);

impl Models {
    /// A tile's model, read once (and told once).
    pub(crate) fn get(&mut self, filename: &str, animation: Option<&str>, data: &GameData) -> Option<Arc<MeterModel>> {
        let key = (model_path(filename), animation.map(str::to_string));
        self.0
            .entry(key)
            .or_insert_with_key(|(path, sequence)| {
                let read = data.read(path).ok_or_else(|| "not found".to_string()).and_then(|bytes| nvf_assets::guard::guarded(|| MeterModel::read_sequence(&bytes, &data.schema, sequence.as_deref())));
                match read {
                    Ok(m) => {
                        info!("tiles: model {path} read: {} pieces, sequence {} {:.3} s", m.pieces.len(), sequence.as_deref().unwrap_or("none"), m.length);
                        Some(Arc::new(m))
                    }
                    Err(e) => {
                        warn!("tiles: model {path}: {e}; its tile draws nothing");
                        None
                    }
                }
            })
            .clone()
    }
}

/// A model ready to spawn: the model, its pieces' textures by path and the sequence's
/// time it starts at.
pub(crate) struct Prepared {
    pub model: Arc<MeterModel>,
    pub textures: HashMap<String, Option<(Handle<Image>, Vec2)>>,
    pub fraction: f64,
}

/// A drawn `nif` tile's node, its pieces under it ([`ModelPart`]).
#[derive(Component)]
pub(crate) struct TileModel {
    tile: TileId,
    model: Arc<MeterModel>,
}

/// One of a drawn model's pieces: the node it hangs under in the model.
#[derive(Component)]
pub(crate) struct ModelPart(String);

/// Spawn a `nif` tile's node (`node`, the tile's box) and its pieces in `colour`.
pub(crate) fn spawn(p: &mut ChildSpawnerCommands, node: Node, tile: TileId, prepared: Prepared, colour: Color) -> Entity {
    let Prepared { model, textures, fraction } = prepared;
    let t = sequence_time(&model, fraction);
    let mut texture = |path: &str| textures.get(path).cloned().flatten();
    p.spawn((node, TileModel { tile, model: model.clone() }, Pickable::IGNORE))
        .with_children(|c| spawn_pieces(c, &model, place, colour, &mut texture, |piece| ModelPart(piece.node.clone()), |piece| model.lit(&piece.node, t)))
        .id()
}

/// Show each drawn model's pieces at its tile's place in its sequence ([`FRACTION`]), and
/// tell how many show when that changes.
pub(crate) fn animate(script: Res<ScriptState>, models: Query<(&TileModel, &Children)>, mut parts: Query<(&ModelPart, &mut Visibility)>, mut told: Local<HashMap<TileId, usize>>) {
    let Some(ui) = script.rt.as_ref().map(|rt| &rt.game.ui_tree) else { return };
    let mut seen = Vec::new();
    for (drawn, children) in &models {
        if !ui.tile(drawn.tile).live {
            continue;
        }
        let fraction = fraction(ui, drawn.tile);
        let t = sequence_time(&drawn.model, fraction);
        let (mut shown, mut total) = (0, 0);
        for child in children.iter() {
            let Ok((part, mut vis)) = parts.get_mut(child) else { continue };
            let on = drawn.model.lit(&part.0, t);
            vis.set_if_neq(if on { Visibility::Inherited } else { Visibility::Hidden });
            shown += usize::from(on);
            total += 1;
        }
        if told.get(&drawn.tile) != Some(&shown) {
            info!("tiles: model {}: {shown} of {total} pieces shown ({t:.3} of {:.3} s)", ui.tile(drawn.tile).name, drawn.model.length);
            told.insert(drawn.tile, shown);
        }
        seen.push(drawn.tile);
    }
    told.retain(|tile, _| seen.contains(tile));
}

#[cfg(test)]
mod tests {
    use super::super::draw::{pieces, Look, Select, HUD_MENU};
    use super::super::layout::{self, Kind, Rect as TileRect};
    use super::super::vanilla::{kept_pieces, write_breath};
    use super::*;
    use fnv_ui::MemSource;

    fn plain(_: f64, text: &str, _: Option<f64>) -> (f64, f64) {
        (10.0 * text.chars().count() as f64, 20.0)
    }

    /// [design] The HUD's `BreathMeter` as the file has it (hud_main_menu.xml, read with
    /// `fnvtool ui tree`), at the place T4 writes (W/2, 110 at 16:10).
    fn hud() -> (Ui, TileId) {
        let src = MemSource::default().with(
            "hud.xml",
            r#"<menu name="HUDMainMenu"><visible>&false;</visible>
              <nif name="BreathMeter"><filename>Interface\HUD\AirTimer01.NIF</filename><animation>Forward</animation>
                <visible>&false;</visible><alpha>0</alpha><width>64</width><height>64</height><systemcolor>&hudmain;</systemcolor></nif></menu>"#,
        );
        let mut ui = Ui::new();
        let menu = ui.load_menu(&src, "hud.xml").unwrap();
        ui.set_float("HUDMainMenu/visible", 1.0);
        let tile = ui.find_tile("HUDMainMenu/BreathMeter").unwrap();
        ui.set_trait(tile, "x", Value::Num(768.0));
        ui.set_trait(tile, "y", Value::Num(110.0));
        ui.set_trait(tile, "visible", Value::Num(8192.0));
        ui.update();
        (ui, menu)
    }

    fn square(node: &str, centre: Vec2) -> ModelPiece {
        ModelPiece { node: node.into(), centre, size: Vec2::splat(10.0), angle: 0.0, uv: Rect::new(0.0, 0.0, 1.0, 1.0), texture: "textures\\interface\\hud\\air_meter.dds".into() }
    }

    /// [design] A meter like AirTimer01's: the word and two ticks, the first going off at
    /// frame 1 and the second at frame 2 of a two-frame sequence (the vis keys' shape,
    /// hud_breath.rs).
    fn meter() -> MeterModel {
        MeterModel {
            pieces: vec![square("O2", Vec2::new(3.5, -1.8)), square("Tick01", Vec2::new(10.0, 25.0)), square("Tick02", Vec2::new(25.0, 10.0))],
            keys: vec![("Tick01".into(), vec![(0.0, true), (1.0 / 30.0, false)]), ("Tick02".into(), vec![(0.0, true), (2.0 / 30.0, false)])],
            length: 2.0 / 30.0,
            bounds: Rect::new(-30.0, -30.0, 30.0, 30.0),
        }
    }

    #[test]
    fn a_nif_tile_names_its_model_under_meshes_and_its_sequence() {
        assert_eq!(model_path("Interface\\HUD\\AirTimer01.NIF"), crate::hud_breath::MODEL);
        assert_eq!(model_path(" /Interface/HUD/x.nif "), "meshes\\interface\\hud\\x.nif");
        let (ui, menu) = hud();
        let items = layout::layout(&ui, menu, &plain, false);
        let item = items.iter().find(|i| ui.tile(i.tile).name == "BreathMeter").unwrap();
        assert_eq!(item.kind, Kind::Nif(layout::Nif { filename: "Interface\\HUD\\AirTimer01.NIF".into(), animation: Some("Forward".into()) }));
        assert_eq!(item.rect, TileRect { left: 768.0, top: 110.0, width: 64.0, height: 64.0 });
    }

    #[test]
    fn the_breath_meter_draws_from_its_tile_only_while_written_shown_with_the_switch_on() {
        let (mut ui, menu) = hud();
        let whole = |ui: &Ui| pieces(ui, menu, &plain, Select::Whole { kept: &kept_pieces(ui), strip: None }, false).0;
        // The file's alpha 0: nothing, as before any write.
        assert!(whole(&ui).is_empty());
        write_breath(&mut ui, true, 0.5);
        ui.update();
        let drawn = whole(&ui);
        assert_eq!(drawn.len(), 1);
        assert_eq!(drawn[0].look, Look::Model { filename: "Interface\\HUD\\AirTimer01.NIF".into(), animation: Some("Forward".into()) });
        assert_eq!(drawn[0].rect, TileRect { left: 768.0, top: 110.0, width: 64.0, height: 64.0 });
        // In the HUD's colour, at the alpha written.
        let [r, g, b] = crate::interface::hud_rgb().map(|c| f32::from(c) / 255.0);
        assert_eq!(drawn[0].colour, [r, g, b, 1.0]);
        // No longer kept hand-built.
        assert!(kept_pieces(&ui).is_empty());
        // With the switch off only the mods' tiles draw, and a nif tile never did.
        assert!(pieces(&ui, menu, &plain, Select::Mods { vanilla: None }, false).0.is_empty());
        // Hidden again.
        write_breath(&mut ui, false, 0.0);
        ui.update();
        assert!(whole(&ui).is_empty());
        // The fraction moves without changing the piece: no new nodes as the breath runs.
        write_breath(&mut ui, true, 0.1);
        ui.update();
        let a = whole(&ui);
        write_breath(&mut ui, true, 0.9);
        ui.update();
        assert_eq!(a, whole(&ui));
        assert_eq!(fraction(&ui, a[0].tile), 0.9);
    }

    #[test]
    fn the_model_stands_on_the_tiles_place_a_unit_a_unit() {
        // The word's square centred 3.5 right of and 1.8 under the model's origin, the
        // origin at the tile's top left (x W/2, y 110): the meter centred across.
        let m = meter();
        let (centre, size) = place(&m.pieces[0]);
        assert!((centre - Vec2::new(3.5, 1.8)).length() < 1e-6);
        assert_eq!(size, Vec2::splat(10.0));
        // Model z up, the screen's y down.
        assert!(place(&m.pieces[1]).0.y < 0.0);
    }

    #[test]
    fn the_ticks_go_out_as_the_fraction_written_grows() {
        let m = meter();
        assert_eq!(shown_pieces(&m, 0.0), [true, true, true]);
        // Past frame 1: the first tick out, the word always shown (no keys).
        assert_eq!(shown_pieces(&m, 0.6), [true, false, true]);
        assert_eq!(shown_pieces(&m, 1.0), [true, false, false]);
        // Held to 0..=1; a model without its sequence shows every piece.
        assert_eq!(shown_pieces(&m, 7.0), [true, false, false]);
        let still = MeterModel { keys: Vec::new(), length: 0.0, ..meter() };
        assert_eq!(shown_pieces(&still, 1.0), [true, true, true]);
        // The breath used, as the hand-built meter's time (hud_breath::time_at).
        for share in [1.0_f32, 0.75, 0.3, 0.0] {
            assert!((sequence_time(&m, 1.0 - f64::from(share)) - crate::hud_breath::time_at(share, m.length)).abs() < 1e-6);
        }
        // No trait: the start.
        let (ui, _) = hud();
        let tile = ui.find_tile(&format!("{HUD_MENU}/BreathMeter")).unwrap();
        assert_eq!(fraction(&ui, tile), 0.0);
    }
}
