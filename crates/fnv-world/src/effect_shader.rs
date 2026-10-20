//! Effect shaders: EFSH records drawn over a reference's own meshes (holograms, cloaks,
//! glows, burning), played by scripts (PlayMagicShaderVisuals) and magic effects. The
//! membrane and the particle shader are read here with their timing; holes and add-on
//! models are not. Layout and rules: docs/spec-notes/effect-shaders.md.

use crate::fields::{winner, Fields};
use fnv_esm::{LoadOrder, Schema};

/// Blend mode values [documented: xEdit wbBlendModeEnum].
pub mod blend {
    pub const ZERO: u32 = 1;
    pub const ONE: u32 = 2;
    pub const SOURCE_ALPHA: u32 = 5;
    pub const SOURCE_INVERTED_ALPHA: u32 = 6;
    pub const DEST_COLOR: u32 = 9;
}

/// DATA flag bits [documented: xEdit wbDefinitionsFNV.pas EFSH].
pub const NO_MEMBRANE: u8 = 1;
pub const NO_PARTICLES: u8 = 1 << 3;
pub const EDGE_INVERSE: u8 = 1 << 4;
pub const AFFECT_SKIN_ONLY: u8 = 1 << 5;

/// The alpha over time of the fill or the edge (effect-shaders.md §3).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AlphaCycle {
    pub fade_in: f32,
    pub full_time: f32,
    pub fade_out: f32,
    /// Alpha at the top of the cycle.
    pub full: f32,
    /// Alpha after the fade out, while the shader lasts.
    pub persistent: f32,
    pub pulse_amplitude: f32,
    pub pulse_frequency: f32,
}

impl AlphaCycle {
    /// Seconds of the cycle: fade in, full, fade out.
    pub fn length(&self) -> f32 {
        self.fade_in.max(0.0) + self.full_time.max(0.0) + self.fade_out.max(0.0)
    }

    /// The alpha `t` seconds after the shader started; `persist`: whether the persistent
    /// part shows after the cycle (otherwise 0 there) [hypothesis, effect-shaders.md §3].
    pub fn alpha(&self, t: f32, persist: bool) -> f32 {
        let (fi, ft, fo) = (self.fade_in.max(0.0), self.full_time.max(0.0), self.fade_out.max(0.0));
        let a = if t < fi {
            self.full * t / fi
        } else if t < fi + ft {
            self.full
        } else if t < fi + ft + fo {
            let k = (t - fi - ft) / fo;
            self.full + (self.persistent - self.full) * k
        } else if persist {
            self.persistent
        } else {
            0.0
        };
        let pulse = 1.0 + self.pulse_amplitude * (std::f32::consts::TAU * self.pulse_frequency * t).sin();
        (a * pulse).clamp(0.0, 1.0)
    }
}

/// An EFSH record's membrane shader.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectShader {
    pub form_id: u32,
    pub edid: String,
    /// DATA flags ([`NO_MEMBRANE`] …).
    pub flags: u8,
    /// Fill texture path under `textures\`, as stored (`ICON`); `None` when empty.
    pub fill_texture: Option<String>,
    /// Particle texture (`ICO2`), as stored.
    pub particle_texture: Option<String>,
    pub source_blend: u32,
    pub blend_op: u32,
    pub z_test: u32,
    pub dest_blend: u32,
    /// sRGB bytes.
    pub fill_color: [u8; 3],
    pub fill: AlphaCycle,
    /// Texture animation speed, U and V.
    pub fill_speed: [f32; 2],
    pub edge_color: [u8; 3],
    pub edge: AlphaCycle,
    pub edge_fall_off: f32,
    /// The particle shader, when the record has one: not flagged "No Particle Shader",
    /// a particle texture, and the fields in its DATA (effect-shaders.md §6).
    pub particles: Option<ParticleShader>,
}

/// A colour key of the particle shader: sRGB bytes, alpha, and the share of the
/// particle's life it falls at.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ColorKey {
    pub color: [u8; 3],
    pub alpha: f32,
    pub time: f32,
}

