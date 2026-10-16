//! Effect shaders: EFSH records drawn over a reference's own meshes (holograms, cloaks,
//! glows), played by scripts (PlayMagicShaderVisuals) and magic effects. The membrane
//! part is read here with its timing; the particle shader, holes and add-on models are
//! not. Layout and rules: docs/spec-notes/effect-shaders.md.

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
    /// Particle texture (`ICO2`), for reports; particles are not drawn.
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
    /// Every field read lies before this.
    pub const NEEDED: usize = 96;
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
        particle_texture,
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
