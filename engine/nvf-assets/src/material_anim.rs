//! Sequences that drive a shape's material: its glow (emissive colour, emissive
//! multiplier) and its base texture's offset (docs/spec-notes/activators.md §2). The
//! jukeboxes' Idle flickers their lights and scrolls the music notes and bubbles; the
//! radios' Open and Close light and darken their dials.
//!
//! A rig whose clips have such tracks carries [`RigMaterials`] on its root: the shapes
//! they drive, each given its own copy of its material when first bound
//! ([`bind_rig_materials`]), so one reference's look does not change every copy of the
//! model. The idle clip's tracks follow the rig's [`Playing`] clock
//! ([`animate_rig_materials`]); other clips are applied by whoever poses them
//! ([`RigMaterials::apply`]).
//!
//! Values [hypothesis, nif.md §6 "Emissive light"]: the drawn emissive is colour ×
//! multiplier, as for a stored material; a track replaces the member it drives, the
//! others keep their stored value. A texture offset track moves the texture by its value
//! (`uv + offset`) [hypothesis: the direction is not documented; the textures tile, so
//! only the direction of the scroll depends on it].

use crate::anim_play::{MaterialTarget, MaterialTrack, Playing};
use crate::nif_material::{multiply_emissive, NifMaterial};
use bevy::math::Affine2;
use bevy::prelude::*;
use std::sync::Arc;

/// One shape of a rig whose material a clip drives.
#[derive(Debug, Clone)]
pub struct MaterialSlot {
    /// The shape's name in the model (what tracks name).
    pub shape: String,
    pub entity: Entity,
    /// The model's shared material until bound, then this reference's own copy.
    pub handle: Handle<NifMaterial>,
    /// Whether `handle` is this reference's own copy (only then is it changed).
    pub own: bool,
    /// Stored emissive colour and multiplier.
    pub emissive: [f32; 3],
    pub emissive_mult: f32,
    /// What was last written: emissive colour, multiplier, texture offset.
    last: Option<([f32; 3], f32, [f32; 2])>,
}

impl MaterialSlot {
    pub fn new(shape: String, entity: Entity, handle: Handle<NifMaterial>, emissive: [f32; 3], emissive_mult: f32) -> MaterialSlot {
        MaterialSlot { shape, entity, handle, own: false, emissive, emissive_mult, last: None }
    }
}

/// On the root of a rig whose clips drive materials.
#[derive(Component, Debug, Clone)]
pub struct RigMaterials {
    pub slots: Vec<MaterialSlot>,
    /// The idle clip's material tracks, following the rig's [`Playing`].
    pub idle: Arc<Vec<MaterialTrack>>,
    /// Draw the idle's lights dark (emissive tracks' shapes at black) and keep its
    /// texture offsets where they are: a radio switched off (activators.md §3).
    pub dark: bool,
}

/// The values of `tracks` at `t` for each slot; `dark` blacks out the emissive of
/// every slot an emissive track drives. Per slot: emissive colour, multiplier, offset.
pub fn sample(slots: &[MaterialSlot], tracks: &[MaterialTrack], t: f32, dark: bool) -> Vec<([f32; 3], f32, [f32; 2])> {
    slots
        .iter()
        .map(|s| {
            let (mut colour, mut mult, mut uv) = (s.emissive, s.emissive_mult, s.last.map_or([0.0; 2], |l| l.2));
            for tr in tracks.iter().filter(|tr| tr.shape == s.shape) {
                match tr.target {
                    MaterialTarget::SelfIllum if dark => colour = [0.0; 3],
                    MaterialTarget::EmissiveMult if dark => mult = 0.0,
                    _ if dark => {}
                    MaterialTarget::SelfIllum => colour = tr.values.at(t).unwrap_or(colour),
                    MaterialTarget::EmissiveMult => mult = tr.values.at(t).map_or(mult, |v| v[0].max(0.0)),
                    MaterialTarget::UvOffset(axis) => uv[axis] = tr.values.at(t).map_or(uv[axis], |v| v[0]),
                }
            }
            (colour, mult, uv)
        })
        .collect()
}

