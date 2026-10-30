//! xNVSE's embedded form lists (GenericAddForm, GenericDeleteForm, GenericGetForm,
//! GenericReplaceForm) and an actor base's animation list (AddAnimation, DelAnimation,
//! DelAnimations, GetNthAnimation): docs/spec-notes/ext-forms.md §6–§7 [documented: their
//! pages in the wiki export]. Changes are kept for the session [hypothesis: as the
//! extender's other list setters, ListAddForm "changes are not saved"].

use crate::game::Game;
use fnv_script::vm::XVal;
use fnv_sim::FormId;
use std::collections::HashMap;

/// The lists as scripts changed them.
#[derive(Debug, Default)]
pub struct ListState {
    /// By list code and owner: race hairs (0), eyes (1), base spells (6), leveled spells
    /// (7), head parts (10). Packages (5) live in `OtherState::package_lists`, which the AI
    /// reads, and form lists (13) in `Game::list_edits`, which ListAddForm changes.
    forms: HashMap<(i64, FormId), Vec<FormId>>,
    /// An actor base's animation file names (KFFZ), by base form.
    anims: HashMap<FormId, Vec<String>>,
}

/// The list codes [documented: GenericGetForm, "Embedded List Codes"].
const CODES: [i64; 7] = [0, 1, 5, 6, 7, 10, 13];

fn form(args: &[XVal], k: usize) -> Option<FormId> {
    match args.get(k)? {
        XVal::Ref(0) => None,
        XVal::Ref(r) => Some(FormId(*r)),
        XVal::Num(x) if *x >= 1.0 && x.fract() == 0.0 => Some(FormId(*x as u32)),
        _ => None,
    }
}

fn int(args: &[XVal], k: usize) -> Option<i64> {
    match args.get(k)? {
        XVal::Num(x) => Some(*x as i64),
        _ => None,
    }
}

fn hex_ids(v: Option<&serde_json::Value>) -> Vec<FormId> {
    v.and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str()).filter_map(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok()).map(FormId).collect())
        .unwrap_or_default()
}

/// The form whose list a code names: an actor base for the actor codes (a reference
/// stands for its base [hypothesis]), else the form itself.
fn owner(g: &Game, code: i64, target: FormId) -> FormId {
    match code {
        5 | 6 | 7 | 10 => crate::functions::base_of(g, target).unwrap_or(target),
        _ => target,
    }
}

/// A list as the records have it, in the order of its getter: GetRaceHairs / GetRaceEyes
/// (HNAM, ENAM), GetBaseSpellListSpells (SPLO, the template's when it supplies the
/// spells), GetBaseSpellListLevSpells (none read), GetHeadParts (PNAM from the last back,
/// as GetHeadParts gives them [observed: nvse-fifth]) [hypothesis: GenericGetForm's index
/// counts as the getter's does].
fn record_list(g: &Game, code: i64, f: FormId) -> Vec<FormId> {
    match code {
        0 => hex_ids(g.data.record_field(f, "HNAM", "")),
        1 => hex_ids(g.data.record_field(f, "ENAM", "")),
        6 => {
            let def = g.data.actors.get(&f);
            let src = match def.and_then(|d| d.template.filter(|_| d.template_spells)) {
                Some(t) => g.data.actors.get(&t),
                None => def,
            };
            src.map(|d| d.spells.clone()).unwrap_or_default()
        }
        10 => g
            .data
            .record_fields
            .get(&f)
            .into_iter()
            .flatten()
            .filter(|(s, _)| s == b"PNAM")
            .filter_map(|(_, v)| v.as_str().and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok()))
            .map(FormId)
            .rev()
            .collect(),
        _ => Vec::new(),
    }
}

impl Game<'_> {
    /// A script's change of an embedded list (codes 0, 1, 6, 7, 10), for its getter.
    pub(crate) fn changed_list(&self, code: i64, f: FormId) -> Option<&Vec<FormId>> {
        self.lists.forms.get(&(code, f))
    }

    /// An embedded list as it is now; `None` for a code that names no list.
    fn embedded(&mut self, code: i64, f: FormId) -> Option<Vec<FormId>> {
        Some(match code {
            5 => self.base_packages(f),
            13 => self.edited_list(f).clone(),
            c if CODES.contains(&c) => match self.lists.forms.get(&(c, f)) {
                Some(l) => l.clone(),
                None => record_list(self, c, f),
            },
            _ => return None,
        })
    }

    /// An actor base's packages as GetBasePackages reads them: AddPackageAt's list, else
    /// the record's PKID, the template's when it supplies the packages and the record has
    /// none.
    fn base_packages(&self, b: FormId) -> Vec<FormId> {
        if let Some(l) = self.other.package_lists.get(&b) {
            return l.clone();
        }
        let def = self.data.actors.get(&b);
        let own = def.map(|d| d.packages.clone()).unwrap_or_default();
        match def.and_then(|d| d.template.filter(|_| d.template_packages)) {
            Some(t) if own.is_empty() => self.data.actors.get(&t).map(|d| d.packages.clone()).unwrap_or_default(),
            _ => own,
        }
    }

    fn store_embedded(&mut self, code: i64, f: FormId, list: Vec<FormId>) {
        match code {
            5 => {
                self.other.package_lists.insert(f, list);
            }
            13 => *self.edited_list(f) = list,
            c => {
                self.lists.forms.insert((c, f), list);
            }
        }
    }

    /// An actor base's animation list: a script's change, else its KFFZ, the template's
    /// when the template supplies the model and animations [hypothesis: one template
    /// step, as the record's flag reads].
    pub(crate) fn animations(&self, base: FormId) -> Vec<String> {
        if let Some(l) = self.lists.anims.get(&base) {
            return l.clone();
        }
        let def = self.data.actors.get(&base);
        let src = match def.and_then(|d| d.template.filter(|_| d.template_model)) {
            Some(t) => t,
            None => base,
        };
        self.data
            .record_field(src, "KFFZ", "")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str()).filter(|s| !s.is_empty()).map(str::to_string).collect())
            .unwrap_or_default()
    }
}

