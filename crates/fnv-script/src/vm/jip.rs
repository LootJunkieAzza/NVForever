//! The JIP LN plugin's functions, as NVForever's own module (tasks J4, J5 of
//! docs/handoff/jip-tasks.md; docs/spec-notes/jip-*.md). Catalogue origins `JIP` and `LU`
//! (lutana's functions, merged into JIP LN). Written from the wiki export's pages and the
//! peer session's runs in the original game with JIP LN 57.30 [observed rows]; no plugin
//! code is read.

use super::{ArrayKind, ExtStore, Vm, VmError, XKey, XVal};
use crate::ir::{XCall, XExpr};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// JIP LN's auxiliary variables and RefMap arrays [documented: jip-variable-stores.md].
/// Names are kept lower-cased: they ignore case [observed: aux.case].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct JipStores {
    /// Owner form -> name -> elements (each a number, a form or a string).
    #[serde(default)]
    pub aux: BTreeMap<u32, BTreeMap<String, Vec<XVal>>>,
    /// Name -> (key form, value), in the order the keys were first set [decision:
    /// jip-variable-stores.md §3].
    #[serde(default)]
    pub refmaps: BTreeMap<String, Vec<(u32, XVal)>>,
    /// Where RefMapArrayGetNext goes on from, per map.
    #[serde(skip)]
    cursor: BTreeMap<String, usize>,
}

impl JipStores {
    pub fn is_empty(&self) -> bool {
        self.aux.is_empty() && self.refmaps.is_empty()
    }
}

/// The store a name lives in: names starting `*` are temporary, not saved [observed:
/// aux.class.*; documented by the class names].
fn stores<'s>(x: &'s mut ExtStore, name: &str) -> &'s mut JipStores {
    if name.starts_with('*') { &mut x.jip_temp } else { &mut x.jip }
}

/// The element type codes of AuxiliaryVariableGetType and RefMapArrayGetType
/// [documented]: 0 none, 1 number, 2 form, 4 string.
fn type_code(v: Option<&XVal>) -> f64 {
    match v {
        Some(XVal::Num(_)) => 1.0,
        Some(XVal::Ref(_)) => 2.0,
        Some(XVal::Str(_)) => 4.0,
        _ => 0.0,
    }
}

/// What a typed getter gives for an element: the element when its type matches, else
/// 0, the empty string or no form [observed: aux.float.of.string].
fn typed(v: Option<&XVal>, want: char) -> XVal {
    match (want, v) {
        ('f', Some(x @ XVal::Num(_))) | ('r', Some(x @ XVal::Ref(_))) | ('s', Some(x @ XVal::Str(_))) => x.clone(),
        ('f', _) => XVal::Num(0.0),
        ('r', _) => XVal::Ref(0),
        _ => XVal::Str(String::new()),
    }
}

/// The type letter of a setter or getter name ("setfloat" -> 'f', "getref" -> 'r').
fn kind_of(op: &str) -> char {
    op.as_bytes().get(3).map_or('f', |b| *b as char)
}

