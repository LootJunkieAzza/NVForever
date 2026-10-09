//! KF animation: controller sequences and transform tracks that can be sampled at any
//! time (docs/spec-notes/nif.md §6). Built on the schema-decoded blocks.

use crate::scene::Decoded;
use crate::value::{Struct, Value};

/// Interpolation of a key group (nif.xml KeyType).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyKind {
    Linear,
    /// Hermite with stored tangents: `backward` leaves the key, `forward` arrives at it.
    Quadratic,
    /// Kochanek–Bartels (tension, bias, continuity).
    Tbc,
    /// Rotation as three independent float key groups (Euler X, Y, Z).
    XyzRotation,
    /// Step: hold each key's value until the next key.
    Constant,
}

impl KeyKind {
    pub fn from_raw(v: i64) -> Option<KeyKind> {
        Some(match v {
            1 => KeyKind::Linear,
            2 => KeyKind::Quadratic,
            3 => KeyKind::Tbc,
            4 => KeyKind::XyzRotation,
            5 => KeyKind::Constant,
            _ => return None,
        })
    }
}

/// One key: time, value and (for quadratic keys) tangents; TBC parameters for TBC keys.
/// Despite the nif.xml names, `forward` is the tangent arriving at the key and
/// `backward` the one leaving it [observed: docs/spec-notes/nif.md §5].
#[derive(Debug, Clone, PartialEq)]
pub struct Key<const N: usize> {
    pub time: f32,
    pub value: [f32; N],
    pub forward: [f32; N],
    pub backward: [f32; N],
    pub tbc: [f32; 3],
}

#[derive(Debug, Clone, PartialEq)]
pub struct Keys<const N: usize> {
    pub kind: KeyKind,
    pub keys: Vec<Key<N>>,
}

fn arr<const N: usize>(v: Option<&Value>) -> Option<[f32; N]> {
    match v? {
        Value::Float(f) if N == 1 => Some([*f; N]),
        Value::Floats(f) => f.as_slice().try_into().ok(),
        _ => None,
    }
}

impl<const N: usize> Keys<N> {
    /// From a KeyGroup struct (num_keys, interpolation, keys).
    pub fn from_group(g: &Struct) -> Option<Keys<N>> {
        let kind = KeyKind::from_raw(g.int("interpolation").unwrap_or(1))?;
        let keys = g.list("keys").unwrap_or_default();
        Self::from_list(kind, keys)
    }