/// The functions of this file (lower-case catalogue names); `None` for any other.
pub(crate) fn call(g: &mut Game, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let none = XVal::Ref(0);
    let v = match name {
        // `GenericGetForm code target index`: the form at the index, none outside the
        // list or for a code that names none [hypothesis].
        "genericgetform" | "genericdeleteform" => {
            let (Some(code), Some(t)) = (int(args, 0), form(args, 1)) else { return Some(Ok(none)) };
            let f = owner(g, code, t);
            let Some(mut list) = g.embedded(code, f) else { return Some(Ok(none)) };
            let at = int(args, 2).and_then(|i| usize::try_from(i).ok()).filter(|&i| i < list.len());
            match at {
                None => none,
                Some(i) if name == "genericgetform" => XVal::Ref(list[i].0),
                // [documented: GenericDeleteForm "Returns the removed form"].
                Some(i) => {
                    let gone = list.remove(i);
                    g.store_embedded(code, f, list);
                    XVal::Ref(gone.0)
                }
            }
        }
        // `GenericAddForm code target form index`: -2 adds at the end [documented]; an
        // index past the end or another negative one adds at the end too [hypothesis].
        "genericaddform" => {
            let (Some(code), Some(t), Some(add)) = (int(args, 0), form(args, 1), form(args, 2)) else { return Some(Ok(XVal::Num(0.0))) };
            let f = owner(g, code, t);
            if let Some(mut list) = g.embedded(code, f) {
                let at = int(args, 3).and_then(|i| usize::try_from(i).ok()).filter(|&i| i <= list.len()).unwrap_or(list.len());
                list.insert(at, add);
                g.store_embedded(code, f, list);
            }
            XVal::Num(0.0)
        }
        // `GenericReplaceForm code target new index`: the form that was there
        // [documented]; none and no change outside the list [hypothesis].
        "genericreplaceform" => {
            let (Some(code), Some(t), Some(new)) = (int(args, 0), form(args, 1), form(args, 2)) else { return Some(Ok(none)) };
            let f = owner(g, code, t);
            let Some(mut list) = g.embedded(code, f) else { return Some(Ok(none)) };
            match int(args, 3).and_then(|i| usize::try_from(i).ok()).filter(|&i| i < list.len()) {
                Some(i) => {
                    let old = std::mem::replace(&mut list[i], new);
                    g.store_embedded(code, f, list);
                    XVal::Ref(old.0)
                }
                None => none,
            }
        }
        // The animation list [documented: AddAnimation `name [baseActor]`, DelAnimation
        // `name [baseActor]`, DelAnimations `[baseActor]`, GetNthAnimation `index
        // [baseActor]`: "the base actor's or creature's animation list"], of the form
        // given, else the calling reference's base.
        "getnthanimation" | "addanimation" | "delanimation" | "delanimations" => {
            let k = usize::from(name != "delanimations");
            let Some(b) = form(args, k).or(subject).map(|f| crate::functions::base_of(g, f).unwrap_or(f)) else {
                return Some(Ok(if name == "getnthanimation" { XVal::Str(String::new()) } else { XVal::Num(0.0) }));
            };
            let mut list = g.animations(b);
            if name == "getnthanimation" {
                // Outside the list: an empty string [hypothesis].
                let i = int(args, 0).and_then(|i| usize::try_from(i).ok());
                return Some(Ok(XVal::Str(i.and_then(|i| list.get(i).cloned()).unwrap_or_default())));
            }
            let anim = match args.first() {
                Some(XVal::Str(s)) => s.clone(),
                _ if name == "delanimations" => String::new(),
                _ => return Some(Ok(XVal::Num(0.0))),
            };
            match name {
                // At the end, once: a name already there is not added again [hypothesis];
                // names compare without case, as file names do.
                "addanimation" => {
                    if !list.iter().any(|a| a.eq_ignore_ascii_case(&anim)) {
                        list.push(anim);
                    }
                }
                "delanimation" => list.retain(|a| !a.eq_ignore_ascii_case(&anim)),
                _ => list.clear(),
            }
            g.lists.anims.insert(b, list);
            XVal::Num(0.0)
        }
        _ => return None,
    };
    Some(Ok(v))
}