/// The particle shader: particles born on the reference's surface (effect-shaders.md §6;
/// field meanings from the GECK wiki EffectShader page [documented]).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ParticleShader {
    pub source_blend: u32,
    pub blend_op: u32,
    pub z_test: u32,
    pub dest_blend: u32,
    /// Birth ratio over the shader's time: ramp up to `full_ratio`, held, ramp down to
    /// `persistent_ratio`.
    pub ramp_up: f32,
    pub full_time: f32,
    pub ramp_down: f32,
    pub full_ratio: f32,
    pub persistent_ratio: f32,
    /// Seconds a particle lives, ± the range.
    pub lifetime: f32,
    pub lifetime_range: f32,
    /// Speed along the birth normal, ± the range, and its acceleration.
    pub speed_normal: f32,
    pub speed_normal_range: f32,
    pub accel_normal: f32,
    /// Velocity and acceleration in world coordinates (game axes, z up).
    pub velocity: [f32; 3],
    pub accel: [f32; 3],
    /// Scale keys: (scale, share of the life). Particles are born at scale 0.
    pub scale_keys: [(f32, f32); 2],
    pub color_keys: [ColorKey; 3],
    /// Degrees: initial rotation ± range, rotation speed ± range (per second).
    pub rotation: f32,
    pub rotation_range: f32,
    pub spin: f32,
    pub spin_range: f32,
    /// The texture's frames across and down (1 when the record is too short to say).
    pub frames: [u32; 2],
}

impl ParticleShader {
    /// The birth ratio `t` seconds after the shader started; after the ramps the
    /// persistent ratio while `persist`, else none [hypothesis, as the membrane's alpha;
    /// effect-shaders.md §6].
    pub fn birth_ratio(&self, t: f32, persist: bool) -> f32 {
        let (up, full, down) = (self.ramp_up.max(0.0), self.full_time.max(0.0), self.ramp_down.max(0.0));
        let r = if t < up {
            self.full_ratio * t / up
        } else if t < up + full {
            self.full_ratio
        } else if t < up + full + down {
            self.full_ratio + (self.persistent_ratio - self.full_ratio) * (t - up - full) / down
        } else if persist {
            self.persistent_ratio
        } else {
            0.0
        };
        r.max(0.0)
    }

    /// Seconds of the birth cycle (ramp up, full, ramp down).
    pub fn cycle(&self) -> f32 {
        self.ramp_up.max(0.0) + self.full_time.max(0.0) + self.ramp_down.max(0.0)
    }

    /// The scale at share `f` (0–1) of a particle's life: from 0 to the first key, then
    /// to the second, held after it [documented: born at 0 and growing to key 1, GECK
    /// wiki; hypothesis: linear between keys, held after the last].
    pub fn scale(&self, f: f32) -> f32 {
        let [(k1, t1), (k2, t2)] = self.scale_keys;
        if f <= t1 {
            if t1 > 0.0 {
                k1 * f / t1
            } else {
                k1
            }
        } else if f <= t2 && t2 > t1 {
            k1 + (k2 - k1) * (f - t1) / (t2 - t1)
        } else {
            k2
        }
    }

    /// The tint (0–1 sRGB values, alpha) at share `f` of a particle's life: the
    /// texture's own colour (white) before the first key and after the last, linear
    /// between [documented: GECK wiki, a first key after 0 begins "with the raw color of
    /// the texture file", a last key before 1 returns to it "before dying"; hypothesis:
    /// linear blends]. The alpha is the nearest key's outside the keys and does not
    /// return to 1 [hypothesis: the wiki lists "Color" and "Color Alpha" apart and says
    /// only the colour returns; `Smoke01`'s alphas 0, 0.5, 0 at 0, 0.07 and 0.9 fade its
    /// puffs out, and an alpha back to 1 flashed each puff white at its full size before
    /// it went (owner's playtest 2026-10-20)].
    pub fn tint(&self, f: f32) -> [f32; 4] {
        let keys: Vec<ColorKey> = self.color_keys.iter().copied().filter(|k| (0.0..=1.0).contains(&k.time)).collect();
        let (Some(first), Some(last)) = (keys.first().copied(), keys.last().copied()) else { return [1.0; 4] };
        let raw = |time: f32, alpha: f32| ColorKey { color: [255; 3], alpha, time };
        let mut all = vec![raw(0.0, first.alpha)];
        all.extend(keys);
        all.push(raw(1.0, last.alpha));
        let at = |k: &ColorKey| [k.color[0] as f32 / 255.0, k.color[1] as f32 / 255.0, k.color[2] as f32 / 255.0, k.alpha];
        for w in all.windows(2) {
            let (a, b) = (&w[0], &w[1]);
            if f <= b.time {
                let k = if b.time > a.time { ((f - a.time) / (b.time - a.time)).clamp(0.0, 1.0) } else { 1.0 };
                let (x, y) = (at(a), at(b));
                return [0, 1, 2, 3].map(|i| x[i] + (y[i] - x[i]) * k);
            }
        }
        at(&all[all.len() - 1])
    }
}