/// Drop an emptied variable, and an owner left with none.
fn prune(s: &mut JipStores, owner: u32, name: &str) {
    if let Some(m) = s.aux.get_mut(&owner) {
        if m.get(name).is_some_and(Vec::is_empty) {
            m.remove(name);
        }
        if m.is_empty() {
            s.aux.remove(&owner);
        }
    }
}

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
            // Record traits and flags (jip-records.md): each number is the decoded field
            // the original's value equalled [observed: jip-records rows]. A number outside
            // the table gives 0 [hypothesis].
            "getprojectiletraitnumeric" | "getexplosiontraitnumeric" | "getammotraitnumeric" | "getexplosiontraitform" | "getprojectileflag" | "getexplosionflag" => {
                let f = val(self, 0)?.value()?.as_form().unwrap_or(0);
                let k = val(self, 1)?.num()? as i64;
                let field = match (n, k) {
                    ("getprojectiletraitnumeric", 0) => Some(("DATA", "type")),
                    ("getprojectiletraitnumeric", 1) => Some(("DATA", "gravity")),
                    ("getprojectiletraitnumeric", 2) => Some(("DATA", "speed")),
                    ("getprojectiletraitnumeric", 3) => Some(("DATA", "range")),
                    ("getprojectiletraitnumeric", 4) => Some(("DATA", "tracer_chance")),
                    ("getprojectiletraitnumeric", 5) => Some(("DATA", "explosion_alt_trigger_proximity")),
                    ("getprojectiletraitnumeric", 6) => Some(("DATA", "explosion_alt_trigger_timer")),
                    ("getprojectiletraitnumeric", 7) => Some(("DATA", "muzzle_flash_duration")),
                    ("getprojectiletraitnumeric", 8) => Some(("DATA", "fade_duration")),
                    ("getprojectiletraitnumeric", 9) => Some(("DATA", "impact_force")),
                    ("getprojectiletraitnumeric", 10) => Some(("DATA", "rotation.x")),
                    ("getprojectiletraitnumeric", 11) => Some(("DATA", "rotation.y")),
                    ("getprojectiletraitnumeric", 12) => Some(("DATA", "rotation.z")),
                    ("getprojectiletraitnumeric", 13) => Some(("DATA", "bouncy_mult")),
                    ("getprojectiletraitnumeric", 14) => Some(("VNAM", "")),
                    ("getexplosiontraitnumeric", 0) => Some(("DATA", "force")),
                    ("getexplosiontraitnumeric", 1) => Some(("DATA", "damage")),
                    ("getexplosiontraitnumeric", 2) => Some(("DATA", "radius")),
                    ("getexplosiontraitnumeric", 3) => Some(("DATA", "is_radius")),
                    ("getexplosiontraitnumeric", 4) => Some(("DATA", "radiation_level")),
                    ("getexplosiontraitnumeric", 5) => Some(("DATA", "radiation_dissipation_time")),
                    ("getexplosiontraitnumeric", 6) => Some(("DATA", "radiation_radius")),
                    ("getexplosiontraitnumeric", 7) => Some(("DATA", "sound_level")),
                    ("getexplosiontraitform", 0) => Some(("DATA", "light")),
                    ("getexplosiontraitform", 1) => Some(("DATA", "sound_1")),
                    ("getexplosiontraitform", 2) => Some(("DATA", "sound_2")),
                    ("getexplosiontraitform", 3) => Some(("DATA", "impact_dataset")),
                    ("getexplosiontraitform", 4) => Some(("INAM", "")),
                    ("getexplosiontraitform", 5) => Some(("EITM", "")),
                    ("getexplosiontraitform", 6) => Some(("MNAM", "")),
                    ("getammotraitnumeric", 0) => Some(("DATA", "speed")),
                    ("getammotraitnumeric", 1) => Some(("DAT2", "projectiles_per_shot")),
                    ("getammotraitnumeric", 2) => Some(("DAT2", "consumed_percentage")),
                    ("getammotraitnumeric", 3) => Some(("DATA", "clip_rounds")),
                    ("getprojectileflag" | "getexplosionflag", _) => Some(("DATA", "flags")),
                    _ => None,
                };
                let v = field.and_then(|(sub, path)| self.host.record_field(f, sub, path));
                Ok(Some(match n {
                    // Bit n of the flags [observed: projflag.*, explflag.*]; explosion IDs
                    // start at 1.
                    "getprojectileflag" | "getexplosionflag" => {
                        let bits = v.map_or(Ok(0.0), |v| v.num())? as u64;
                        let on = (0..64).contains(&k) && !(n == "getexplosionflag" && k == 0) && bits & (1 << k) != 0;
                        XVal::Num(f64::from(u8::from(on)))
                    }
                    "getexplosiontraitform" => match v {
                        Some(XVal::Ref(r)) => XVal::Ref(r),
                        _ => XVal::Ref(0),
                    },
                    _ => match v {
                        Some(XVal::Num(x)) => XVal::Num(x),
                        _ => XVal::Num(0.0),
                    },
                }))
            }
            // [observed: projexpl.*, projlight.slug, ammoproj.*] form fields.
            "getprojectileexplosion" | "getprojectilelight" | "getammoprojectile" => {
                let f = val(self, 0)?.value()?.as_form().unwrap_or(0);
                let (sub, path) = match n {
                    "getprojectileexplosion" => ("DATA", "explosion"),
                    "getprojectilelight" => ("DATA", "light"),
                    _ => ("DAT2", "projectile"),
                };
                Ok(Some(match self.host.record_field(f, sub, path) {
                    Some(XVal::Ref(r)) => XVal::Ref(r),
                    _ => XVal::Ref(0),
                }))
            }
            "sv_regexmatch" | "sv_regexsearch" | "sv_regexreplace" | "validateregex" => self.regex_function(n, c).map(Some),
            _ => self.jip_store_function(n, c),
        }
    }

    /// The regular-expression functions (jip-strings.md §1). The dialect is ECMAScript's
    /// [hypothesis from ValidateRegex's error kinds]; `fancy-regex` has its groups,
    /// classes and back-references. An invalid pattern is a run-time error [decision: the
    /// original crashes, documented].
    pub(super) fn regex_function(&mut self, n: &str, c: &XCall) -> Result<XVal, VmError> {
        let target = |vm: &mut Self| -> Result<String, VmError> {
            match c.args.first() {
                Some(a) => Ok(match vm.xeval(a)? {
                    XVal::Str(s) => s,
                    other => vm.text_of(&other)?,
                }),
                None => Err(VmError::Expression(format!("{n} without a string"))),
            }
        };
        let build = |p: &str| fancy_regex::Regex::new(p).map_err(|e| VmError::Type(format!("{n}: the pattern {p:?} is not valid ({e})")));
        Ok(match n {
            // [documented: ValidateRegex; observed: "[" gives 5] 0 when valid. Which
            // fancy-regex error is which code is our reading of the error names
            // [hypothesis].
            "validateregex" => {
                let p = match c.args.first() {
                    Some(a) => match self.xeval(a)? {
                        XVal::Str(s) => s,
                        _ => return Ok(XVal::Num(-1.0)),
                    },
                    None => return Ok(XVal::Num(-1.0)),
                };
                match fancy_regex::Regex::new(&p) {
                    Ok(_) => XVal::Num(0.0),
                    Err(e) => {
                        let msg = e.to_string();
                        if let Some(XExpr::Var { var, .. }) = c.args.get(1) {
                            self.write_var(var, XVal::Str(msg.clone()))?;
                        }
                        XVal::Num(regex_error_code(&msg))
                    }
                }
            }
            // [observed: regex.match, regex.partial] the whole target must match.
            "sv_regexmatch" => {
                let t = target(self)?;
                let (p, _) = self.format_arg(c, 1)?;
                let re = build(&format!("^(?:{p})$"))?;
                XVal::Num(f64::from(u8::from(re.is_match(&t).map_err(|e| VmError::Type(e.to_string()))?)))
            }
            // [documented: Sv RegexSearch; observed: regex.search] every match, in order.
            "sv_regexsearch" => {
                let t = target(self)?;
                let (p, _) = self.format_arg(c, 1)?;
                let re = build(&p)?;
                let mut found = Vec::new();
                for m in re.find_iter(&t) {
                    found.push(XVal::Str(m.map_err(|e| VmError::Type(e.to_string()))?.as_str().to_string()));
                }
                self.jlist(found)?
            }
            // [documented: Sv RegexReplace; observed: regex.replace] "replacement|pattern"
            // as one string; split at the first `|` [hypothesis].
            "sv_regexreplace" => {
                let t = target(self)?;
                let (both, _) = self.format_arg(c, 1)?;
                let Some((with, p)) = both.split_once('|') else {
                    return Err(VmError::Type(format!("{n} needs \"replacement|pattern\"")));
                };
                let re = build(p)?;
                XVal::Str(re.replace_all(&t, with).into_owned())
            }
            _ => unreachable!("routed by name"),
        })
    }

    /// Argument `k`, or `None` when left out.
    fn jarg(&mut self, c: &XCall, k: usize) -> Result<Option<XVal>, VmError> {
        c.args.get(k).map(|a| self.xeval(a)).transpose()
    }

    /// The owner (aux) or key (RefMap) given as argument `k`, else the reference the
    /// function is called on, else the calling reference.
    fn jowner(&mut self, n: &str, c: &XCall, k: usize) -> Result<u32, VmError> {
        if let Some(f) = match self.jarg(c, k)? {
            Some(v) => v.value()?.as_form(),
            None => None,
        } {
            return Ok(f);
        }
        if let Some(e) = &c.on {
            if let Some(f) = self.xeval(e)?.value()?.as_form() {
                return Ok(f);
            }
        }
        self.host.calling_ref().ok_or_else(|| VmError::Type(format!("{n} without a reference or a form")))
    }

    /// Argument `k` as an index (`dflt` when left out).
    fn jindex(&mut self, c: &XCall, k: usize, dflt: i64) -> Result<i64, VmError> {
        Ok(match self.jarg(c, k)? {
            Some(v) => v.num()? as i64,
            None => dflt,
        })
    }

    /// A setter's value (argument 1) as the type its name gives.
    fn jvalue(&mut self, n: &str, c: &XCall, kind: char) -> Result<XVal, VmError> {
        let v = self.jarg(c, 1)?.ok_or_else(|| VmError::Expression(format!("{n} without a value")))?;
        Ok(match kind {
            'f' => XVal::Num(v.num()?),
            'r' => XVal::Ref(v.value()?.as_form().unwrap_or(0)),
            _ => match v {
                XVal::Str(s) => XVal::Str(s),
                other => XVal::Str(self.text_of(&other)?),
            },
        })
    }

    /// A list array of `items`.
    fn jlist(&mut self, items: Vec<XVal>) -> Result<XVal, VmError> {
        let entries = items.into_iter().enumerate().map(|(i, v)| (XKey::Num(i as f64), v)).collect();
        Ok(XVal::Array(self.store()?.new_array(ArrayKind::List, entries)))
    }

    /// The auxiliary-variable and RefMap functions (jip-variable-stores.md), by their
    /// full names; aliases reach here through the catalogue.
    fn jip_store_function(&mut self, n: &str, c: &XCall) -> Result<Option<XVal>, VmError> {
        let (aux, op) = match (n.strip_prefix("auxiliaryvariable"), n.strip_prefix("refmaparray")) {
            (Some(op), _) => (true, op),
            (_, Some(op)) => (false, op),
            _ => return Ok(None),
        };
        if aux && op == "getall" {
            return self.aux_get_all(c).map(Some);
        }
        let name = match self.jarg(c, 0)? {
            Some(XVal::Str(s)) => s.to_lowercase(),
            _ => return Err(VmError::Type(format!("{n} without a name"))),
        };
        if aux {
            return self.aux_function(n, op, &name, c);
        }
        let v = match op {
            "setfloat" | "setref" | "setstring" => {
                let v = self.jvalue(n, c, kind_of(op))?;
                let key = self.jowner(n, c, 2)?;
                let m = stores(self.store()?, &name).refmaps.entry(name.clone()).or_default();
                match m.iter_mut().find(|(k, _)| *k == key) {
                    Some(e) => e.1 = v,
                    None => m.push((key, v)),
                }
                XVal::Num(1.0)
            }
            "getfloat" | "getref" | "getstring" | "gettype" => {
                let key = self.jowner(n, c, 1)?;
                let m = stores(self.store()?, &name).refmaps.get(&name);
                let v = m.and_then(|m| m.iter().find(|(k, _)| *k == key)).map(|(_, v)| v);
                if op == "gettype" { XVal::Num(type_code(v)) } else { typed(v, kind_of(op)) }
            }
            "getsize" => XVal::Num(stores(self.store()?, &name).refmaps.get(&name).map_or(0, Vec::len) as f64),
            "getkeys" => {
                let keys = stores(self.store()?, &name).refmaps.get(&name).map(|m| m.iter().map(|(k, _)| XVal::Ref(*k)).collect()).unwrap_or_default();
                self.jlist(keys)?
            }
            "getall" => {
                let pairs = stores(self.store()?, &name).refmaps.get(&name).cloned().unwrap_or_default();
                let entries = pairs.into_iter().map(|(k, v)| (XKey::Num(f64::from(k)), v)).collect();
                XVal::Array(self.store()?.new_array(ArrayKind::Map, entries))
            }
            // [documented: RefMapArrayGetNext; observed: Next after the last pair is
            // ar_Null] a [key, value] list.
            "getfirst" | "getnext" => {
                let s = stores(self.store()?, &name);
                let at = if op == "getfirst" { 0 } else { s.cursor.get(&name).copied().unwrap_or(0) };
                let pair = s.refmaps.get(&name).and_then(|m| m.get(at)).cloned();
                s.cursor.insert(name.clone(), at + 1);
                match pair {
                    Some((k, v)) => self.jlist(vec![XVal::Ref(k), v])?,
                    None => XVal::Array(0),
                }
            }
            // 1 when the key was there [hypothesis: the page gives no return].
            "erase" => {
                let key = self.jowner(n, c, 1)?;
                let s = stores(self.store()?, &name);
                let mut gone = false;
                if let Some(m) = s.refmaps.get_mut(&name) {
                    let before = m.len();
                    m.retain(|(k, _)| *k != key);
                    gone = m.len() < before;
                    if m.is_empty() {
                        s.refmaps.remove(&name);
                    }
                }
                XVal::Num(f64::from(u8::from(gone)))
            }
            "destroy" => {
                let s = stores(self.store()?, &name);
                s.cursor.remove(&name);
                XVal::Num(f64::from(u8::from(s.refmaps.remove(&name).is_some())))
            }
            // [documented: RefMapArrayValidate; observed: a live key gives 1] the keys left.
            "validate" => {
                let keys: Vec<u32> = stores(self.store()?, &name).refmaps.get(&name).map(|m| m.iter().map(|(k, _)| *k).collect()).unwrap_or_default();
                let mut dead = Vec::new();
                for k in keys {
                    if self.host.form_sig(k).is_none() {
                        dead.push(k);
                    }
                }
                let s = stores(self.store()?, &name);
                if let Some(m) = s.refmaps.get_mut(&name) {
                    m.retain(|(k, _)| !dead.contains(k));
                }
                XVal::Num(s.refmaps.get(&name).map_or(0, Vec::len) as f64)
            }
            _ => return Ok(None),
        };
        Ok(Some(v))
    }

    /// AuxiliaryVariableSet…/Get…/GetType/GetSize/Erase/GetAsArray.
    fn aux_function(&mut self, n: &str, op: &str, name: &str, c: &XCall) -> Result<Option<XVal>, VmError> {
        let v = match op {
            // [documented: AuxiliaryVariableSetFloat; observed: -1 appends, an index past
            // the end does nothing, a new type replaces the element].
            "setfloat" | "setref" | "setstring" => {
                let v = self.jvalue(n, c, kind_of(op))?;
                let i = self.jindex(c, 2, 0)?;
                let o = self.jowner(n, c, 3)?;
                let s = stores(self.store()?, name);
                let list = s.aux.entry(o).or_default().entry(name.to_string()).or_default();
                let ok = match usize::try_from(i) {
                    _ if i == -1 => {
                        list.push(v);
                        true
                    }
                    Ok(at) if at < list.len() => {
                        list[at] = v;
                        true
                    }
                    Ok(at) if at == list.len() => {
                        list.push(v);
                        true
                    }
                    _ => false,
                };
                prune(s, o, name);
                XVal::Num(f64::from(u8::from(ok)))
            }
            "getfloat" | "getref" | "getstring" | "gettype" => {
                let i = self.jindex(c, 1, 0)?;
                let o = self.jowner(n, c, 2)?;
                let list = stores(self.store()?, name).aux.get(&o).and_then(|m| m.get(name));
                let v = list.and_then(|l| if i == -1 { l.last() } else { usize::try_from(i).ok().and_then(|i| l.get(i)) });
                if op == "gettype" { XVal::Num(type_code(v)) } else { typed(v, kind_of(op)) }
            }
            "getsize" => {
                let o = self.jowner(n, c, 1)?;
                XVal::Num(stores(self.store()?, name).aux.get(&o).and_then(|m| m.get(name)).map_or(0, Vec::len) as f64)
            }
            // [documented: AuxiliaryVariableErase; observed: the new count, later elements
            // move down; the whole variable gives -1].
            "erase" => {
                let i = self.jindex(c, 1, -1)?;
                let o = self.jowner(n, c, 2)?;
                let s = stores(self.store()?, name);
                let left = match s.aux.get_mut(&o) {
                    Some(m) if i == -1 => {
                        m.remove(name);
                        -1.0
                    }
                    Some(m) => match m.get_mut(name) {
                        Some(l) => {
                            if let Some(at) = usize::try_from(i).ok().filter(|&at| at < l.len()) {
                                l.remove(at);
                            }
                            l.len() as f64
                        }
                        None => 0.0,
                    },
                    None if i == -1 => -1.0,
                    None => 0.0,
                };
                prune(s, o, name);
                XVal::Num(left)
            }
            "getasarray" => {
                let o = self.jowner(n, c, 1)?;
                let l = stores(self.store()?, name).aux.get(&o).and_then(|m| m.get(name)).cloned().unwrap_or_default();
                self.jlist(l)?
            }
            _ => return Ok(None),
        };
        Ok(Some(v))
    }

    /// AuxiliaryVariableGetAll class [base]: a string map of name -> list of values.
    /// Classes 0 and 2 hold the saved variables, 1 and 3 the temporary ones [observed:
    /// aux.class.*; private and public are not told apart, jip-variable-stores.md §1].
    fn aux_get_all(&mut self, c: &XCall) -> Result<XVal, VmError> {
        let class = self.jindex(c, 0, 0)?;
        let o = self.jowner("AuxiliaryVariableGetAll", c, 1).ok();
        let x = self.store()?;
        let s = if class % 2 == 1 { &x.jip_temp } else { &x.jip };
        let vars: Vec<(String, Vec<XVal>)> = o.and_then(|o| s.aux.get(&o)).map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect()).unwrap_or_default();
        let mut entries = Vec::new();
        for (k, l) in vars {
            let inner = l.into_iter().enumerate().map(|(i, v)| (XKey::Num(i as f64), v)).collect();
            entries.push((XKey::Str(k), XVal::Array(x.new_array(ArrayKind::List, inner))));
        }
        Ok(XVal::Array(x.new_array(ArrayKind::StringMap, entries)))
    }

    /// The form `hex` (without the mod index) of plugin `name`.
    fn plugin_form(&mut self, name: &str, hex: &str) -> Option<u32> {
        let i = self.host.mod_index(name.trim())?;
        let low = u32::from_str_radix(hex.trim(), 16).ok()? & 0x00FF_FFFF;
        let f = (i << 24) | low;
        self.host.form_sig(f).map(|_| f)
    }
}

/// ValidateRegex's code for a pattern error [documented: ValidateRegex lists the codes;
/// which message is which is our reading, hypothesis]: 5 brackets, 6 parentheses,
/// 7 braces, 3 escape, 4 back-reference, 9 range, 11 repeat, else 14 (unknown).
fn regex_error_code(msg: &str) -> f64 {
    let m = msg.to_ascii_lowercase();
    let has = |w: &[&str]| w.iter().any(|w| m.contains(w));
    if has(&["class", "bracket"]) {
        5.0
    } else if has(&["paren", "group"]) {
        6.0
    } else if has(&["brace", "counted", "repetition quantifier"]) {
        7.0
    } else if has(&["backref"]) {
        4.0
    } else if has(&["escape"]) {
        3.0
    } else if has(&["range"]) {
        9.0
    } else if has(&["repeat", "repetition"]) {
        11.0
    } else {
        14.0
    }
}
