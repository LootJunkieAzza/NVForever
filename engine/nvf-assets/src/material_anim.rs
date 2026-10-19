//! Sequences that drive a shape's material: its glow (emissive colour, emissive
//! multiplier), its base texture's offset and its alpha (docs/spec-notes/activators.md
//! §2, §6). The jukeboxes' Idle flickers their lights and scrolls the music notes and
//! bubbles; the radios' Open and Close light and darken their dials; the neon signs' and
//! lit windows' Left and Right fade them in and out.
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
//! only the direction of the scroll depends on it]. An alpha track replaces the
//! material's alpha [hypothesis]; it shows only on a shape drawn blended or tested (one
//! with an `NiAlphaProperty`).

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
    /// Stored material alpha.
    pub alpha: f32,
    /// What was last written, before the rig's fade, and the fade it was written with.
    last: Option<Look>,
    faded: f32,
}

impl MaterialSlot {
    pub fn new(shape: String, entity: Entity, handle: Handle<NifMaterial>, emissive: [f32; 3], emissive_mult: f32, alpha: f32) -> MaterialSlot {
        MaterialSlot { shape, entity, handle, own: false, emissive, emissive_mult, alpha, last: None, faded: 1.0 }
    }
}

/// What the tracks give one shape's material.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub colour: [f32; 3],
    pub mult: f32,
    pub uv: [f32; 2],
    pub alpha: f32,
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
    /// What the alpha of every slot is multiplied by, 1 to 0: a shot's beam fading out
    /// (nvforever `shot_fx`, docs/spec-notes/combat-rules.md §15); 1 for everything else.
    pub fade: f32,
}

/// The values of `tracks` at `t` for each slot; `dark` blacks out the emissive of
/// every slot an emissive track drives (and leaves offsets and alpha where they are).
/// A member no track drives keeps its stored value; offset and alpha stay where a
/// track last left them. A track with its own timing ([`MaterialTrack::cycle`]) is
/// taken at its own time after `clock` seconds of play instead of at `t`.
pub fn sample(slots: &[MaterialSlot], tracks: &[MaterialTrack], t: f32, clock: f32, dark: bool) -> Vec<Look> {
    slots
        .iter()
        .map(|s| {
            let mut l = Look { colour: s.emissive, mult: s.emissive_mult, uv: s.last.map_or([0.0; 2], |l| l.uv), alpha: s.last.map_or(s.alpha, |l| l.alpha) };
            for tr in tracks.iter().filter(|tr| tr.shape == s.shape) {
                let t = tr.cycle.map_or(t, |c| c.time(clock));
                match tr.target {
                    MaterialTarget::SelfIllum if dark => l.colour = [0.0; 3],
                    MaterialTarget::EmissiveMult if dark => l.mult = 0.0,
                    _ if dark => {}
                    MaterialTarget::SelfIllum => l.colour = tr.values.at(t).unwrap_or(l.colour),
                    MaterialTarget::EmissiveMult => l.mult = tr.values.at(t).map_or(l.mult, |v| v[0].max(0.0)),
                    MaterialTarget::UvOffset(axis) => l.uv[axis] = tr.values.at(t).map_or(l.uv[axis], |v| v[0]),
                    MaterialTarget::Alpha => l.alpha = tr.values.at(t).map_or(l.alpha, |v| v[0].clamp(0.0, 1.0)),
                }
            }
            l
        })
        .collect()
}