impl EffectShader {
    /// Whether a membrane is drawn at all.
    pub fn has_membrane(&self) -> bool {
        self.flags & NO_MEMBRANE == 0
    }

    /// Seconds of one cycle: the longer of the fill's and the edge's.
    pub fn cycle(&self) -> f32 {
        self.fill.length().max(self.edge.length())
    }
}

/// Byte offsets in DATA [documented: xEdit wbDefinitionsFNV.pas EFSH; table in
/// effect-shaders.md §1].
mod off {
    pub const FLAGS: usize = 0;
    pub const SOURCE_BLEND: usize = 4;
    pub const BLEND_OP: usize = 8;
    pub const Z_TEST: usize = 12;
    pub const FILL_COLOR: usize = 16;
    pub const FILL_TIMES: usize = 20;
    pub const FILL_PERSISTENT: usize = 32;
    pub const FILL_PULSE: usize = 36;
    pub const FILL_SPEED: usize = 44;
    pub const EDGE_FALL_OFF: usize = 52;
    pub const EDGE_COLOR: usize = 56;
    pub const EDGE_TIMES: usize = 60;
    pub const EDGE_PERSISTENT: usize = 72;
    pub const EDGE_PULSE: usize = 76;
    pub const FILL_FULL: usize = 84;
    pub const EDGE_FULL: usize = 88;
    pub const DEST_BLEND: usize = 92;
    /// Every membrane field lies before this.
    pub const NEEDED: usize = 96;
    /// The particle shader (effect-shaders.md §6).
    pub const PARTICLE_BLENDS: usize = 96;
    pub const BIRTH: usize = 112;
    pub const LIFETIME: usize = 132;
    pub const SPEED_NORMAL: usize = 140;
    pub const VELOCITY: usize = 148;
    pub const ACCEL: usize = 160;
    pub const SCALE_KEYS: usize = 172;
    pub const COLOR_KEYS: usize = 188;
    pub const COLOR_ALPHAS: usize = 200;
    pub const COLOR_TIMES: usize = 212;
    pub const SPEED_NORMAL_RANGE: usize = 224;
    pub const ROTATION: usize = 228;
    pub const TEXTURE_COUNT: usize = 276;
    /// The shortest DATA with every particle field but the texture counts.
    pub const PARTICLES_NEEDED: usize = 244;
}

/// Read the membrane fields from an EFSH `DATA` payload (at least 96 bytes).
pub fn from_data(form_id: u32, edid: String, data: &[u8], fill_texture: Option<String>, particle_texture: Option<String>) -> Option<EffectShader> {
    if data.len() < off::NEEDED {
        return None;
    }
    let u = |o: usize| u32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]);
    let f = |o: usize| f32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]);
    let rgb = |o: usize| [data[o], data[o + 1], data[o + 2]];
    let cycle = |times: usize, persistent: usize, pulse: usize, full: usize| AlphaCycle {
        fade_in: f(times),
        full_time: f(times + 4),
        fade_out: f(times + 8),
        full: f(full),
        persistent: f(persistent),
        pulse_amplitude: f(pulse),
        pulse_frequency: f(pulse + 4),
    };
    Some(EffectShader {
        form_id,
        edid,
        flags: data[off::FLAGS],
        fill_texture,
        source_blend: u(off::SOURCE_BLEND),
        blend_op: u(off::BLEND_OP),
        z_test: u(off::Z_TEST),
        dest_blend: u(off::DEST_BLEND),
        fill_color: rgb(off::FILL_COLOR),
        fill: cycle(off::FILL_TIMES, off::FILL_PERSISTENT, off::FILL_PULSE, off::FILL_FULL),
        fill_speed: [f(off::FILL_SPEED), f(off::FILL_SPEED + 4)],
        edge_color: rgb(off::EDGE_COLOR),
        edge: cycle(off::EDGE_TIMES, off::EDGE_PERSISTENT, off::EDGE_PULSE, off::EDGE_FULL),
        edge_fall_off: f(off::EDGE_FALL_OFF),
        particles: particle_shader(data, &particle_texture),
        particle_texture,
    })
}

