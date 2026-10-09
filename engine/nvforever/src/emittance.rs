//! External emittance (docs/spec-notes/world.md §8): references can name an emittance
//! source (REFR XEMI): a region whose weather's sunlight colour at the current hour, or a
//! light's colour, drives them [hypothesis]. It scales:
//! - shapes whose shader property sets BSShaderFlags bit 29 "External_Emittance": the
//!   surface colour of unlit shapes (light beams and rays), the emitted light of lit ones
//!   (glowing windows);
//! - the colour of LIGH references.
//!
//! The Prospector Saloon shows why: its porch lamp beam and the two lights above the porch
//! name InvertedDaylight regions, whose weathers have black sunlight by day, and the beam
//! is invisible by day in game; its window light rays name NVInteriorRegion, whose
//! weather has warm sunlight by day and black at night, and in game they are warm yellow.
//!
//! Each (material, source) pair gets its own material copy, recoloured when the hour
//! moves; references without XEMI keep the shared material.

use bevy::prelude::*;
use fnv_world::lighting::{emittance, Emittance};
use fnv_world::sky::{srgb_to_linear, weather, ColorType, Weather};
use nvf_assets::NifMaterial;
use std::collections::HashMap;

use crate::sky::SkyState;
use crate::world::WorldData;

pub struct EmittancePlugin;

impl Plugin for EmittancePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EmittanceState>().add_systems(Update, update_emittance.after(crate::sky::update_sky));
    }
}

/// A material copy driven by an emittance source, with the colours it started from.
struct Driven {
    copy: Handle<NifMaterial>,
    source: u32,
    base_color: LinearRgba,
    emissive: LinearRgba,
    unlit: bool,
}

#[derive(Resource, Default)]
pub struct EmittanceState {
    copies: HashMap<(AssetId<NifMaterial>, u32), Handle<NifMaterial>>,
    driven: Vec<Driven>,
    /// Resolved XEMI targets; `None` remembers one that is neither a region with a
    /// weather nor a light.
    sources: HashMap<u32, Option<Source>>,
    /// Hour of the last update; `None` forces the next one.
    hour: Option<f32>,
}

enum Source {
    Weather(Box<Weather>),
    Color(LinearRgba),
}

/// A light whose colour an emittance source scales.
#[derive(Component)]
pub struct LightEmittance {
    pub source: u32,
    pub color: LinearRgba,
}

impl EmittanceState {
    fn source(&mut self, world: &WorldData, xemi: u32) -> bool {
        self.sources
            .entry(xemi)
            .or_insert_with(|| match emittance(world.lo, world.schema, xemi)? {
                Emittance::Weather(w) => weather(world.lo, world.schema, w).map(|w| Source::Weather(Box::new(w))),
                Emittance::Light(c) => Some(Source::Color(LinearRgba::rgb(srgb_to_linear(c[0]), srgb_to_linear(c[1]), srgb_to_linear(c[2])))),
            })
            .is_some()
    }

    /// The material for a shape with external emittance on a reference naming `xemi`:
    /// a copy recoloured by the source, or `None` to keep the shared one.
    pub fn material(&mut self, world: &WorldData, materials: &mut Assets<NifMaterial>, shared: &Handle<NifMaterial>, xemi: u32) -> Option<Handle<NifMaterial>> {
        if !self.source(world, xemi) {
            return None;
        }
        if let Some(h) = self.copies.get(&(shared.id(), xemi)) {
            return Some(h.clone());
        }
        let m = materials.get(shared)?.clone();
        let (base_color, emissive, unlit) = (m.base.base_color.to_linear(), m.base.emissive, m.base.unlit);
        let copy = materials.add(m);
        self.copies.insert((shared.id(), xemi), copy.clone());
        self.driven.push(Driven { copy: copy.clone(), source: xemi, base_color, emissive, unlit });
        self.hour = None;
        Some(copy)
    }

    /// Mark a light as driven by `xemi`, if that resolves.
    pub fn light(&mut self, world: &WorldData, xemi: u32, color: LinearRgba) -> Option<LightEmittance> {
        self.hour = None;
        self.source(world, xemi).then_some(LightEmittance { source: xemi, color })
    }
}

/// The emittance colour of a source at the sky's hour (linear).
fn color(source: &Source, sky: &SkyState) -> LinearRgba {
    match source {
        Source::Weather(w) => {
            let c = w.color(&sky.climate, ColorType::Sunlight, sky.hour);
            LinearRgba::rgb(c[0], c[1], c[2])
        }
        Source::Color(c) => *c,
    }
}

fn scale(a: LinearRgba, b: LinearRgba) -> LinearRgba {
    LinearRgba::new(a.red * b.red, a.green * b.green, a.blue * b.blue, a.alpha)
}

fn update_emittance(
    sky: Res<SkyState>,
    mut state: ResMut<EmittanceState>,
    mut materials: ResMut<Assets<NifMaterial>>,
    mut lights: Query<(&LightEmittance, Option<&mut PointLight>, Option<&mut SpotLight>)>,
    added: Query<(), Added<LightEmittance>>,
) {
    let moved = state.hour.is_none_or(|h| (h - sky.hour).abs() > 1.0 / 60.0);
    if !moved && added.is_empty() {
        return;
    }
    state.hour = Some(sky.hour);
    let colors: HashMap<u32, LinearRgba> =
        state.sources.iter().filter_map(|(&k, s)| Some((k, color(s.as_ref()?, &sky)))).collect();
    for d in &state.driven {
        let Some(&e) = colors.get(&d.source) else { continue };
        let Some(mut m) = materials.get_mut(&d.copy) else { continue };
        if d.unlit {
            m.base.base_color = Color::LinearRgba(scale(d.base_color, e));
        } else {
            m.base.emissive = scale(d.emissive, e);
        }
    }
    for (l, point, spot) in &mut lights {
        let Some(&e) = colors.get(&l.source) else { continue };
        let c = Color::LinearRgba(scale(l.color, e));
        if let Some(mut p) = point {
            p.color = c;
        }
        if let Some(mut s) = spot {
            s.color = c;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_per_channel() {
        let s = scale(LinearRgba::new(1.0, 0.5, 0.25, 0.3), LinearRgba::rgb(0.5, 1.0, 0.0));
        assert_eq!((s.red, s.green, s.blue, s.alpha), (0.5, 0.5, 0.0, 0.3));
    }
}