impl RigMaterials {
    /// Pose the slots' materials with `tracks` at sequence time `t`, after `clock`
    /// seconds of play (see [`sample`]).
    pub fn apply(&mut self, tracks: &[MaterialTrack], t: f32, clock: f32, dark: bool, materials: &mut Assets<NifMaterial>) {
        let values = sample(&self.slots, tracks, t, clock, dark);
        let fade = self.fade.clamp(0.0, 1.0);
        for (slot, v) in self.slots.iter_mut().zip(values) {
            if !slot.own || (slot.last == Some(v) && slot.faded == fade) {
                continue;
            }
            let Some(mut m) = materials.get_mut(&slot.handle) else { continue };
            let Look { colour, mult, uv, alpha } = v;
            let value = |c: f32| if matches!(m.base.alpha_mode, AlphaMode::Multiply) { multiply_emissive(c * mult) } else { c * mult };
            m.base.emissive = LinearRgba::rgb(value(colour[0]), value(colour[1]), value(colour[2]));
            m.base.uv_transform = Affine2::from_translation(Vec2::from_array(uv));
            m.base.base_color.set_alpha(alpha * fade);
            slot.last = Some(v);
            slot.faded = fade;
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

/// Pose the materials of rigs playing their idle clip at its time (or dark), and fade
/// those that fade.
pub fn animate_rig_materials(mut rigs: Query<(&Playing, &mut RigMaterials)>, mut materials: ResMut<Assets<NifMaterial>>) {
    for (p, mut rig) in &mut rigs {
        if rig.idle.is_empty() && rig.fade >= 1.0 {
            continue;
        }
        let idle = rig.idle.clone();
        let dark = rig.dark;
        rig.apply(&idle, p.sequence_time(), p.time - p.start, dark, &mut materials);
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
        MaterialSlot::new(name.into(), Entity::PLACEHOLDER, Handle::default(), [0.86; 3], 1.0, 0.0)
    }

    #[test]
    fn tracks_replace_what_they_drive() {
        // The saloon jukebox's body light: emissive 0.86 stored, the Idle dims it to
        // 0.349 at 2 s; the music notes scroll from 2 to 0 in 8 s.
        let glow = MaterialTrack {
            shape: "JukeBox01:0".into(),
            target: MaterialTarget::SelfIllum,
            values: Arc::new(ValueTrack::Point3(Channel::Keys(keys(&[(0.0, [0.863; 3]), (2.0, [0.349; 3])])))),
            cycle: None,
        };
        let notes = MaterialTrack {
            shape: "Music".into(),
            target: MaterialTarget::UvOffset(0),
            values: Arc::new(ValueTrack::Float(Channel::Keys(keys(&[(0.0, [2.0]), (8.0, [0.0])])))),
            cycle: None,
        };
        let slots = [slot("JukeBox01:0"), slot("Music"), slot("Other")];
        let tracks = [glow, notes];
        let v = sample(&slots, &tracks, 2.0, 2.0, false);
        assert!((v[0].colour[0] - 0.349).abs() < 1e-5 && v[0].mult == 1.0 && v[0].uv == [0.0, 0.0]);
        assert!((v[1].uv[0] - 1.5).abs() < 1e-5 && v[1].colour == [0.86; 3]);
        assert_eq!(v[2], Look { colour: [0.86; 3], mult: 1.0, uv: [0.0, 0.0], alpha: 0.0 });
        // Dark: the light goes out, the notes stay where they were.
        let d = sample(&slots, &tracks, 2.0, 2.0, true);
        assert_eq!(d[0].colour, [0.0; 3]);
        assert_eq!(d[1].colour, [0.86; 3]);
        assert_eq!(d[2].colour, [0.86; 3]);
    }

    #[test]
    fn an_alpha_track_fades_the_shape() {
        // `windowem01.nif`: the window stores alpha 0; Left keys 1 (lit), Right 0.
        let left = MaterialTrack {
            shape: "windowEm01:0".into(),
            target: MaterialTarget::Alpha,
            values: Arc::new(ValueTrack::Float(Channel::Keys(keys(&[(0.0, [1.0]), (0.033, [1.0])])))),
            cycle: None,
        };
        let slots = [slot("windowEm01:0")];
        assert_eq!(sample(&slots, &[], 0.0, 0.0, false)[0].alpha, 0.0);
        assert_eq!(sample(&slots, std::slice::from_ref(&left), 0.01, 0.01, false)[0].alpha, 1.0);
    }

    #[test]
    fn a_controller_of_its_own_keeps_its_own_cycle() {
        // `fxsmokewisps01.nif`: the wisps' texture scrolls along V from 0 to 1 in 36.67 s
        // and along U in 44.1 s, each looping by itself.
        let track = |axis: usize, stop: f32| MaterialTrack {
            shape: "WispsGeo:0".into(),
            target: MaterialTarget::UvOffset(axis),
            values: Arc::new(ValueTrack::Float(Channel::Keys(keys(&[(0.0, [0.0]), (stop, [1.0])])))),
            cycle: Some(crate::anim_play::OwnCycle { start: 0.0, stop, frequency: 1.0, phase: 0.0, cycle_type: 0 }),
        };
        let tracks = [track(1, 36.666668), track(0, 44.1)];
        let clip = crate::anim_play::Clip::from_controllers(tracks.to_vec());
        assert!((clip.stop - 44.1).abs() < 1e-4);
        let slots = [slot("WispsGeo:0")];
        // 50 s of play: V is 13.33 s into its second cycle, U 5.9 s into its second,
        // whatever the clip's own (wrapped) time.
        let v = sample(&slots, &tracks, 5.9, 50.0, false)[0].uv;
        assert!((v[1] - 13.333332 / 36.666668).abs() < 1e-4, "{v:?}");
        assert!((v[0] - 5.9 / 44.1).abs() < 1e-4, "{v:?}");
    }
}