/// The particle fields, when the shader has particles (no "No Particle Shader" flag, a
/// texture: without one "the particle will not display", GECK wiki) and DATA holds them.
fn particle_shader(data: &[u8], texture: &Option<String>) -> Option<ParticleShader> {
    if data[off::FLAGS] & NO_PARTICLES != 0 || texture.is_none() || data.len() < off::PARTICLES_NEEDED {
        return None;
    }
    let u = |o: usize| u32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]);
    let f = |o: usize| f32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]);
    let v3 = |o: usize| [f(o), f(o + 4), f(o + 8)];
    let key = |i: usize| ColorKey {
        color: [data[off::COLOR_KEYS + 4 * i], data[off::COLOR_KEYS + 4 * i + 1], data[off::COLOR_KEYS + 4 * i + 2]],
        alpha: f(off::COLOR_ALPHAS + 4 * i),
        time: f(off::COLOR_TIMES + 4 * i),
    };
    let frames = if data.len() >= off::TEXTURE_COUNT + 8 { [u(off::TEXTURE_COUNT).clamp(1, 64), u(off::TEXTURE_COUNT + 4).clamp(1, 64)] } else { [1, 1] };
    let (p, b, r) = (off::PARTICLE_BLENDS, off::BIRTH, off::ROTATION);
    Some(ParticleShader {
        source_blend: u(p),
        blend_op: u(p + 4),
        z_test: u(p + 8),
        dest_blend: u(p + 12),
        ramp_up: f(b),
        full_time: f(b + 4),
        ramp_down: f(b + 8),
        full_ratio: f(b + 12),
        persistent_ratio: f(b + 16),
        lifetime: f(off::LIFETIME),
        lifetime_range: f(off::LIFETIME + 4),
        speed_normal: f(off::SPEED_NORMAL),
        accel_normal: f(off::SPEED_NORMAL + 4),
        speed_normal_range: f(off::SPEED_NORMAL_RANGE),
        velocity: v3(off::VELOCITY),
        accel: v3(off::ACCEL),
        scale_keys: [(f(off::SCALE_KEYS), f(off::SCALE_KEYS + 8)), (f(off::SCALE_KEYS + 4), f(off::SCALE_KEYS + 12))],
        color_keys: [key(0), key(1), key(2)],
        rotation: f(r),
        rotation_range: f(r + 4),
        spin: f(r + 8),
        spin_range: f(r + 12),
        frames,
    })
}

