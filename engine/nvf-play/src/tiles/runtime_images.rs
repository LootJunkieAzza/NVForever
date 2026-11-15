//! Pictures made at run time for `image` tiles: a texture NVForever's code makes (the Local
//! Map's outline picture, `crate::pipboy::data::outline`) drawn where a menu file has an
//! image whose picture the original's code sets [design: docs/design/ui-tile-tree.md §6.1,
//! docs/spec-notes/menu-xml.md §3.1.5].
//!
//! - **The name**: a tile's `filename` that starts with [`PREFIX`] (`nvf:`, any case) names an
//!   entry of [`RuntimeImages`] instead of a game file. No game path starts so: the menu
//!   files name pictures under `textures\` by relative paths, and in the Windows paths the
//!   archives and the Data folder hold a colon only follows a drive letter [design]. The
//!   name is resolved before the atlas indexes and the files are looked in, and never enters
//!   the file-name cache (`crate::interface::Pictures`).
//! - **Repainting**: each [`RuntimeImages::set`] (and [`RuntimeImages::touch`]) is a new
//!   revision. The renderer stamps a runtime picture's piece with its entry's revision and
//!   size before comparing the frame's pieces with the last ones ([`mark`]), so a new handle,
//!   the same handle's texels changed, an entry that comes late, goes, or changes size
//!   repaints the piece although its name stays the same.
//! - **Not read from a runtime picture** [design]: the `tile` repeats are not baked into one
//!   image (`crate::repeat` reads files): Bevy's own tiles draw them, as for a picture whose
//!   texels cannot be read; SetUITexOffset and SetUIAlphaMap (`super::texmod`, which read the
//!   texels from files) are not applied; each said once in the log.
//! - A name with no entry draws nothing and says so once.
//!
//! The way code uses it: `let name = images.set("local_map", handle, size);` then the tile's
//! `filename` written to `name` (`nvf:local_map`); set the entry before writing the name, or
//! the first frame says the entry is missing.

use super::draw::{Look, Piece, Source};
use crate::interface::SizedPicture;
use fnv_ui::{TileId, Ui, Value};
use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

/// The `filename` prefix of a runtime picture [design: see the module notes].
pub const PREFIX: &str = "nvf:";

/// Whether a tile's `filename` names a runtime picture ([`PREFIX`], any case, spaces around
/// ignored as the renderer trims them).
pub fn is_runtime(filename: &str) -> bool {
    filename.trim().get(..PREFIX.len()).is_some_and(|p| p.eq_ignore_ascii_case(PREFIX))
}

/// The `filename` a tile is given for the runtime picture `name` (with or without the
/// prefix).
pub fn filename(name: &str) -> String {
    let name = name.trim();
    if is_runtime(name) { name.to_string() } else { format!("{PREFIX}{name}") }
}

/// The key an entry is kept by: the whole filename, in small letters.
fn key(name: &str) -> String {
    filename(name).to_ascii_lowercase()
}

/// Explicit presentation of generated pixels for one actual recipient generation.
/// This is engine-owned metadata, not a change to the XML's generic turn policy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RecipientTurn {
    pub recipient: TileId,
    pub angle: f64,
}

/// Shared admission for a generated leaf's resolved image geometry. Width/height
/// are the image layout's numeric traits (layout::place); the bounds also keep
/// compensation finite on every current normal/projected target scale<=960.
pub(super) fn presentation_geometry(width:f64,height:f64,pivot:[f64;2])->bool {
    [width,height].into_iter().all(|n|n.is_finite() && n>0. && n<=65536.)
        && pivot.into_iter().all(|n|n.is_finite() && n.abs()<=65536.)
}

/// One runtime picture: its texture, its size in texels and its revision.
#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeImage {
    pub image: Handle<Image>,
    pub size: Vec2,
    pub revision: u64,
    pub presentation: Option<RecipientTurn>,
}

/// What a runtime picture's piece is compared by from frame to frame ([`mark`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stamp {
    pub revision: u64,
    pub size: Vec2,
}

/// The runtime pictures by name, set by the code that makes them.
#[derive(Resource, Default, Debug)]
pub struct RuntimeImages {
    entries: HashMap<String, RuntimeImage>,
    /// The last revision given, over every name, so an entry removed and set again never
    /// gets a revision it had.
    revision: u64,
}

impl RuntimeImages {
    fn next(&mut self) -> u64 {
        self.revision += 1;
        self.revision
    }

    /// Give `name` (with or without [`PREFIX`]) the texture `image` of `size` texels (the
    /// image's own size: a `tile` picture's repeats are as large as Bevy finds the texture),
    /// and return the `filename` to write into the tile. Each call is a new revision: the piece
    /// repaints even when the handle is the same one.
    pub fn set(&mut self, name: &str, image: Handle<Image>, size: Vec2) -> String {
        let revision = self.next();
        self.entries.insert(key(name), RuntimeImage { image, size, revision, presentation: None });
        filename(name)
    }

