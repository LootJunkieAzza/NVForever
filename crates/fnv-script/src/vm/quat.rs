//! xNVSE's quaternion and vector functions (QFromEuler … V3Crossproduct). They write their
//! results into the calling script's variables named by the first string arguments, W X
//! Y Z for a quaternion, X Y Z for a vector [observed: the peer's nvse-math probes in the
//! original game, 2026-10-26; the conventions below are fitted to its values]. Angles are
//! degrees.

use super::{Vm, VmError, XVal};
use crate::ir::XCall;

type Quat = [f64; 4];

/// The Hamilton product a·b (w, x, y, z).
fn mul(a: Quat, b: Quat) -> Quat {
    let [w1, x1, y1, z1] = a;
    let [w2, x2, y2, z2] = b;
    [
        w1 * w2 - x1 * x2 - y1 * y2 - z1 * z2,
        w1 * x2 + x1 * w2 + y1 * z2 - z1 * y2,
        w1 * y2 - x1 * z2 + y1 * w2 + z1 * x2,
        w1 * z2 + x1 * y2 - y1 * x2 + z1 * w2,
    ]
}

fn axis(i: usize, deg: f64) -> Quat {
    let h = deg.to_radians() / 2.0;
    let mut q = [h.cos(), 0.0, 0.0, 0.0];
    q[i + 1] = h.sin();
    q
}

fn normalize(q: Quat) -> Quat {
    let n = q.iter().map(|c| c * c).sum::<f64>().sqrt();
    if n == 0.0 { q } else { q.map(|c| c / n) }
}

impl Vm<'_, '_> {
    /// The quaternion and vector functions; `None` for any other name.
    pub(super) fn quat_function(&mut self, n: &str, c: &XCall) -> Result<Option<XVal>, VmError> {
        let outs = match n {
            "qfromeuler" | "qmultquatquat" | "qnormalize" | "qinterpolate" | "qfromaxisangle" => 4,
            "qtoeuler" | "qmultquatvector3" | "v3normalize" | "v3crossproduct" => 3,
            _ => return Ok(None),
        };
        let mut names = Vec::new();
        for k in 0..outs {
            match c.args.get(k).map(|a| self.xeval(a)).transpose()? {
                Some(XVal::Str(s)) => names.push(s),
                _ => return Err(VmError::Type(format!("{n} needs {outs} variable names"))),
            }
        }
        let mut nums = Vec::new();
        for a in c.args.iter().skip(outs) {
            nums.push(self.xeval(a)?.num()?);
        }
        let at = |k: usize| nums.get(k).copied().unwrap_or(0.0);
        let q4 = |k: usize| [at(k), at(k + 1), at(k + 2), at(k + 3)];
        let out: Vec<f64> = match n {
            // Z·Y·X; with pitchYawOnly X·Z, Y left out [observed: x90 0.7071 0.7071 0 0;
            // (30,45,60) 0.8224 0.0223 0.4397 0.3604, pitch-yaw 0.8365 0.2241 -0.1294
            // 0.4830].
            "qfromeuler" => {
                let (x, y, z) = (at(0), at(1), at(2));
                if at(3) != 0.0 { mul(axis(0, x), axis(2, z)) } else { mul(mul(axis(2, z), axis(1, y)), axis(0, x)) }.to_vec()
            }
            // The inverse of Z·Y·X [observed: (0.7071,0,0,0.7071) gives 0 -0 90; (0.8224,
            // 0.0222,0.4396,0.3604) 29.983 44.997 59.987]; pitch-yaw the inverse of X·Z
            // [hypothesis].
            // The quaternion normalised first [observed: an unnormalised input gives the
            // normalised angles].
            "qtoeuler" => {
                let [w, x, y, z] = normalize(q4(0));
                if at(4) != 0.0 {
                    vec![2.0 * x.atan2(w).to_degrees(), 0.0, 2.0 * z.atan2(w).to_degrees()]
                } else {
                    let rx = (2.0 * (w * x + y * z)).atan2(1.0 - 2.0 * (x * x + y * y));
                    // Written as -2(xz - wy): the game prints -0 for z90 [observed].
                    let ry = (-2.0 * (x * z - w * y)).clamp(-1.0, 1.0).asin();
                    let rz = (2.0 * (w * z + x * y)).atan2(1.0 - 2.0 * (y * y + z * z));
                    vec![rx.to_degrees(), ry.to_degrees(), rz.to_degrees()]
                }
            }
            // B·A [observed: A x90, B z90 gives 0.5 0.5 0.5 0.5].
            "qmultquatquat" => mul(q4(4), q4(0)).to_vec(),
            // By the conjugate [observed: z90 on (1,0,0) gives (0,-1,0)].
            "qmultquatvector3" => {
                let q = q4(0);
                let conj = [q[0], -q[1], -q[2], -q[3]];
                let r = mul(mul(conj, [0.0, at(4), at(5), at(6)]), q);
                vec![r[1], r[2], r[3]]
            }
            "qnormalize" => normalize(q4(0)).to_vec(),
            // Spherical between A and B at the ratio [observed: identity to z90 at 0.5
            // gives 0.9239 0 0 0.3827, with and without useSLI; hypothesis: both slerp].
            "qinterpolate" => {
                let (a, mut b, t) = (normalize(q4(0)), normalize(q4(4)), at(8));
                let mut d: f64 = a.iter().zip(&b).map(|(x, y)| x * y).sum();
                if d < 0.0 {
                    b = b.map(|v| -v);
                    d = -d;
                }
                if d > 0.9995 {
                    normalize(std::array::from_fn(|i| a[i] + t * (b[i] - a[i]))).to_vec()
                } else {
                    let th = d.acos();
                    let (sa, sb) = (((1.0 - t) * th).sin() / th.sin(), (t * th).sin() / th.sin());
                    (0..4).map(|i| sa * a[i] + sb * b[i]).collect()
                }
            }
            // [observed: (0,0,1) 90 gives 0.7071 0 0 0.7071].
            "qfromaxisangle" => {
                let v = [at(0), at(1), at(2)];
                let len = v.iter().map(|c| c * c).sum::<f64>().sqrt();
                let h = at(3).to_radians() / 2.0;
                let s = if len == 0.0 { 0.0 } else { h.sin() / len };
                vec![h.cos(), v[0] * s, v[1] * s, v[2] * s]
            }
            "v3normalize" => {
                let len = (at(0) * at(0) + at(1) * at(1) + at(2) * at(2)).sqrt();
                if len == 0.0 { vec![0.0; 3] } else { vec![at(0) / len, at(1) / len, at(2) / len] }
            }
            // B×A [observed: (1,0,0), (0,1,0) gives (0,0,-1)].
            _ => {
                let (a, b) = ([at(0), at(1), at(2)], [at(3), at(4), at(5)]);
                vec![b[1] * a[2] - b[2] * a[1], b[2] * a[0] - b[0] * a[2], b[0] * a[1] - b[1] * a[0]]
            }
        };
        for (name, v) in names.iter().zip(out) {
            let var = self.host.local_var(name).ok_or_else(|| VmError::Type(format!("{n}: the script has no variable {name}")))?;
            self.write_var(&var, XVal::Num(v))?;
        }
        Ok(Some(XVal::Num(0.0)))
    }
}
