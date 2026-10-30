//! A trait's value (docs/spec-notes/menu-xml.md §3).

/// What a trait holds. A trait is a number and a string at once in the original
/// [documented: AddTraitToTile, "trait are both types at the same time"]; here it holds
/// one of the three and reads as the other on demand.
#[derive(Clone, Debug)]
pub enum Value {
    Num(f64),
    Str(String),
    /// A constant of the menu files written as an entity (`&center;`, `&xbox;`), kept by
    /// its name in lower case. Only some have known numbers (menu-xml.md §2.3);
    /// two of them compare equal when their names do.
    Sym(String),
}

impl Default for Value {
    fn default() -> Value {
        Value::Num(0.0)
    }
}

/// Menu class IDs only [documented: GECK wiki, Hardcoded UI Shortcuts, revision 39903
/// (2026-04-02; the same rows as revision 30746), "Menu class ID" rows; GetActiveMenuMode: the same codes as MenuMode].
pub const MENU_CLASSES: &[(&str, u16)] = &[
    ("MessageMenu", 1001),
    ("InventoryMenu", 1002),
    ("StatsMenu", 1003),
    ("HUDMainMenu", 1004),
    ("LoadingMenu", 1007),
    ("ContainerMenu", 1008),
    ("DialogMenu", 1009),
    ("SleepWaitMenu", 1012),
    ("StartMenu", 1013),
    ("LockPickMenu", 1014),
    ("QuantityMenu", 1016),
    ("AudioMenu", 1017),
    ("VideoMenu", 1018),
    ("GameplayMenu", 1020),
    ("MapMenu", 1023),
    ("BookMenu", 1026),
    ("LevelUpMenu", 1027),
    ("RepairMenu", 1035),
    ("RaceSexMenu", 1036),
    ("CreditsMenu", 1047),
    ("CharGenMenu", 1048),
    ("TextEditMenu", 1051),
    ("BarterMenu", 1053),
    ("SurgeryMenu", 1054),
    ("HackingMenu", 1055),
    ("VATSMenu", 1056),
    ("ComputersMenu", 1057),
    ("RepairServicesMenu", 1058),
    ("TutorialMenu", 1059),
    ("SPECIALBookMenu", 1060),
    ("ItemModMenu", 1061),
    ("LoveTesterMenu", 1074),
    ("CompanionWheelMenu", 1075),
    ("TraitSelectMenu", 1076),
    ("RecipeMenu", 1077),
    ("SlotMachineMenu", 1080),
    ("BlackJackMenu", 1081),
    ("RouletteMenu", 1082),
    ("CaravanMenu", 1083),
    ("TraitMenu", 1084),
];

pub fn menu_class_name(code: u16) -> Option<&'static str> {
    MENU_CLASSES.iter().find(|(_, n)| *n == code).map(|(name, _)| *name)
}

pub fn menu_class_number(name: &str) -> Option<u16> {
    MENU_CLASSES.iter().find(|(s, _)| s.eq_ignore_ascii_case(name)).map(|(_, n)| *n)
}

/// Menu class IDs use MENU_CLASSES' documented numbers. Arithmetic flags keep their
/// existing numbers
/// [hypothesis: `&true;` 1 and `&false;` 0, from the meter prefab picking its picture by
/// `_filename_` + `_SolidMeter`; the platform flags 0 on a PC, since the files guard
/// console-only lengths with them. Observer check: menu-xml.md §8].
pub fn sym_number(name: &str) -> Option<f64> {
    if let Some(code) = menu_class_number(name) {
        return Some(f64::from(code));
    }
    Some(match name {
        "true" => 1.0,
        // [observed: private job hud_jip file_0032; hud_hardcore2 hc4-hc5]
        "hudmain" => 1.0,
        "hudalt" => 2.0,
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
    fn menu_class_numbers_only() {
        for &(name, code) in MENU_CLASSES {
            let sym = Value::Sym(name.to_ascii_lowercase());
            assert_eq!(sym.num(), f64::from(code));
            assert!(sym.same(&Value::Num(f64::from(code))));
            assert_eq!(menu_class_name(code), Some(name));
        }
        for name in ["center", "nosound", "pipboymenu", "does_not_stack", "unknown"] {
            assert_eq!(sym_number(name), None);
        }
    }

    #[test]
    fn observed_hud_colour_ids() {
        assert_eq!(Value::Sym("hudmain".into()).num(), 1.0);
        assert_eq!(Value::Sym("hudalt".into()).num(), 2.0);
    }

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