    /// The generated raster stays WorldNorth. Present it in the explicitly
    /// configured orthonormal marker frame without changing generic XML angles.
    pub fn set_recipient_axes(&mut self, name: &str, recipient: TileId, cos: f64, sin: f64) -> bool {
        if ![cos,sin].into_iter().all(f64::is_finite) || (cos*cos+sin*sin-1.).abs() > 1e-9 { return false; }
        self.set_recipient_turn(name, recipient, -sin.atan2(cos))
    }

    /// Present this generated entry in a declared marker frame, scoped to a full
    /// recipient ID. The caller retains/checks the complete entry as its ownership
    /// token. Only the piece turn changes: no texture revision or XML trait write.
    fn set_recipient_turn(&mut self, name: &str, recipient: TileId, angle: f64) -> bool {
        if !angle.is_finite() || angle.abs() > std::f64::consts::TAU { return false; }
        let Some(entry) = self.entries.get_mut(&key(name)) else { return false };
        entry.presentation = Some(RecipientTurn { recipient, angle });
        true
    }

    /// The texels of `name`'s texture changed in place (the same handle): a new revision.
    /// False when there is no such entry.
    pub fn touch(&mut self, name: &str) -> bool {
        let revision = self.next();
        match self.entries.get_mut(&key(name)) {
            Some(entry) => {
                entry.revision = revision;
                true
            }
            None => false,
        }
    }

    /// Take `name`'s entry away: its tiles draw nothing. False when there was none.
    pub fn remove(&mut self, name: &str) -> bool {
        self.entries.remove(&key(name)).is_some()
    }

    /// The entry a tile's `filename` names.
    pub fn get(&self, filename: &str) -> Option<&RuntimeImage> {
        self.entries.get(&key(filename))
    }

    /// Whether any runtime name still retains this image. An owner dropping its own
    /// name must not delete the pixels of an independent runtime alias [design].
    pub fn uses_image(&self, image: &Handle<Image>) -> bool {
        self.entries.values().any(|entry| entry.image == *image)
    }

    /// What a piece of `filename` is compared by: none without an entry.
    pub fn stamp(&self, filename: &str) -> Option<Stamp> {
        self.get(filename).map(|e| Stamp { revision: e.revision, size: e.size })
    }
}

/// What a drawing has said once about its runtime pictures (one [`super::draw`] drawing's).
#[derive(Default, Debug)]
pub struct Told {
    missing: HashSet<String>,
    repeats: HashSet<String>,
    texmod: HashSet<String>,
}

impl Told {
    /// Whether the missing entry `name` was said.
    pub fn missing(&self, name: &str) -> bool {
        self.missing.contains(&key(name))
    }

    /// Whether `name`'s unbaked repeats were said.
    pub fn repeats(&self, name: &str) -> bool {
        self.repeats.contains(&key(name))
    }

    /// Whether `name`'s dropped texture offset or alpha map was said.
    pub fn texmod(&self, name: &str) -> bool {
        self.texmod.contains(&key(name))
    }
}

/// Stamp each runtime picture's piece with its entry's revision and size (none without one),
/// so a change repaints it, and drop a texture offset or alpha map from it, saying so once
/// ([`super::texmod`] reads files). Run before the pieces are compared with the last frame's.
pub fn mark(pieces: &mut [Piece], images: Option<&RuntimeImages>, told: &mut Told) {
    for p in pieces {
        let Look::Picture { name, source: Source::Runtime(stamp), .. } = &mut p.look else { continue };
        *stamp = images.and_then(|i| i.stamp(name));
        if p.tex.take().is_some() && told.texmod.insert(key(name)) {
            warn!("tiles: the runtime picture {name} takes no texture offset or alpha map (they are read from files); drawn without");
        }
    }
}