    pub fn from_list(kind: KeyKind, keys: &[Value]) -> Option<Keys<N>> {
        let mut out = Vec::with_capacity(keys.len());
        for k in keys {
            let Value::Struct(k) = k else { return None };
            let value = arr::<N>(k.get("value"))?;
            let tbc = k
                .strukt("tbc")
                .map(|t| [t.float("tension").unwrap_or(0.0), t.float("bias").unwrap_or(0.0), t.float("continuity").unwrap_or(0.0)])
                .unwrap_or([0.0; 3]);
            out.push(Key {
                time: k.float("time").unwrap_or(0.0),
                value,
                forward: arr::<N>(k.get("forward")).unwrap_or([0.0; N]),
                backward: arr::<N>(k.get("backward")).unwrap_or([0.0; N]),
                tbc,
            });
        }
        Some(Keys { kind, keys: out })
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Value at time `t`, clamped to the first/last key. Quaternions (N = 4) are
    /// interpolated component-wise and normalised by the caller.
    pub fn sample(&self, t: f32) -> Option<[f32; N]> {
        let ks = &self.keys;
        let first = ks.first()?;
        if ks.len() == 1 || t <= first.time {
            return Some(first.value);
        }
        let last = ks.last().unwrap();
        if t >= last.time {
            return Some(last.value);
        }
        let i = ks.partition_point(|k| k.time <= t).saturating_sub(1).min(ks.len() - 2);
        let (a, b) = (&ks[i], &ks[i + 1]);
        let span = b.time - a.time;
        let u = if span > 0.0 { (t - a.time) / span } else { 0.0 };
        Some(match self.kind {
            KeyKind::Constant => a.value,
            KeyKind::Linear | KeyKind::XyzRotation => lerp(a.value, b.value, u),
            KeyKind::Quadratic => hermite(a.value, a.backward, b.value, b.forward, u),
            KeyKind::Tbc => {
                let prev = if i > 0 { &ks[i - 1] } else { a };
                let next = if i + 2 < ks.len() { &ks[i + 2] } else { b };
                let out_a = tbc_tangent(prev, a, b, true);
                let in_b = tbc_tangent(a, b, next, false);
                hermite(a.value, out_a, b.value, in_b, u)
            }
        })
    }
}

fn lerp<const N: usize>(a: [f32; N], b: [f32; N], u: f32) -> [f32; N] {
    std::array::from_fn(|k| a[k] + (b[k] - a[k]) * u)
}

/// Cubic Hermite between p0 and p1 with tangents m0 (leaving p0) and m1 (arriving at p1),
/// tangents expressed per key interval [documented: standard Hermite basis].
fn hermite<const N: usize>(p0: [f32; N], m0: [f32; N], p1: [f32; N], m1: [f32; N], u: f32) -> [f32; N] {
    let u2 = u * u;
    let u3 = u2 * u;
    let h00 = 2.0 * u3 - 3.0 * u2 + 1.0;
    let h10 = u3 - 2.0 * u2 + u;
    let h01 = -2.0 * u3 + 3.0 * u2;
    let h11 = u3 - u2;
    std::array::from_fn(|k| h00 * p0[k] + h10 * m0[k] + h01 * p1[k] + h11 * m1[k])
}

/// Kochanek–Bartels tangent at `cur` (outgoing when `outgoing`, else incoming)
/// [documented: Kochanek & Bartels 1984, "Interpolating splines with local tension,
/// continuity, and bias control"].
fn tbc_tangent<const N: usize>(prev: &Key<N>, cur: &Key<N>, next: &Key<N>, outgoing: bool) -> [f32; N] {
    let [t, b, c] = cur.tbc;
    let (fa, fb) = if outgoing {
        ((1.0 - t) * (1.0 + b) * (1.0 - c) / 2.0, (1.0 - t) * (1.0 - b) * (1.0 + c) / 2.0)
    } else {
        ((1.0 - t) * (1.0 + b) * (1.0 + c) / 2.0, (1.0 - t) * (1.0 - b) * (1.0 - c) / 2.0)
    };
    std::array::from_fn(|k| fa * (cur.value[k] - prev.value[k]) + fb * (next.value[k] - cur.value[k]))
}

/// A B-spline channel: `dim` components, control points already decompressed.
#[derive(Debug, Clone, PartialEq)]
pub struct BSplineChannel {
    pub dim: usize,
    /// `n` control points × `dim` floats.
    pub points: Vec<f32>,
}

/// Degree of the B-splines [hypothesis: cubic; nif.xml's description of the basis
/// ("frames plus degree minus one") does not fix the value; checked indirectly in
/// docs/spec-notes/nif.md §6].
pub const BSPLINE_DEGREE: usize = 3;

impl BSplineChannel {
    pub fn count(&self) -> usize {
        self.points.len() / self.dim.max(1)
    }

    /// Evaluate the open uniform B-spline at normalised time `u` in [0, 1].
    pub fn eval(&self, u: f32) -> Vec<f32> {
        (0..self.dim).map(|c| self.eval_component(u, c)).collect()
    }

    /// [`Self::eval`] for an `N`-component channel without allocating (animation runs
    /// every frame); `None` when the channel does not have `N` components.
    pub fn eval_array<const N: usize>(&self, u: f32) -> Option<[f32; N]> {
        (self.dim == N).then(|| std::array::from_fn(|k| self.eval_component(u, k)))
    }

