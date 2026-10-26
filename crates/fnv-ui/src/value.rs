//! A trait's value (docs/spec-notes/menu-xml.md §3).

/// What a trait holds. A trait is a number and a string at once in the original
/// [documented: AddTraitToTile, "trait are both types at the same time"]; here it holds
/// one of the three and reads as the other on demand.
#[derive(Clone, Debug)]
pub enum Value {
    Num(f64),
    Str(String),
    /// A constant of the menu files written as an entity (`&center;`, `&xbox;`), kept by
    /// its name in lower case. The game's numbers for them are not known (menu-xml.md
    /// §2.3); two of them compare equal when their names do.
    Sym(String),
}

impl Default for Value {
    fn default() -> Value {
        Value::Num(0.0)
    }
}

/// The numbers of the few constants whose number matters to the files' arithmetic
/// [hypothesis: `&true;` 1 and `&false;` 0, from the meter prefab picking its picture by
/// `_filename_` + `_SolidMeter`; the platform flags 0 on a PC, since the files guard
/// console-only lengths with them. Observer check: menu-xml.md §8].
pub fn sym_number(name: &str) -> Option<f64> {
    Some(match name {
        "true" => 1.0,
        "false" | "xbox" | "console" | "xenon" => 0.0,
        _ => return None,
    })
}

impl Value {
    /// A literal as the files write it: a number when it reads as one, else text
    /// (trimmed).
    pub fn literal(text: &str) -> Value {
        let t = text.trim();
        match t.parse::<f64>() {
            Ok(n) if !t.is_empty() => Value::Num(n),
            _ => Value::Str(t.to_string()),
        }
    }

    pub fn from_bool(b: bool) -> Value {
        Value::Num(if b { 1.0 } else { 0.0 })
    }

    /// The value as a number: text that is no number, and a constant of unknown number,
    /// read 0 [hypothesis].
    pub fn num(&self) -> f64 {
        match self {
            Value::Num(n) => *n,
            Value::Str(s) => s.trim().parse().unwrap_or(0.0),
            Value::Sym(s) => sym_number(s).unwrap_or(0.0),
        }
    }

    /// The value as text: a whole number without a point [hypothesis: how the original
    /// prints a number into a string trait is not captured].
    pub fn text(&self) -> String {
        match self {
            Value::Num(n) if n.fract() == 0.0 && n.abs() < 1e15 => format!("{}", *n as i64),
            Value::Num(n) => format!("{n}"),
            Value::Str(s) => s.clone(),
            Value::Sym(s) => format!("&{s};"),
        }
    }

    /// Whether the value counts as true for `onlyif`, `and`, `or`, `not`: a number not
    /// 0, text that is a number not 0 or any other text but the empty one, `&true;`
    /// [hypothesis].
    pub fn truthy(&self) -> bool {
        match self {
            Value::Num(n) => *n != 0.0,
            Value::Str(s) => match s.trim().parse::<f64>() {
                Ok(n) => n != 0.0,
                Err(_) => !s.trim().is_empty(),
            },
            Value::Sym(s) => sym_number(s).is_some_and(|n| n != 0.0),
        }
    }

    /// `eq`: constants by name, text by text, otherwise by number.
    pub fn same(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Sym(a), Value::Sym(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b || (a.trim().parse::<f64>().ok().is_some() && self.num() == other.num()),
            _ => self.num() == other.num(),
        }
    }

    /// Whether setting `self` over `old` changes anything a reader would see.
    pub(crate) fn differs(&self, old: &Value) -> bool {
        match (self, old) {
            (Value::Num(a), Value::Num(b)) => a != b && !(a.is_nan() && b.is_nan()),
            (Value::Str(a), Value::Str(b)) | (Value::Sym(a), Value::Sym(b)) => a != b,
            _ => true,
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Value) -> bool {
        !self.differs(other)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literals_and_readings() {
        assert_eq!(Value::literal(" 24 "), Value::Num(24.0));
        assert_eq!(Value::literal(" solid.dds "), Value::Str("solid.dds".into()));
        assert_eq!(Value::Num(3.0).text(), "3");
        assert_eq!(Value::Num(0.5).text(), "0.5");
        assert!(Value::Sym("true".into()).truthy());
        assert!(!Value::Sym("false".into()).truthy());
        assert!(!Value::Sym("center".into()).truthy());
        assert!(Value::Sym("center".into()).same(&Value::Sym("center".into())));
        assert!(!Value::Sym("center".into()).same(&Value::Sym("right".into())));
        assert!(Value::Str("2".into()).same(&Value::Num(2.0)));
        assert!(!Value::Str("".into()).truthy());
    }
}