/// Apply optional generated-picture presentation after marking and before comparing
/// final pieces. `mark` retains its existing slice API and stamp/texture policy. Only
/// the exact declared recipient generation is affected; aliases keep their XML turn.
/// Current target units must already be scoped as they are for actual painting.
pub fn present(pieces: &mut Vec<Piece>, images: Option<&RuntimeImages>, ui: Option<&Ui>) {
    pieces.retain_mut(|p| {
        let Look::Picture { name, source: Source::Runtime(_), .. } = &p.look else { return true };
        let Some(turn) = images.and_then(|i| i.get(name)).and_then(|i| i.presentation).filter(|t| t.recipient == p.tile) else { return true };
        let Some(ui) = ui else { return false };
        let tile = ui.tile(p.tile);
        if !tile.live || tile.kind != "image" || tile.children.iter().any(|&c| ui.tile(c).live) { return false; }
        let axis = |name| match tile.get(name) { Some(Value::Num(n)) if n.is_finite() && n.abs() <= 65536. => Some(*n), _ => None };
        let (Some(x), Some(y)) = (axis("rotateaxisx"), axis("rotateaxisy")) else { return false };
        let scale = crate::interface::unit_scale();
        if !turn.angle.is_finite() || turn.angle.abs() > std::f64::consts::TAU
            || !presentation_geometry(p.rect.width, p.rect.height, [x, y])
            || !scale.is_finite() || scale <= 0. || scale > 960. { return false; }
        p.turn = (turn.angle != 0.).then_some(super::layout::Turn { angle: turn.angle, pivot: (x, y) });
        true
    });
}

/// The picture of a runtime piece named `name`: its texture, no atlas cut, its size. None
/// for a name with no entry, said once.
pub fn picture(images: Option<&RuntimeImages>, name: &str, told: &mut Told) -> Option<SizedPicture> {
    match images.and_then(|i| i.get(name)) {
        Some(entry) => Some((entry.image.clone(), None, entry.size)),
        None => {
            if told.missing.insert(key(name)) {
                warn!("tiles: no runtime picture {name} is set; its tile draws nothing until one is");
            }
            None
        }
    }
}