    /// Component `c` of the spline at `u`: de Boor's algorithm on that component.
    fn eval_component(&self, u: f32, c: usize) -> f32 {
        let n = self.count();
        if n == 0 {
            return 0.0;
        }
        let d = BSPLINE_DEGREE.min(n - 1);
        if d == 0 {
            return self.points[c];
        }
        // Open uniform knot vector: d+1 zeros, uniform interior knots, d+1 maxima.
        let max_knot = (n - d) as f32;
        let knot = |i: usize| -> f32 {
            if i <= d {
                0.0
            } else if i >= n {
                max_knot
            } else {
                (i - d) as f32
            }
        };
        let x = u.clamp(0.0, 1.0) * max_knot;
        // Span index s with knot(s) <= x < knot(s+1), clamped at the end.
        let mut s = d;
        while s < n - 1 && knot(s + 1) <= x {
            s += 1;
        }
        // De Boor's algorithm [documented: de Boor 1972].
        let mut pts = [0.0f32; BSPLINE_DEGREE + 1];
        for (j, p) in pts.iter_mut().enumerate().take(d + 1) {
            *p = self.points[(s - d + j) * self.dim + c];
        }
        for r in 1..=d {
            for j in (r..=d).rev() {
                let i = s - d + j;
                let denom = knot(i + d + 1 - r) - knot(i);
                let alpha = if denom > 0.0 { (x - knot(i)) / denom } else { 0.0 };
                pts[j] = (1.0 - alpha) * pts[j - 1] + alpha * pts[j];
            }
        }
        pts[d]
    }
}

/// One animated property of a transform.
#[derive(Debug, Clone, PartialEq)]
pub enum Channel<const N: usize> {
    /// Not animated by this track: use the interpolator's static pose (if any).
    Static(Option<[f32; N]>),
    Keys(Keys<N>),
    BSpline { start: f32, stop: f32, spline: BSplineChannel },
}

impl<const N: usize> Channel<N> {
    pub fn sample(&self, t: f32) -> Option<[f32; N]> {
        match self {
            Channel::Static(v) => *v,
            Channel::Keys(k) => k.sample(t),
            Channel::BSpline { start, stop, spline } => {
                let span = stop - start;
                let u = if span > 0.0 { (t - start) / span } else { 0.0 };
                spline.eval_array(u)
            }
        }
    }