/// Read an EFSH record by its load-order FormID.
pub fn effect_shader(lo: &LoadOrder, schema: &Schema, fid: u32) -> Option<EffectShader> {
    let (pi, r) = winner(lo, fid)?;
    if &r.header.sig != b"EFSH" {
        return None;
    }
    let f = Fields::new(r, pi, schema).ok()?;
    let path = |sig: &[u8; 4]| f.string(sig).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let data = &f.raw(b"DATA")?.data;
    from_data(fid, f.string(b"EDID").unwrap_or_default(), data, path(b"ICON"), path(b"ICO2"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A DATA payload with the hologram's membrane numbers (NVDLC01HologramNeutral,
    /// effect-shaders.md §2), 96 bytes.
    fn hologram() -> Vec<u8> {
        let mut d = vec![0u8; 96];
        let put_u = |d: &mut Vec<u8>, o: usize, v: u32| d[o..o + 4].copy_from_slice(&v.to_le_bytes());
        let put_f = |d: &mut Vec<u8>, o: usize, v: f32| d[o..o + 4].copy_from_slice(&v.to_le_bytes());
        d[0] = 0x18;
        put_u(&mut d, 4, 5);
        put_u(&mut d, 8, 1);
        put_u(&mut d, 12, 3);
        d[16..19].copy_from_slice(&[119, 218, 255]);
        put_f(&mut d, 20, 0.2);
        put_f(&mut d, 32, 0.9);
        put_f(&mut d, 36, 0.1);
        put_f(&mut d, 40, 20.0);
        put_f(&mut d, 44, -0.5);
        put_f(&mut d, 52, 0.95);
        d[56..59].copy_from_slice(&[164, 206, 240]);
        put_f(&mut d, 60, 0.2);
        put_f(&mut d, 72, 0.8);
        put_f(&mut d, 84, 1.0);
        put_f(&mut d, 88, 1.0);
        put_u(&mut d, 92, 5);
        d
    }

    #[test]
    fn reads_the_membrane_fields() {
        let s = from_data(1, "Holo".into(), &hologram(), Some("x.dds".into()), None).unwrap();
        assert_eq!(s.fill_color, [119, 218, 255]);
        assert_eq!(s.edge_color, [164, 206, 240]);
        assert_eq!((s.source_blend, s.blend_op, s.z_test, s.dest_blend), (5, 1, 3, 5));
        assert_eq!(s.fill.fade_in, 0.2);
        assert_eq!(s.fill.full, 1.0);
        assert_eq!(s.fill.persistent, 0.9);
        assert_eq!(s.fill_speed, [-0.5, 0.0]);
        assert_eq!(s.edge.persistent, 0.8);
        assert!(s.has_membrane());
        assert_eq!(s.flags & EDGE_INVERSE, EDGE_INVERSE);
        assert!(from_data(1, String::new(), &hologram()[..95], None, None).is_none());
    }

    /// A 308-byte DATA with the particle numbers of `Flames01` (0x0005F9B6, the
    /// flamer's burning; effect-shaders.md §6).
    fn flames() -> Vec<u8> {
        let mut d = vec![0u8; 308];
        let put_u = |d: &mut Vec<u8>, o: usize, v: u32| d[o..o + 4].copy_from_slice(&v.to_le_bytes());
        let put_f = |d: &mut Vec<u8>, o: usize, v: f32| d[o..o + 4].copy_from_slice(&v.to_le_bytes());
        for (o, v) in [(4, 5), (8, 1), (12, 3), (92, 2), (96, 5), (100, 1), (104, 4), (108, 2), (276, 4), (280, 2)] {
            put_u(&mut d, o, v);
        }
        d[56..59].copy_from_slice(&[215, 86, 0]);
        let fs = [
            (52, 3.0),
            (72, 1.0),
            (84, 1.0),
            (88, 1.0),
            (112, 0.5),
            (124, 30.0),
            (128, 15.0),
            (132, 0.35),
            (136, 0.1),
            (140, 50.0),
            (144, -5.0),
            (148, 10.0),
            (152, 10.0),
            (156, 195.0),
            (168, 50.0),
            (172, 30.0),
            (176, 3.0),
            (180, 0.05),
            (184, 1.0),
            (200, 1.0),
            (204, 1.0),
            (208, 1.0),
            (212, 0.2),
            (216, 0.5),
            (220, 0.9),
            (224, 50.0),
            (232, 180.0),
            (236, 5.0),
            (240, 5.0),
        ];
        for (o, v) in fs {
            put_f(&mut d, o, v);
        }
        d[188..191].copy_from_slice(&[255, 107, 64]);
        d[192..195].copy_from_slice(&[255, 156, 23]);
        d[196..199].copy_from_slice(&[244, 80, 0]);
        d
    }

    #[test]
    fn reads_the_particle_shader() {
        let s = from_data(0x5F9B6, "Flames01".into(), &flames(), None, Some("Effects\\FXFlameSmallSub02.dds".into())).unwrap();
        assert_eq!(s.edge_color, [215, 86, 0]);
        let p = s.particles.expect("particles");
        assert_eq!((p.source_blend, p.dest_blend, p.z_test), (5, 2, 4));
        assert_eq!((p.full_ratio, p.persistent_ratio, p.ramp_up), (30.0, 15.0, 0.5));
        assert_eq!((p.lifetime, p.lifetime_range), (0.35, 0.1));
        assert_eq!((p.speed_normal, p.speed_normal_range, p.accel_normal), (50.0, 50.0, -5.0));
        assert_eq!(p.velocity, [10.0, 10.0, 195.0]);
        assert_eq!(p.accel, [0.0, 0.0, 50.0]);
        assert_eq!(p.scale_keys, [(30.0, 0.05), (3.0, 1.0)]);
        assert_eq!(p.color_keys[1].color, [255, 156, 23]);
        assert_eq!(p.color_keys[2].time, 0.9);
        assert_eq!((p.rotation_range, p.spin, p.spin_range), (180.0, 5.0, 5.0));
        assert_eq!(p.frames, [4, 2]);
        // Ramps up over 0.5 s, then the persistent ratio while persisting.
        assert!((p.birth_ratio(0.25, true) - 15.0).abs() < 1e-4);
        assert_eq!(p.birth_ratio(3.0, true), 15.0);
        assert_eq!(p.birth_ratio(3.0, false), 0.0);
        // Grows to 30 by 5 % of its life, shrinks to 3 at its end.
        assert!((p.scale(0.025) - 15.0).abs() < 1e-3);
        assert!((p.scale(0.525) - 16.5).abs() < 1e-3);
        // The texture's own colour at birth and death, the keys between.
        assert_eq!(p.tint(0.0), [1.0, 1.0, 1.0, 1.0]);
        let t = p.tint(0.5);
        assert!((t[1] - 156.0 / 255.0).abs() < 1e-4 && t[3] == 1.0);
        assert_eq!(p.tint(1.0), [1.0, 1.0, 1.0, 1.0]);
        // Smoke01's alphas 0, 0.5, 0: it fades in and out, never back to opaque.
        let mut smoke = p.clone();
        smoke.color_keys = [(84, 0.0, 0.0), (79, 0.5, 0.07), (64, 0.0, 0.9)].map(|(c, alpha, time)| ColorKey { color: [c; 3], alpha, time });
        let t = smoke.tint(0.0);
        assert!((t[0] - 84.0 / 255.0).abs() < 1e-6 && t[3] == 0.0);
        assert!((smoke.tint(0.07)[3] - 0.5).abs() < 1e-6);
        assert!((0.0..0.5).contains(&smoke.tint(0.5)[3]));
        assert_eq!(smoke.tint(0.95)[3], 0.0);
        assert_eq!(smoke.tint(1.0), [1.0, 1.0, 1.0, 0.0]);
        // No texture, the flag, or a short DATA: no particles, or no frame counts.
        assert!(from_data(1, String::new(), &flames(), None, None).unwrap().particles.is_none());
        let mut flagged = flames();
        flagged[0] = NO_PARTICLES;
        assert!(from_data(1, String::new(), &flagged, None, Some("x.dds".into())).unwrap().particles.is_none());
        let short = from_data(1, String::new(), &flames()[..244], None, Some("x.dds".into())).unwrap();
        assert_eq!(short.particles.unwrap().frames, [1, 1]);
    }

    #[test]
    fn alpha_rises_holds_and_settles() {
        let c = AlphaCycle { fade_in: 1.0, full_time: 2.0, fade_out: 1.0, full: 0.8, persistent: 0.2, ..Default::default() };
        assert_eq!(c.length(), 4.0);
        assert!((c.alpha(0.5, true) - 0.4).abs() < 1e-6);
        assert_eq!(c.alpha(2.0, true), 0.8);
        assert!((c.alpha(3.5, true) - 0.5).abs() < 1e-6);
        assert_eq!(c.alpha(10.0, true), 0.2);
        assert_eq!(c.alpha(10.0, false), 0.0);
        // No fade-in: full at once.
        let snap = AlphaCycle { full: 1.0, persistent: 1.0, ..Default::default() };
        assert_eq!(snap.alpha(0.0, true), 1.0);
        // A pulse varies the alpha around its value, held to 0..1.
        let p = AlphaCycle { full: 0.5, persistent: 0.5, pulse_amplitude: 0.5, pulse_frequency: 1.0, ..Default::default() };
        assert!((p.alpha(0.25, true) - 0.75).abs() < 1e-5);
        assert!((p.alpha(0.75, true) - 0.25).abs() < 1e-5);
    }
}