/// Say once that a runtime picture's `tile` repeats are drawn by Bevy's tiles, not baked.
pub fn unbaked(name: &str, told: &mut Told) {
    if told.repeats.insert(key(name)) {
        info!("tiles: the runtime picture {name} is repeated by Bevy's tiles (only files' repeats are baked into one image)");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prefix_names_a_runtime_picture_in_any_case() {
        assert!(is_runtime("nvf:local_map") && is_runtime(" NVF:Local_Map ") && !is_runtime("Interface\\nvf.dds") && !is_runtime("nv"));
        assert_eq!(filename("local_map"), "nvf:local_map");
        assert_eq!(filename(" nvf:local_map "), "nvf:local_map");
    }

    #[test]
    fn every_set_is_a_new_revision_and_a_removed_name_never_gets_an_old_one() {
        let mut images = RuntimeImages::default();
        let (a, b) = (Handle::<Image>::default(), Handle::<Image>::default());
        assert_eq!(images.set("map", a.clone(), Vec2::new(64.0, 32.0)), "nvf:map");
        let first = images.stamp("NVF:Map").unwrap();
        images.set("nvf:map", b, Vec2::new(64.0, 32.0));
        let second = images.stamp("nvf:map").unwrap();
        assert!(second.revision > first.revision);
        assert!(images.touch("map"));
        assert!(images.stamp("nvf:map").unwrap().revision > second.revision);
        assert!(images.remove("map") && images.stamp("nvf:map").is_none() && !images.remove("map") && !images.touch("map"));
        images.set("map", a, Vec2::new(64.0, 32.0));
        assert!(images.stamp("nvf:map").unwrap().revision > second.revision + 1);
    }

    #[test]
    fn a_missing_entry_draws_nothing_and_is_told_once() {
        let mut told = Told::default();
        assert!(picture(None, "nvf:gone", &mut told).is_none());
        assert!(told.missing("nvf:gone"));
        let images = RuntimeImages::default();
        assert!(picture(Some(&images), "NVF:Gone", &mut told).is_none());
        assert_eq!(told.missing.len(), 1, "said once, whatever the case");
        let mut images = images;
        let h = Handle::<Image>::default();
        images.set("gone", h.clone(), Vec2::new(8.0, 4.0));
        assert_eq!(picture(Some(&images), "nvf:gone", &mut told), Some((h, None, Vec2::new(8.0, 4.0))));
    }
    fn ui_for_presentation()->(Ui,TileId,TileId,TileId) {
        let src=fnv_ui::MemSource::default().with("owned.xml",r#"<menu name="Owned"><image name="A"><width>20</width><height>20</height><filename>nvf:one</filename><rotateangle>1</rotateangle><rotateaxisx>10</rotateaxisx><rotateaxisy>10</rotateaxisy></image><image name="B"><width>20</width><height>20</height><filename>nvf:one</filename><rotateangle>1</rotateangle><rotateaxisx>10</rotateaxisx><rotateaxisy>10</rotateaxisy></image></menu>"#);
        let mut ui=Ui::new();let root=ui.load_menu(&src,"owned.xml").unwrap();ui.update();let a=ui.find_tile("Owned/A").unwrap();let b=ui.find_tile("Owned/B").unwrap();(ui,root,a,b)
    }
    #[test] fn presentation_is_scoped_and_does_not_change_texture_revision_or_xml() {
        let (ui,root,a,b)=ui_for_presentation();let mut images=RuntimeImages::default();images.set("one",Handle::default(),Vec2::splat(2.));let stamp=images.stamp("one");
        assert!(images.set_recipient_axes("one",a,1.,0.));assert_eq!(images.stamp("one"),stamp);
        let (mut pieces,_) = super::super::draw::pieces(&ui,root,&|_,_,_|(0.,0.),super::super::draw::Select::Whole {kept:&[],strip:None},true);
        mark(&mut pieces,Some(&images),&mut Told::default());present(&mut pieces,Some(&images),Some(&ui));
        assert_eq!(pieces.iter().find(|p|p.tile==a).unwrap().turn,None);
        assert_eq!(pieces.iter().find(|p|p.tile==b).unwrap().turn.unwrap().angle,1.,"same runtime filename gives no authority over another TileId");
        assert_eq!(ui.get_float("Owned/A/rotateangle"),Some(1.));assert_eq!(ui.get_float("Owned/B/rotateangle"),Some(1.));
    }
    #[test] fn ordinary_foreign_set_releases_presentation_while_invalid_axes_leave_it_unchanged() {
        let (ui,_,a,_)=ui_for_presentation();let mut images=RuntimeImages::default();images.set("one",Handle::default(),Vec2::splat(2.));
        assert!(images.set_recipient_axes("one",a,0.,1.));let before=images.get("one").cloned();
        for (cos,sin) in [(0.,0.),(2.,0.),(f64::NAN,0.),(0.,f64::INFINITY)] {assert!(!images.set_recipient_axes("one",a,cos,sin));assert_eq!(images.get("one"),before.as_ref());}
        assert!(!images.set_recipient_axes("absent",a,1.,0.));
        images.set("one",Handle::default(),Vec2::splat(2.));assert!(images.get("one").unwrap().presentation.is_none());
        assert!(images.get("one").unwrap().revision>before.unwrap().revision);assert_eq!(ui.get_float("Owned/A/rotateangle"),Some(1.));
    }
    #[test] fn missing_ui_or_unloaded_owned_recipient_refuses_presentation() {
        let (mut ui,root,a,_)=ui_for_presentation();let mut images=RuntimeImages::default();images.set("one",Handle::default(),Vec2::splat(2.));assert!(images.set_recipient_axes("one",a,0.,1.));
        let (mut pieces,_)=super::super::draw::pieces(&ui,root,&|_,_,_|(0.,0.),super::super::draw::Select::Whole {kept:&[],strip:None},true);
        mark(&mut pieces,Some(&images),&mut Told::default());present(&mut pieces,Some(&images),None);assert!(!pieces.iter().any(|p|p.tile==a));assert_eq!(pieces.len(),1,"foreign recipient remains untouched");
        ui.unload_tile(a);let (mut pieces,_)=super::super::draw::pieces(&ui,root,&|_,_,_|(0.,0.),super::super::draw::Select::Whole {kept:&[],strip:None},true);
        mark(&mut pieces,Some(&images),&mut Told::default());present(&mut pieces,Some(&images),Some(&ui));assert_eq!(pieces.len(),1);
    }

    #[test]
    fn generated_pixels_retained_by_an_independent_name_remain_in_use() {
        let mut images = Assets::<Image>::default();
        let h = images.add(Image::default());
        let mut runtime = RuntimeImages::default();
        runtime.set("owner", h.clone(), Vec2::ONE);
        runtime.set("foreign-copy", h.clone(), Vec2::ONE);
        assert!(runtime.remove("owner"));
        assert!(runtime.uses_image(&h));
        assert_eq!(runtime.get("foreign-copy").unwrap().image, h);
        assert!(images.get(&h).is_some());
        assert!(runtime.remove("foreign-copy"));
        assert!(!runtime.uses_image(&h));
    }

    #[test]
    fn invalid_generated_recipient_geometry_drops_only_the_declared_recipient() {
        let (mut ui, root, a, b) = ui_for_presentation();
        let mut runtime = RuntimeImages::default();
        runtime.set("one", Handle::default(), Vec2::ONE);
        assert!(runtime.set_recipient_axes("one", a, 0., 1.));
        for width in [0., -1., 65537., f64::INFINITY, f64::NAN] {
            ui.set_trait(a, "width", Value::Num(width));
            let (mut set, _) = super::super::draw::pieces(&ui, root, &|_,_,_| (0.,0.), super::super::draw::Select::Whole { kept: &[], strip: None }, true);
            present(&mut set, Some(&runtime), Some(&ui));
            assert!(!set.iter().any(|p| p.tile == a));
            assert!(set.iter().any(|p| p.tile == b), "foreign alias retains its own valid drawing");
        }
    }
}
