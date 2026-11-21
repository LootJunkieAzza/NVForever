//! The extender lab's runtime-query jobs (fnv.capture.steps/1, as
//! tools/capture/jip_probes.py writes them) and the fnv.ext.probes/1 document their
//! answers make, shared by the headless `fnvtool ext queries` and the running engine's
//! test aid (`nvf_play::query_job`), so both write the same rows for the same answers.
//!
//! A step is one runtime-compiled function script run once on no reference, as
//! nvf_capture's query step [documented: docs/spec-notes/xnvse-capture.md §5]; the original
//! runs one step per game loop [documented: xnvse-capture.md §4].

use crate::game::{Game, RunCtx};
use fnv_script::vm::XVal;
use fnv_sim::FormId;
use serde_json::{json, Map, Value};

pub const STEPS_FORMAT: &str = "fnv.capture.steps/1";
pub const PROBES_FORMAT: &str = "fnv.ext.probes/1";

/// One query step of a job.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryStep {
    /// The whole function script (`Begin Function { } … End`).
    pub script: String,
    /// The row's id.
    pub label: String,
    /// The answer as text whatever its type (the capture's `text` steps).
    pub text: bool,
}

/// The probe set of a query job (its name with `_` as `-`, as jip_probes.py names the
/// golden) and its query steps in order. Only query jobs: a step without a query (a
/// console line, a wait) is refused.
pub fn query_steps(job: &Value) -> Result<(String, Vec<QueryStep>), String> {
    if job["format"] != STEPS_FORMAT {
        return Err(format!("not a {STEPS_FORMAT} job"));
    }
    let set = job["name"].as_str().ok_or("job without a name")?.replace('_', "-");
    let mut out = Vec::new();
    for s in job["steps"].as_array().ok_or("job without steps")? {
        let script = s["query"].as_str().ok_or("a step without a query (only query jobs run here)")?;
        let label = s["label"].as_str().ok_or("a step without a label")?;
        out.push(QueryStep { script: script.to_string(), label: label.to_string(), text: s["text"].as_bool().unwrap_or(false) });
    }
    Ok((set, out))
}

/// Run one query step now (`Game::run_query`, on no reference [observed: GetSelfAlt gives
/// none in the original's capture queries]) and give its answer as the runner's
/// observation fields: `value` (a number, a reference as its FormID text, or text) or
/// `error`.
pub fn run_step(g: &mut Game<'_>, step: &QueryStep) -> Map<String, Value> {
    let mut obs = Map::new();
    match g.run_query(&step.script, RunCtx::default()) {
        Ok(XVal::Str(s)) => obs.insert("value".into(), json!(s)),
        Ok(XVal::Ref(r)) if !step.text => obs.insert("value".into(), json!(FormId(r).to_string())),
        Ok(v) if step.text => obs.insert("value".into(), json!(query_text(&v))),
        Ok(XVal::Num(n)) => obs.insert("value".into(), json!(n)),
        Ok(v) => obs.insert("value".into(), json!(query_text(&v))),
        Err(e) => obs.insert("error".into(), json!(e)),
    };
    obs
}

/// A query's answer as text (a `text` step's, or an array's).
pub fn query_text(v: &XVal) -> String {
    match v {
        XVal::Num(n) => fnv_script::vm::number_text(*n),
        XVal::Str(s) => s.clone(),
        XVal::Ref(r) => FormId(*r).to_string(),
        XVal::Array(id) => format!("array #{id}"),
    }
}

/// fnv.ext.probes/1 from observations (each with `label` and `value` or `error`), as
/// jip_probes.py's collector writes it from the game's capture: numbers as `%.6g`, errors
/// listed apart; `source` names what ran the queries.
pub fn probes_doc<'v>(set: &str, observations: impl IntoIterator<Item = &'v Value>, source: &str) -> Value {
    let mut rows = Vec::new();
    let mut errors = Vec::new();
    for o in observations {
        let Some(id) = o["label"].as_str() else { continue };
        if let Some(e) = o.get("error").or_else(|| o.pointer("/errors/action")) {
            errors.push(json!({"id": id, "error": e.as_str().map(str::to_string).unwrap_or_else(|| e.to_string())}));
            continue;
        }
        let value = match &o["value"] {
            Value::Number(n) => fmt_g6(n.as_f64().unwrap_or(f64::NAN)),
            Value::String(s) => s.clone(),
            Value::Null => String::new(),
            v => v.to_string(),
        };
        rows.push(json!({"id": id, "value": value}));
    }
    json!({
        "format": PROBES_FORMAT,
        "set": set,
        "version": "nvforever",
        "sources": [source],
        "rows": rows,
        "files": [],
        "duplicates": [],
        "errors": errors,
    })
}

/// A number as Python's `"%.6g" % x`: six significant digits, trailing zeros dropped,
/// exponent form below 1e-4 and from 1e6 up.
pub fn fmt_g6(x: f64) -> String {
    if x.is_nan() {
        return "nan".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf".into() } else { "-inf".into() };
    }
    if x == 0.0 {
        return if x.is_sign_negative() { "-0".into() } else { "0".into() };
    }
    let sci = format!("{x:.5e}");
    let (mant, exp) = sci.split_once('e').unwrap_or((&sci, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let trim = |s: &str| -> String {
        if s.contains('.') {
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        } else {
            s.to_string()
        }
    };
    if !(-4..6).contains(&exp) {
        let sign = if exp < 0 { '-' } else { '+' };
        format!("{}e{sign}{:02}", trim(mant), exp.abs())
    } else {
        trim(&format!("{x:.*}", (5 - exp).max(0) as usize))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_print_as_python_g6() {
        for (x, s) in [
            (1.0, "1"),
            (0.5, "0.5"),
            (44.99977, "44.9998"),
            (-98.1, "-98.1"),
            (10000.0, "10000"),
            (0.039_999_999_105_930_33, "0.04"),
            (6.49, "6.49"),
            (1e6, "1e+06"),
            (0.00001234, "1.234e-05"),
            (0.0001, "0.0001"),
            (-0.5294, "-0.5294"),
            (0.0, "0"),
        ] {
            assert_eq!(fmt_g6(x), s, "{x}");
        }
    }

    #[test]
    fn steps_and_document() {
        let job = json!({"format": STEPS_FORMAT, "name": "jip_forms", "steps": [
            {"query": "Begin Function { }\nSetFunctionValue 1\nEnd\n", "label": "a", "field": "value"},
            {"query": "Begin Function { }\nSetFunctionValue (GetModName 0)\nEnd\n", "label": "b", "field": "value", "text": true},
        ]});
        let (set, steps) = query_steps(&job).unwrap();
        assert_eq!(set, "jip-forms");
        assert_eq!(steps.len(), 2);
        assert!(!steps[0].text && steps[1].text);
        assert!(query_steps(&json!({"format": STEPS_FORMAT, "name": "x", "steps": [{"console": "tgm"}]})).is_err());
        assert!(query_steps(&json!({"format": "other", "name": "x", "steps": []})).is_err());

        let obs = [json!({"label": "a", "value": 44.99977}), json!({"label": "b", "value": "FalloutNV"}), json!({"label": "c", "error": "script did not compile"})];
        let doc = probes_doc("jip-forms", &obs, "nvforever --play");
        assert_eq!(doc["format"], PROBES_FORMAT);
        assert_eq!(doc["sources"], json!(["nvforever --play"]));
        assert_eq!(doc["rows"], json!([{"id": "a", "value": "44.9998"}, {"id": "b", "value": "FalloutNV"}]));
        assert_eq!(doc["errors"], json!([{"id": "c", "error": "script did not compile"}]));
    }
}
