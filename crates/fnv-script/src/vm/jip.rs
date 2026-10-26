//! The JIP LN plugin's functions, as NVForever's own module (tasks J4, J5 of
//! docs/handoff/jip-tasks.md; docs/spec-notes/jip-*.md). Catalogue origins `JIP` and `LU`
//! (lutana's functions, merged into JIP LN). Written from the wiki export's pages and the
//! peer session's runs in the original game with JIP LN 57.30 [observed rows]; no plugin
//! code is read.

use super::{Vm, VmError, XVal};
use crate::ir::{XCall, XExpr};

/// The version NVForever reports for JIP LN: the one these tasks follow [observed:
/// GetPluginVersion "JIP NVSE Plugin" and GetLNVersion give 57.3 on the owner's install].
pub const JIP_VERSION: f64 = 57.3;

/// The plugin names JIP LN registers [documented: ext-version.md §2a].
pub const JIP_NAMES: [&str; 1] = ["JIP NVSE Plugin"];

impl Vm<'_, '_> {
    /// A JIP LN function by catalogue name; `None` when it is not built yet.
    pub(super) fn jip_function(&mut self, n: &str, c: &XCall) -> Result<Option<XVal>, VmError> {
        let val = |vm: &mut Self, k: usize| -> Result<XVal, VmError> {
            match c.args.get(k) {
                Some(a) => vm.xeval(a),
                None => Err(VmError::Expression(format!("{n} without argument {}", k + 1))),
            }
        };
        let opt = |vm: &mut Self, k: usize| -> Result<Option<XVal>, VmError> { c.args.get(k).map(|a| vm.xeval(a)).transpose() };
        let text = |v: XVal| -> Option<String> { if let XVal::Str(s) = v { Some(s) } else { None } };
        let num = |x: f64| Ok(Some(XVal::Num(x)));
        match n {
            // [documented: Fsin; Fcos; Ftan; observed: fsin 90 = 1] degrees.
            "fsin" | "fcos" | "ftan" => {
                let d = val(self, 0)?.num()?.to_radians();
                num(match n {
                    "fsin" => d.sin(),
                    "fcos" => d.cos(),
                    _ => d.tan(),
                })
            }
            // [documented: Fasin; Facos; Fatan; observed: fasin 2 = 90] degrees, the input
            // held to ±1 [hypothesis from the clamp observed].
            "fasin" | "facos" | "fatan" => {
                let x = val(self, 0)?.num()?;
                num(match n {
                    "fasin" => x.clamp(-1.0, 1.0).asin().to_degrees(),
                    "facos" => x.clamp(-1.0, 1.0).acos().to_degrees(),
                    _ => x.atan().to_degrees(),
                })
            }
            // [documented: Fatan2; observed: (1, -1) = 135, (-1, -1) = -135].
            "fatan2" => {
                let (y, x) = (val(self, 0)?.num()?, val(self, 1)?.num()?);
                num(y.atan2(x).to_degrees())
            }
            // [documented: Fsqrt; observed: fsqrt -1 = 0].
            "fsqrt" => {
                let x = val(self, 0)?.num()?;
                num(if x < 0.0 { 0.0 } else { x.sqrt() })
            }
            // [documented: GetMaxOf; GetMinOf] 2 to 5 values.
            "getmaxof" | "getminof" => {
                let mut best: Option<f64> = None;
                for k in 0..c.args.len().min(5) {
                    let x = val(self, k)?.num()?;
                    best = Some(match best {
                        None => x,
                        Some(b) if n == "getmaxof" => b.max(x),
                        Some(b) => b.min(x),
                    });
                }
                num(best.unwrap_or(0.0))
            }
            // [documented: GetRandomInRange; observed: (5, 6) = 5] min ≤ x < max.
            "getrandominrange" => {
                let (a, b) = (val(self, 0)?.num()?.trunc(), val(self, 1)?.num()?.trunc());
                let r = self.host.random();
                num(if b <= a { a } else { (a + ((b - a) * r).floor()).min(b - 1.0) })
            }
            // [documented: StringToActorValue; observed: case ignored, -1 for no name].
            "stringtoactorvalue" => {
                let s = text(val(self, 0)?).unwrap_or_default();
                num(self.host.actor_value_code(&s).map_or(-1.0, f64::from))
            }
            // [documented: GetModName; observed: 0 → "FalloutNV", (0, 1) → "FalloutNV.esm",
            // 99 → ""].
            "getmodname" => {
                let i = val(self, 0)?.num()?;
                let keep = opt(self, 1)?.is_some_and(|v| v.truthy());
                let name = if i < 0.0 { None } else { self.host.mod_name(i as u32) };
                let name = name.map(|n| if keep { n } else { n.rsplit_once('.').map_or(n.clone(), |(stem, _)| stem.to_string()) });
                Ok(Some(XVal::Str(name.unwrap_or_default())))
            }
            // [documented: ValidateModIndex; observed: 0 → 0, 200 → -1].
            "validatemodindex" => {
                let i = val(self, 0)?.num()?;
                let ok = i >= 0.0 && self.host.mod_name(i as u32).is_some();
                num(if ok { i.trunc() } else { -1.0 })
            }
            // [documented: GetFormFromMod; observed: "14" and "000014" give the player].
            "getformfrommod" | "gffm" => {
                let (p, h) = (text(val(self, 0)?).unwrap_or_default(), text(val(self, 1)?).unwrap_or_default());
                Ok(Some(XVal::Ref(self.plugin_form(&p, &h).unwrap_or(0))))
            }
            // [documented: StringToRef; observed: "00000014" gives the player] RefToString's
            // "Plugin:hex", or a whole FormID in hex.
            "stringtoref" => {
                let s = text(val(self, 0)?).unwrap_or_default();
                let f = match s.split_once(':') {
                    Some((p, h)) => self.plugin_form(p, h),
                    None => u32::from_str_radix(s.trim(), 16).ok().filter(|&f| self.host.form_sig(f).is_some()),
                };
                Ok(Some(XVal::Ref(f.unwrap_or(0))))
            }
            // [documented: RefToString; observed: "FalloutNV.esm:104C0C"] upper-case hex
            // without the mod index or leading zeros.
            "reftostring" => {
                let f = match c.args.first() {
                    Some(a) => self.xeval(a)?.value()?.as_form(),
                    None => match &c.on {
                        Some(e) => self.xeval(e)?.value()?.as_form(),
                        None => None,
                    },
                };
                let Some(f) = f else { return Ok(Some(XVal::Str(String::new()))) };
                let plugin = self.host.mod_name(f >> 24).unwrap_or_default();
                Ok(Some(XVal::Str(format!("{plugin}:{:X}", f & 0x00FF_FFFF))))
            }
            // What NVForever reports for JIP LN (task J4) [observed: GetLNVersion 57.3 on the
            // owner's install, though its page says 0; GetNVSEVersionFull 6.49 on xNVSE
            // 6.4.9].
            "getlnversion" | "getjiplnversion" => num(JIP_VERSION),
            "getnvseversionfull" => num(6.49),
            // [documented: GetLocalGravity; observed: X 0, Z -98.1] SetLocalGravityVector is
            // not built, so the cell's gravity is the default.
            "getlocalgravity" => {
                let axis = match c.args.first() {
                    Some(XExpr::Axis { c }) => *c,
                    Some(a) => match self.xeval(a)? {
                        XVal::Str(s) => s.chars().next().unwrap_or('Z').to_ascii_uppercase(),
                        _ => 'Z',
                    },
                    None => 'Z',
                };
                num(if axis == 'Z' { -98.1 } else { 0.0 })
            }
            _ => Ok(None),
        }
    }

    /// The form `hex` (without the mod index) of plugin `name`.
    fn plugin_form(&mut self, name: &str, hex: &str) -> Option<u32> {
        let i = self.host.mod_index(name.trim())?;
        let low = u32::from_str_radix(hex.trim(), 16).ok()? & 0x00FF_FFFF;
        let f = (i << 24) | low;
        self.host.form_sig(f).map(|_| f)
    }
}