impl RigMaterials {
    /// Pose the slots' materials with `tracks` at sequence time `t` (see [`sample`]).
    pub fn apply(&mut self, tracks: &[MaterialTrack], t: f32, dark: bool, materials: &mut Assets<NifMaterial>) {
        let values = sample(&self.slots, tracks, t, dark);
        for (slot, v) in self.slots.iter_mut().zip(values) {
            if !slot.own || slot.last == Some(v) {
                continue;
            }
            let Some(mut m) = materials.get_mut(&slot.handle) else { continue };
            let (colour, mult, uv) = v;
            let value = |c: f32| if matches!(m.base.alpha_mode, AlphaMode::Multiply) { multiply_emissive(c * mult) } else { c * mult };
            m.base.emissive = LinearRgba::rgb(value(colour[0]), value(colour[1]), value(colour[2]));
            m.base.uv_transform = Affine2::from_translation(Vec2::from_array(uv));
            slot.last = Some(v);
        }
    }
}

/// Give each newly drawn rig's driven shapes their own copy of their material.
pub fn bind_rig_materials(
    mut rigs: Query<&mut RigMaterials, Added<RigMaterials>>,
    mut materials: ResMut<Assets<NifMaterial>>,
    mut shapes: Query<&mut MeshMaterial3d<NifMaterial>>,
) {
    for mut rig in &mut rigs {
        for slot in &mut rig.slots {
            let Some(copy) = materials.get(&slot.handle).cloned() else { continue };
            let own = materials.add(copy);
            if let Ok(mut m) = shapes.get_mut(slot.entity) {
                m.0 = own.clone();
                slot.handle = own;
                slot.own = true;
            }
        }
    }
}

/// Pose the materials of rigs playing their idle clip at its time (or dark).
pub fn animate_rig_materials(mut rigs: Query<(&Playing, &mut RigMaterials)>, mut materials: ResMut<Assets<NifMaterial>>) {
    for (p, mut rig) in &mut rigs {
        if rig.idle.is_empty() {
            continue;
        }
        let idle = rig.idle.clone();
        let dark = rig.dark;
        rig.apply(&idle, p.sequence_time(), dark, &mut materials);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_nif::anim::{Channel, Key, KeyKind, Keys, ValueTrack};

    fn keys<const N: usize>(pts: &[(f32, [f32; N])]) -> Keys<N> {
        Keys { kind: KeyKind::Linear, keys: pts.iter().map(|&(time, value)| Key { time, value, forward: [0.0; N], backward: [0.0; N], tbc: [0.0; 3] }).collect() }
    }

    fn slot(name: &str) -> MaterialSlot {
        MaterialSlot::new(name.into(), Entity::PLACEHOLDER, Handle::default(), [0.86; 3], 1.0)
    }

    #[test]
    fn tracks_replace_what_they_drive() {
        // The saloon jukebox's body light: emissive 0.86 stored, the Idle dims it to
        // 0.349 at 2 s; the music notes scroll from 2 to 0 in 8 s.
        let glow = MaterialTrack {
            shape: "JukeBox01:0".into(),
            target: MaterialTarget::SelfIllum,
            values: Arc::new(ValueTrack::Point3(Channel::Keys(keys(&[(0.0, [0.863; 3]), (2.0, [0.349; 3])])))),
        };
        let notes = MaterialTrack {
            shape: "Music".into(),
            target: MaterialTarget::UvOffset(0),
            values: Arc::new(ValueTrack::Float(Channel::Keys(keys(&[(0.0, [2.0]), (8.0, [0.0])])))),
        };
        let slots = [slot("JukeBox01:0"), slot("Music"), slot("Other")];
        let tracks = [glow, notes];
        let v = sample(&slots, &tracks, 2.0, false);
        assert!((v[0].0[0] - 0.349).abs() < 1e-5 && v[0].1 == 1.0 && v[0].2 == [0.0, 0.0]);
        assert!((v[1].2[0] - 1.5).abs() < 1e-5 && v[1].0 == [0.86; 3]);
        assert_eq!(v[2], ([0.86; 3], 1.0, [0.0, 0.0]));
        // Dark: the light goes out, the notes stay where they were.
        let d = sample(&slots, &tracks, 2.0, true);
        assert_eq!(d[0].0, [0.0; 3]);
        assert_eq!(d[1].0, [0.86; 3]);
        assert_eq!(d[2].0, [0.86; 3]);
    }
}