    pub fn is_animated(&self) -> bool {
        !matches!(self, Channel::Static(_))
    }
}

/// Translation, rotation (quaternion w, x, y, z) and uniform scale over time.
#[derive(Debug, Clone, PartialEq)]
pub struct TransformTrack {
    pub translation: Channel<3>,
    pub rotation: Channel<4>,
    /// Euler XYZ rotation keys (radians), used instead of `rotation` when present.
    pub euler: Option<[Keys<1>; 3]>,
    pub scale: Channel<1>,
}

impl TransformTrack {
    /// Rotation at `t` as a normalised quaternion (w, x, y, z).
    pub fn rotation_at(&self, t: f32) -> Option<[f32; 4]> {
        if let Some(e) = &self.euler {
            let a: [f32; 3] = std::array::from_fn(|k| e[k].sample(t).map_or(0.0, |v| v[0]));
            return Some(euler_xyz_to_quat(a));
        }
        let q = match &self.rotation {
            Channel::Keys(k) => sample_quat_keys(k, t)?,
            other => other.sample(t)?,
        };
        Some(normalize(q))
    }
}

fn normalize(q: [f32; 4]) -> [f32; 4] {
    let len = (q.iter().map(|x| x * x).sum::<f32>()).sqrt();
    if len > 0.0 {
        q.map(|x| x / len)
    } else {
        [1.0, 0.0, 0.0, 0.0]
    }
}

/// Spherical linear interpolation along the shorter arc [documented: Shoemake 1985,
/// "Animating rotation with quaternion curves"].
pub fn slerp(a: [f32; 4], b: [f32; 4], u: f32) -> [f32; 4] {
    let mut dot: f32 = a.iter().zip(&b).map(|(x, y)| x * y).sum();
    let b = if dot < 0.0 {
        dot = -dot;
        b.map(|x| -x)
    } else {
        b
    };
    if dot > 0.9995 {
        return normalize(std::array::from_fn(|k| a[k] + (b[k] - a[k]) * u));
    }
    let theta = dot.clamp(-1.0, 1.0).acos();
    let s = theta.sin();
    let (wa, wb) = (((1.0 - u) * theta).sin() / s, (u * theta).sin() / s);
    std::array::from_fn(|k| wa * a[k] + wb * b[k])
}

/// Quaternion keys: step for constant keys, slerp otherwise. Quadratic and TBC rotation
/// keys are approximated by slerp between neighbouring keys [hypothesis: the original
/// uses a smoother quaternion spline; 180 of 213,701 transform tracks use TBC rotation
/// keys and none use quadratic ones, observed].
pub fn sample_quat_keys(k: &Keys<4>, t: f32) -> Option<[f32; 4]> {
    let ks = &k.keys;
    let first = ks.first()?;
    if ks.len() == 1 || t <= first.time {
        return Some(first.value);
    }
    let last = ks.last().unwrap();
    if t >= last.time {
        return Some(last.value);
    }
    let i = ks.partition_point(|x| x.time <= t).saturating_sub(1).min(ks.len() - 2);
    let (a, b) = (&ks[i], &ks[i + 1]);
    if k.kind == KeyKind::Constant {
        return Some(a.value);
    }
    let span = b.time - a.time;
    let u = if span > 0.0 { (t - a.time) / span } else { 0.0 };
    Some(slerp(a.value, b.value, u))
}

/// Rotation applying X, then Y, then Z about fixed axes [hypothesis: the order used by
/// nif.xml XYZ_ROTATION_KEY data; only a handful of files use it].
pub fn euler_xyz_to_quat(a: [f32; 3]) -> [f32; 4] {
    let (sx, cx) = (a[0] / 2.0).sin_cos();
    let (sy, cy) = (a[1] / 2.0).sin_cos();
    let (sz, cz) = (a[2] / 2.0).sin_cos();
    // q = qz * qy * qx
    [
        cz * cy * cx + sz * sy * sx,
        cz * cy * sx - sz * sy * cx,
        cz * sy * cx + sz * cy * sx,
        sz * cy * cx - cz * sy * sx,
    ]
}

/// What a controlled block drives.
#[derive(Debug, Clone, PartialEq)]
pub enum TrackData {
    Transform(TransformTrack),
    /// An interpolator type the animation layer does not evaluate yet (its type name).
    Other(String),
    /// No interpolator (null reference).
    None,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub node_name: Option<String>,
    pub controller_type: Option<String>,
    pub priority: Option<i64>,
    pub data: TrackData,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sequence {
    pub block: usize,
    pub name: Option<String>,
    pub start_time: f32,
    pub stop_time: f32,
    pub frequency: f32,
    /// nif.xml CycleType: 0 loop, 1 reverse, 2 clamp.
    pub cycle_type: i64,
    pub accum_root_name: Option<String>,
    pub text_keys: Vec<(f32, String)>,
    pub tracks: Vec<Track>,
}

/// Text of a ControlledBlock string: the string itself (20.1.0.1+) or a string-palette
/// offset (older files).
fn cb_string(dec: &Decoded, cb: &Struct, name: &str) -> Option<String> {
    if let Some(t) = cb.text(name) {
        return Some(t.to_string());
    }
    let off = cb.int(&format!("{name}_offset"))?;
    if off < 0 || off == 0xFFFF_FFFF {
        return None;
    }
    let palette = dec.get(cb.block_ref("string_palette")?)?;
    // Offsets count bytes of the on-disk (Windows-1252) palette.
    let bytes = fnv_util::cp1252::encode(palette.text("palette")?)?;
    let start = off as usize;
    if start > bytes.len() {
        return None;
    }
    let end = bytes[start..].iter().position(|&b| b == 0).map_or(bytes.len(), |p| start + p);
    Some(fnv_util::cp1252::decode(&bytes[start..end]))
}

/// Static pose parts of an NiQuatTransform: translation, rotation, scale.
type StaticPose = (Option<[f32; 3]>, Option<[f32; 4]>, Option<[f32; 1]>);

fn quat_transform(s: Option<&Struct>) -> StaticPose {
    let Some(s) = s else { return (None, None, None) };
    // nif.xml marks unused parts of an NiQuatTransform with -FLT_MAX [documented].
    let valid = |x: f32| x > -3.0e38;
    let t = s.floats("translation").and_then(|v| <[f32; 3]>::try_from(v).ok()).filter(|v| v.iter().all(|x| valid(*x)));
    let r = s.floats("rotation").and_then(|v| <[f32; 4]>::try_from(v).ok()).filter(|v| v.iter().all(|x| valid(*x)));
    let sc = s.float("scale").filter(|x| valid(*x)).map(|x| [x]);
    (t, r, sc)
}

fn bspline_channel(
    dec: &Decoded,
    it: &Struct,
    handle: &str,
    dim: usize,
    compact: Option<(&str, &str)>,
) -> Option<BSplineChannel> {
    let h = it.int(handle)?;
    if h == 0xFFFF || h < 0 {
        return None;
    }
    let data = dec.get(it.block_ref("spline_data")?)?;
    let n = dec.get(it.block_ref("basis_data")?)?.int("num_control_points")? as usize;
    let h = h as usize;
    let points: Vec<f32> = match compact {
        Some((off, half)) => {
            let (o, r) = (it.float(off)?, it.float(half)?);
            let cps = data.ints("compact_control_points")?;
            cps.get(h..h + n * dim)?.iter().map(|&c| o + (c as f32 / 32767.0) * r).collect()
        }
        None => data.floats("float_control_points")?.get(h..h + n * dim)?.to_vec(),
    };
    Some(BSplineChannel { dim, points })
}

/// Build the transform track for an interpolator block, if it is a transform type.
pub fn transform_track(dec: &Decoded, idx: i32, type_name: &str) -> Option<TransformTrack> {
    let it = dec.get(idx)?;
    let (t0, r0, s0) = quat_transform(it.strukt("transform"));
    match type_name {
        "NiTransformInterpolator" => {
            let mut track = TransformTrack {
                translation: Channel::Static(t0),
                rotation: Channel::Static(r0),
                euler: None,
                scale: Channel::Static(s0),
            };
            if let Some(data) = it.block_ref("data").and_then(|d| dec.get(d)) {
                if let Some(g) = data.strukt("translations").and_then(Keys::<3>::from_group).filter(|k| !k.is_empty()) {
                    track.translation = Channel::Keys(g);
                }
                if let Some(g) = data.strukt("scales").and_then(Keys::<1>::from_group).filter(|k| !k.is_empty()) {
                    track.scale = Channel::Keys(g);
                }
                let kind = data.int("rotation_type").and_then(KeyKind::from_raw);
                match kind {
                    Some(KeyKind::XyzRotation) => {
                        if let Some(groups) = data.list("xyz_rotations") {
                            let g: Vec<Keys<1>> = groups
                                .iter()
                                .filter_map(|g| match g {
                                    Value::Struct(s) => Keys::<1>::from_group(s),
                                    _ => None,
                                })
                                .collect();
                            if let Ok(g) = <[Keys<1>; 3]>::try_from(g) {
                                track.euler = Some(g);
                            }
                        }
                    }
                    Some(k) => {
                        if let Some(keys) = data.list("quaternion_keys").and_then(|l| Keys::<4>::from_list(k, l)) {
                            if !keys.is_empty() {
                                track.rotation = Channel::Keys(keys);
                            }
                        }
                    }
                    None => {}
                }
            }
            Some(track)
        }
        "NiBSplineCompTransformInterpolator" | "NiBSplineTransformInterpolator" => {
            let comp = type_name == "NiBSplineCompTransformInterpolator";
            let (start, stop) = (it.float("start_time")?, it.float("stop_time")?);
            let ch = |handle: &str, dim: usize, off: &'static str, half: &'static str| {
                bspline_channel(dec, it, handle, dim, comp.then_some((off, half)))
            };
            let wrap3 = |c: Option<BSplineChannel>, s: Option<[f32; 3]>| match c {
                Some(spline) => Channel::BSpline { start, stop, spline },
                None => Channel::Static(s),
            };
            Some(TransformTrack {
                translation: wrap3(ch("translation_handle", 3, "translation_offset", "translation_half_range"), t0),
                rotation: match ch("rotation_handle", 4, "rotation_offset", "rotation_half_range") {
                    Some(spline) => Channel::BSpline { start, stop, spline },
                    None => Channel::Static(r0),
                },
                euler: None,
                scale: match ch("scale_handle", 1, "scale_offset", "scale_half_range") {
                    Some(spline) => Channel::BSpline { start, stop, spline },
                    None => Channel::Static(s0),
                },
            })
        }
        _ => None,
    }
}

/// Every NiControllerSequence in a decoded file.
pub fn sequences(file: &crate::NifFile, dec: &Decoded) -> Vec<Sequence> {
    let mut out = Vec::new();
    for (i, b) in file.blocks.iter().enumerate() {
        if b.type_name != "NiControllerSequence" {
            continue;
        }
        let Ok(s) = &dec.blocks[i] else { continue };
        let mut text_keys = Vec::new();
        if let Some(tk) = s.block_ref("text_keys").and_then(|r| dec.get(r)) {
            for k in tk.list("text_keys").unwrap_or_default() {
                if let Value::Struct(k) = k {
                    text_keys.push((k.float("time").unwrap_or(0.0), k.text("value").unwrap_or_default().to_string()));
                }
            }
        }
        let mut tracks = Vec::new();
        for cb in s.list("controlled_blocks").unwrap_or_default() {
            let Value::Struct(cb) = cb else { continue };
            let interp = cb.block_ref("interpolator").unwrap_or(-1);
            let data = match usize::try_from(interp).ok().and_then(|x| file.blocks.get(x)) {
                None => TrackData::None,
                Some(ib) => match transform_track(dec, interp, &ib.type_name) {
                    Some(t) => TrackData::Transform(t),
                    None => TrackData::Other(ib.type_name.clone()),
                },
            };
            tracks.push(Track {
                node_name: cb_string(dec, cb, "node_name"),
                controller_type: cb_string(dec, cb, "controller_type"),
                priority: cb.int("priority"),
                data,
            });
        }
        out.push(Sequence {
            block: i,
            name: s.text("name").map(str::to_string),
            start_time: s.float("start_time").unwrap_or(0.0),
            stop_time: s.float("stop_time").unwrap_or(0.0),
            frequency: s.float("frequency").unwrap_or(1.0),
            cycle_type: s.int("cycle_type").unwrap_or(0),
            accum_root_name: s.text("accum_root_name").map(str::to_string),
            text_keys,
            tracks,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key<const N: usize>(time: f32, value: [f32; N]) -> Key<N> {
        Key { time, value, forward: [0.0; N], backward: [0.0; N], tbc: [0.0; 3] }
    }

    #[test]
    fn linear_and_constant_keys() {
        let mut k = Keys { kind: KeyKind::Linear, keys: vec![key(0.0, [0.0]), key(2.0, [10.0])] };
        assert_eq!(k.sample(1.0), Some([5.0]));
        assert_eq!(k.sample(-1.0), Some([0.0]));
        assert_eq!(k.sample(3.0), Some([10.0]));
        k.kind = KeyKind::Constant;
        assert_eq!(k.sample(1.9), Some([0.0]));
    }

    #[test]
    fn hermite_hits_endpoints_and_tbc_zero_is_catmull_rom() {
        let k = Keys {
            kind: KeyKind::Tbc,
            keys: vec![key(0.0, [0.0]), key(1.0, [1.0]), key(2.0, [4.0]), key(3.0, [9.0])],
        };
        assert_eq!(k.sample(1.0), Some([1.0]));
        assert_eq!(k.sample(2.0), Some([4.0]));
        // Catmull-Rom between 1 and 4 with tangents 2 and 4 at u=0.5:
        // 0.5·1 + 0.125·2 + 0.5·4 − 0.125·4 = 2.25.
        let v = k.sample(1.5).unwrap()[0];
        assert!((v - 2.25).abs() < 1e-5, "{v}");
    }

    #[test]
    fn bspline_array_matches_vec() {
        let pts: Vec<f32> = (0..21).map(|i| ((i * 7) % 5) as f32 - 0.5 * i as f32).collect();
        let c = BSplineChannel { dim: 3, points: pts };
        for i in 0..=16 {
            let u = i as f32 / 16.0;
            assert_eq!(c.eval_array::<3>(u).unwrap().to_vec(), c.eval(u));
        }
        assert_eq!(c.eval_array::<4>(0.5), None);
        let one = BSplineChannel { dim: 4, points: vec![1.0, 2.0, 3.0, 4.0] };
        assert_eq!(one.eval_array::<4>(0.7), Some([1.0, 2.0, 3.0, 4.0]));
    }

    #[test]
    fn quadratic_tangents_leave_backward_arrive_forward() {
        // The spinning windmill's blades: two turns in 7 s, the tangent leaving the first
        // key in `backward` and the one arriving at the last in `forward`; a steady spin.
        let turn = 4.0 * std::f32::consts::PI;
        let mut a = key(0.0, [0.0]);
        a.backward = [turn];
        let mut b = key(7.0, [turn]);
        b.forward = [turn];
        let k = Keys { kind: KeyKind::Quadratic, keys: vec![a, b] };
        for t in [1.0f32, 1.75, 3.5, 6.0] {
            let v = k.sample(t).unwrap()[0];
            assert!((v - turn * t / 7.0).abs() < 1e-4, "{t}: {v}");
        }
    }

    #[test]
    fn bspline_endpoints_and_constant() {
        let flat = BSplineChannel { dim: 1, points: vec![2.0; 6] };
        for u in [0.0, 0.3, 1.0] {
            assert!((flat.eval(u)[0] - 2.0).abs() < 1e-6);
        }
        // Open uniform splines interpolate their end control points.
        let ramp = BSplineChannel { dim: 2, points: vec![0.0, 5.0, 1.0, 5.0, 2.0, 5.0, 3.0, 5.0, 4.0, 5.0] };
        assert_eq!(ramp.eval(0.0), [0.0, 5.0]);
        let end = ramp.eval(1.0);
        assert!((end[0] - 4.0).abs() < 1e-6 && (end[1] - 5.0).abs() < 1e-6, "{end:?}");
        // A linear control polygon gives a monotone curve.
        let mut last = -1.0;
        for i in 0..=20 {
            let v = ramp.eval(i as f32 / 20.0)[0];
            assert!(v >= last);
            last = v;
        }
    }

    #[test]
    fn slerp_takes_short_arc() {
        let a = [1.0, 0.0, 0.0, 0.0];
        let b = [-1.0, 0.0, 0.0, 0.0]; // the same rotation, opposite sign
        let m = slerp(a, b, 0.5);
        let n: f32 = m.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((n - 1.0).abs() < 1e-5);
        let half = std::f32::consts::FRAC_1_SQRT_2;
        let q90 = [half, 0.0, 0.0, half]; // 90° about z
        let q45 = slerp(a, q90, 0.5);
        assert!((q45[0] - (std::f32::consts::PI / 8.0).cos()).abs() < 1e-5, "{q45:?}");
    }

    #[test]
    fn euler_identity() {
        let q = euler_xyz_to_quat([0.0, 0.0, 0.0]);
        assert_eq!(q, [1.0, 0.0, 0.0, 0.0]);
        let q = euler_xyz_to_quat([0.0, 0.0, std::f32::consts::PI]);
        assert!((q[3] - 1.0).abs() < 1e-6);
    }
}
