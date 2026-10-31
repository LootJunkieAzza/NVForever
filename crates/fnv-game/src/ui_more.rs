//! The rest of the tile functions (task T9; docs/design/ui-tile-tree.md §5.2,
//! docs/spec-notes/menu-xml.md §9): JIP's SetUIFloatGradual, AttachUIXML,
//! AttachUIComponent, GetStringUIDimensions; SUP's GetUIFloatGradual… readers,
//! SetUIFloatMultiple, GetUIValueType, GetUIElementType and DumpTileInfo family; CacheUI's
//! AddTraitToTile and SetUIStringReveal; xNVSE's SortUIListBox. CacheUI's cache functions
//! are in [`crate::ui_cache`], SUP's HUD bars in [`crate::ui_hudbar`].
//!
//! [`call`] takes a function by name as `crate::ui_functions::call` does; `crate::nvse::call`
//! asks it right after that one. Behaviour from the GECK wiki export's pages, cited per
//! function; what a page leaves open is tagged.

use crate::game::Game;
use crate::ui_gradients::{mode_built, Gradient, Kind, Owner};
use fnv_script::vm::{ArrayKind, XKey, XVal};
use fnv_sim::FormId;
use fnv_ui::{MemSource, Source, TileId, Ui, Value};

/// A text's measure: `(font, text, wrap width)` to `(width, height, lines)` in interface
/// units, the engine's font metrics (as `crate::ui_hud::write_text_widths` takes them).
pub type Measure = Box<dyn Fn(f64, &str, f64) -> (f64, f64, f64) + Send + Sync>;

/// The measure borrowed, as `string_dimensions` takes it.
pub type MeasureRef<'a> = dyn Fn(f64, &str, f64) -> (f64, f64, f64) + 'a;

/// A component AttachUIXML or AttachUIComponent made: injected into `HUDMainMenu`'s root
/// [documented: AttachUIXML, "Internally, the components are injected into HUDMainMenu
/// menu root"], to follow the named NiNode of the reference's model. Placing it at the
/// node on screen is the engine's (not built: menu-xml.md §9).
#[derive(Clone, Debug, PartialEq)]
pub struct Attachment {
    pub reference: FormId,
    /// The NiNode's name, case-sensitive [documented: AttachUIXML notes].
    pub node: String,
    pub tile: TileId,
}

/// The tile functions' session state beyond the tree (not saved).
#[derive(Default)]
pub struct UiExtra {
    pub cache: crate::ui_cache::Cache,
    pub bars: crate::ui_hudbar::Bars,
    pub attached: Vec<Attachment>,
    /// The engine's text measure for GetStringUIDimensions; `None` headless (the function
    /// then writes nothing).
    pub measure: Option<Measure>,
}

pub(crate) fn text(args: &[XVal], k: usize) -> String {
    match args.get(k) {
        Some(XVal::Str(s)) => s.clone(),
        Some(XVal::Num(n)) => Value::Num(*n).text(),
        _ => String::new(),
    }
}

/// A number argument; `None` when it was not given.
pub(crate) fn opt(args: &[XVal], k: usize) -> Option<f64> {
    match args.get(k)? {
        XVal::Num(n) => Some(*n),
        XVal::Str(s) => Some(s.trim().parse().unwrap_or(0.0)),
        XVal::Ref(r) => Some(f64::from(*r)),
        XVal::Array(_) => Some(0.0),
    }
}

pub(crate) fn num(args: &[XVal], k: usize) -> f64 {
    opt(args, k).unwrap_or(0.0)
}

pub(crate) fn yes(b: bool) -> XVal {
    XVal::Num(if b { 1.0 } else { 0.0 })
}

/// A trait's name as the tile holds it.
pub(crate) fn trait_name(ui: &Ui, tile: TileId, k: usize) -> String {
    ui.tile(tile).traits[k].name.clone()
}

/// The gradual functions' tail `[start [end [duration [mode]]]]` on a trait: none stops
/// the change, a start alone stops it and sets the start, more starts it [documented:
/// SetUIFloatGradual, CacheUIGradual: "call the function again while omitting all but"
/// the path (the index and trait), "Alternatively, do include the 2nd argument"]. A
/// missing duration is 0, the end at once [hypothesis]. A start refused by a lock or for a
/// mode not built gives false.
pub(crate) fn gradual(g: &mut Game, owner: Owner, tile: TileId, name: &str, rest: &[XVal]) -> bool {
    match rest.len() {
        0 => g.ui_gradients.stop(owner, tile, name),
        1 => {
            g.ui_gradients.stop(owner, tile, name);
            g.ui_tree.set_trait(tile, name, Value::Num(num(rest, 0)));
        }
        _ => {
            let mode = num(rest, 3).max(0.0) as u8;
            // JIP's page lists modes 0-3; another reads as 0 there [hypothesis].
            let mode = if owner == Owner::Jip && mode > 3 { 0 } else { mode };
            if !mode_built(mode) || g.ui_gradients.locked(owner, tile, name) {
                return false;
            }
            let kind = Kind::Float { start: Some(num(rest, 0)), end: num(rest, 1), mode };
            let gr = Gradient { owner, tile, name: name.to_string(), kind, duration: num(rest, 2), elapsed: 0.0, delay: 0.0, lock: false, watch: None };
            g.ui_gradients.start(&mut g.ui_tree, gr);
        }
    }
    true
}

/// CacheUIGradualDelay's tail `delay [end [duration [flag [mode [match]]]]]`: after the
/// delay the trait goes from its value then to `end` [documented: CacheUIGradualDelay].
/// Without an end it stops the trait's changes, locked ones too; flag 0 gives way to a
/// running change, 1 replaces it, 2 replaces it and locks until it finishes; `match` names
/// a trait of the tile whose change during the delay ends it [documented: the page and
/// its notes].
pub(crate) fn gradual_delay(g: &mut Game, tile: TileId, name: &str, rest: &[XVal]) -> bool {
    let owner = Owner::CacheUi;
    if rest.len() < 2 {
        g.ui_gradients.stop(owner, tile, name);
        return true;
    }
    let flag = num(rest, 3) as i64;
    let mode = num(rest, 4).max(0.0) as u8;
    if !mode_built(mode) || (flag != 2 && g.ui_gradients.locked(owner, tile, name)) {
        return false;
    }
    if flag == 0 && g.ui_gradients.active(owner, tile, name).is_some() {
        return false;
    }
    let watch = (rest.len() > 5).then(|| {
        let m = text(rest, 5);
        let v = g.ui_tree.tile(tile).get(&m).cloned().unwrap_or_default();
        (m, v)
    });
    let kind = Kind::Float { start: None, end: num(rest, 1), mode };
    let gr = Gradient { owner, tile, name: name.to_string(), kind, duration: num(rest, 2), elapsed: 0.0, delay: num(rest, 0).max(0.0), lock: flag == 2, watch };
    g.ui_gradients.start(&mut g.ui_tree, gr);
    true
}

/// The reveal functions' tail `[text [time [mode [invert]]]]`: none stops, the text alone
/// stops and sets it, more starts it [documented: SetUIStringReveal,
/// CacheUIStringReveal: "To stop a reveal use either args"].
pub(crate) fn reveal(g: &mut Game, tile: TileId, name: &str, rest: &[XVal]) -> bool {
    let owner = Owner::CacheUi;
    match rest.len() {
        0 => g.ui_gradients.stop(owner, tile, name),
        1 => {
            g.ui_gradients.stop(owner, tile, name);
            g.ui_tree.set_trait(tile, name, Value::Str(text(rest, 0)));
        }
        _ => {
            let kind = Kind::Reveal { text: text(rest, 0), mode: (num(rest, 2).max(0.0) as u8).min(3), invert: num(rest, 3) != 0.0 };
            let gr = Gradient { owner, tile, name: name.to_string(), kind, duration: num(rest, 1), elapsed: 0.0, delay: 0.0, lock: false, watch: None };
            g.ui_gradients.start(&mut g.ui_tree, gr);
        }
    }
    true
}

/// SUP's element types: 1 rect, 2 image, 3 text, 4 3D, 5 menu [documented:
/// GetUIElementType, SetUIFloatMultiple]; `nif` is the 3D tile [hypothesis]; `hotrect`,
/// `radial`, `window` are in no list and read 0 [hypothesis].
pub fn element_type(kind: &str) -> i64 {
    match kind {
        "rect" => 1,
        "image" => 2,
        "text" => 3,
        "nif" => 4,
        "menu" => 5,
        _ => 0,
    }
}

/// GetUIValueType: 1 a number, 2 a string, 3 an action (a trait worked out by operators,
/// "not given a static value") [documented: GetUIValueType]. A constant reads as a number
/// [hypothesis: menu-xml.md §2.3, the original holds them as numbers].
pub fn value_type(ui: &Ui, tile: TileId, k: usize) -> i64 {
    let t = &ui.tile(tile).traits[k];
    if t.expr.is_some() {
        3
    } else if matches!(t.value, Value::Str(_)) {
        2
    } else {
        1
    }
}

/// A trait's value as DumpTileInfo prints it: numbers with six decimals [documented:
/// DumpTileInfoToArray's example output, "[ visible ] : 1.000000"], strings as they are.
fn dump_value(v: &Value) -> String {
    match v {
        Value::Str(s) => s.clone(),
        other => format!("{:.6}", other.num()),
    }
}

/// DumpTileInfo's lines for one tile: its name, then every trait [documented:
/// DumpTileInfoToArray's example output, `[ Tilename ] : MCM_Trigger` then `[ trait ] :
/// value`; hypothesis: the console dump prints the same lines].
pub fn dump_lines(ui: &Ui, tile: TileId) -> Vec<String> {
    let t = ui.tile(tile);
    let mut lines = vec![format!("[ Tilename ] : {}", t.name)];
    lines.extend(t.traits.iter().map(|tr| format!("[ {} ] : {}", tr.name, dump_value(&tr.value))));
    lines
}

/// DumpTileInfoALL's lines: every live menu's tiles, each preceded by its path [design:
/// the page says only "all XML elements which are currently loaded"].
pub fn dump_all_lines(ui: &Ui) -> Vec<String> {
    let mut lines = Vec::new();
    let menus: Vec<TileId> = ui.menus().collect();
    for m in menus {
        let mut stack = vec![m];
        while let Some(t) = stack.pop() {
            lines.push(format!("{}:", ui.path_of(t)));
            lines.extend(dump_lines(ui, t));
            stack.extend(ui.tile(t).children.iter().rev().copied());
        }
    }
    lines
}

/// Where a dump goes: the console, or with DumpToFile the log (SUP writes `SUPNVSE.log` in
/// the game's folder [documented: DumpTileInfo]; NVForever never writes there and logs
/// to its own log instead [design: CLAUDE.md]).
pub(crate) fn emit(g: &mut Game, to_file: bool, lines: Vec<String>) {
    if to_file {
        for l in &lines {
            tracing::info!(target: "nvforever::ui", "{l}");
        }
    } else {
        g.console_out.extend(lines);
    }
}

/// The key SortUIListBox sorts an item by.
#[derive(Clone, Debug, PartialEq, PartialOrd)]
enum SortKey {
    Num(f64),
    Text(String),
}

struct KeySpec {
    path: String,
    reverse: bool,
    numeric: bool,
    item_name: bool,
}

/// An item name without its quantity and mod marks: "9mm Pistol (2)" and "9mm Pistol+"
/// both "9mm Pistol" [documented: SortUIListBox, `@`].
pub fn bare_item_name(name: &str) -> String {
    let mut s = name.trim();
    loop {
        let before = s;
        if let Some(open) = s.rfind(" (") {
            let inner = &s[open + 2..];
            if inner.ends_with(')') && inner[..inner.len() - 1].chars().all(|c| c.is_ascii_digit()) && inner.len() > 1 {
                s = s[..open].trim_end();
            }
        }
        s = s.trim_end_matches('+').trim_end();
        if s == before {
            return s.to_string();
        }
    }
}

/// SortUIListBox: `"key[,key…][$destination[$increment]]"`, each key `[-][#][@]path`;
/// `-` reverses it, `#` sorts numerically (else alphabetically), `@` by the item name
/// without quantity and mod marks; the destination trait (default `_y`) of each item is
/// set in the sorted order, each the last one's plus the last one's increment trait
/// (default `height`) [documented: SortUIListBox notes]. [hypothesis] The items are the
/// list's children that have the destination trait (a list's other children, its
/// scroll bar, are not placed by it); the first sorted item takes the smallest
/// destination value the items had, so the list stays where it was; alphabetic keys
/// ignore case; a key an item lacks reads 0 or empty; equal items keep their order; the
/// children are not reordered (the page names only a trait to change). False when the
/// path names no tile.
pub fn sort_list_box(ui: &mut Ui, path: &str, spec: &str) -> bool {
    let Some(list) = ui.find_tile(path) else { return false };
    let mut parts = spec.split('$');
    let keys: Vec<KeySpec> = parts
        .next()
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .map(|k| {
            let mut spec = KeySpec { path: String::new(), reverse: false, numeric: false, item_name: false };
            let mut rest = k;
            while let Some(c) = rest.chars().next().filter(|c| matches!(c, '-' | '#' | '@')) {
                match c {
                    '-' => spec.reverse = true,
                    '#' => spec.numeric = true,
                    _ => spec.item_name = true,
                }
                rest = &rest[1..];
            }
            spec.path = rest.trim().to_string();
            spec
        })
        .collect();
    let dest = parts.next().map(str::trim).filter(|s| !s.is_empty()).unwrap_or("_y").to_string();
    let inc = parts.next().map(str::trim).filter(|s| !s.is_empty()).unwrap_or("height").to_string();
    let items: Vec<TileId> = ui.tile(list).children.iter().copied().filter(|&c| ui.tile(c).live && ui.find_trait_in(c, &dest).is_some()).collect();
    if items.is_empty() {
        return true;
    }
    let read = |ui: &Ui, item: TileId, p: &str| ui.find_trait_in(item, p).map(|(t, k)| ui.tile(t).traits[k].value.clone());
    let key_of = |ui: &Ui, item: TileId| -> Vec<SortKey> {
        keys.iter()
            .map(|k| {
                let v = read(ui, item, &k.path).unwrap_or_default();
                if k.numeric {
                    SortKey::Num(v.num())
                } else if k.item_name {
                    SortKey::Text(bare_item_name(&v.text()).to_lowercase())
                } else {
                    SortKey::Text(v.text().to_lowercase())
                }
            })
            .collect()
    };
    let mut sorted: Vec<(TileId, Vec<SortKey>)> = items.iter().map(|&i| (i, key_of(ui, i))).collect();
    sorted.sort_by(|a, b| {
        for (k, spec) in keys.iter().enumerate() {
            let o = a.1[k].partial_cmp(&b.1[k]).unwrap_or(std::cmp::Ordering::Equal);
            let o = if spec.reverse { o.reverse() } else { o };
            if o != std::cmp::Ordering::Equal {
                return o;
            }
        }
        std::cmp::Ordering::Equal
    });
    let mut at = items.iter().filter_map(|&i| read(ui, i, &dest)).map(|v| v.num()).fold(f64::INFINITY, f64::min);
    if !at.is_finite() {
        at = 0.0;
    }
    for (item, _) in sorted {
        if let Some((t, k)) = ui.find_trait_in(item, &dest) {
            let name = trait_name(ui, t, k);
            ui.set_trait(t, &name, Value::Num(at));
        }
        at += read(ui, item, &inc).map_or(0.0, |v| v.num());
    }
    true
}

/// GetStringUIDimensions' outputs: width, height and lines of `text` in font `font`
/// wrapped at `wrap` [documented: GetStringUIDimensions], by the engine's measure.
pub fn string_dimensions(measure: &MeasureRef<'_>, text: &str, font: f64, wrap: f64) -> (f64, f64, f64) {
    measure(font, text, wrap)
}

/// Whether XML is a single `rect` with everything else in it [documented: AttachUIXML,
/// AttachUIComponent, "must be composed of a single rect parent"].
pub fn single_rect(xml: &str) -> bool {
    let doc = fnv_ui::xml::parse(xml);
    let mut elements = doc.nodes.iter().filter_map(|n| match n {
        fnv_ui::xml::Node::Element(e) => Some(e),
        _ => None,
    });
    matches!((elements.next(), elements.next()), (Some(e), None) if e.is("rect"))
}

/// Reads a menu file as InjectUIXML does: under the game's folder (`data\…`) or `Data`.
fn read_xml(src: &dyn Source, file: &str) -> Option<String> {
    let f = file.replace('/', "\\");
    let f = match f.get(..5) {
        Some(head) if head.eq_ignore_ascii_case("data\\") => &f[5..],
        _ => &f[..],
    };
    src.read(f).map(|b| fnv_ui::xml::decode(&b))
}

/// AttachUIXML / AttachUIComponent: the XML into `HUDMainMenu`'s root, kept as attached to
/// the reference's node; 1 on success [documented: AttachUIXML]. Without a calling
/// reference, the HUD, or a single rect, 0.
fn attach(g: &mut Game, subject: Option<FormId>, node: &str, xml: &str) -> bool {
    const HUD: &str = "HUDMainMenu";
    let Some(reference) = subject else { return false };
    if !single_rect(xml) {
        return false;
    }
    let Some(hud) = g.ui_tree.menu(HUD) else { return false };
    let before: Vec<TileId> = g.ui_tree.tile(hud).children.clone();
    let empty = MemSource::default();
    let src: &dyn Source = match g.ui_files.as_deref() {
        Some(s) => s,
        None => &empty,
    };
    if !g.ui_tree.inject_text(src, HUD, xml) {
        return false;
    }
    let made = g.ui_tree.tile(hud).children.iter().copied().find(|c| !before.contains(c));
    match made {
        Some(tile) => {
            g.ui_extra.attached.push(Attachment { reference, node: node.to_string(), tile });
            true
        }
        None => false,
    }
}

/// The tree was built again (`crate::ui_loading`): the changes, the cache entries and
/// the attachments named the old tiles [documented: CacheUI, "Cache will be flushed on
/// gamereload"; hypothesis for the others]; the HUD bars are made again in the new HUD
/// ([`crate::ui_hudbar::rebuilt`]).
pub fn tree_rebuilt(g: &mut Game) {
    g.ui_gradients.clear();
    g.ui_extra.cache.clear();
    g.ui_extra.attached.clear();
    crate::ui_hudbar::rebuilt(g);
}

/// The functions here that read or change the tree, in lower case (the HUD's kept-back
/// writes are made first, as `crate::ui_functions` does).
const TREE_FUNCTIONS: [&str; 16] = [
    "setuifloatgradual",
    "getuifloatgradualactivefortile",
    "getuifloatgradualactivefortiletrait",
    "getuifloatgradualmode",
    "getuifloatgradualtimer",
    "setuistringreveal",
    "setuifloatmultiple",
    "attachuixml",
    "attachuicomponent",
    "addtraittotile",
    "getuivaluetype",
    "getuielementtype",
    "sortuilistbox",
    "dumptileinfo",
    "dumptileinfoall",
    "dumptileinfotoarray",
];

/// The function `name` (any case) with its arguments; `None` when it is not one of these.
pub fn call(g: &mut Game, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let lname = name.to_ascii_lowercase();
    if TREE_FUNCTIONS.contains(&lname.as_str()) || lname.starts_with("cacheui") || lname.starts_with("hudbar") {
        g.ui_used = true;
        crate::ui_hud::flush(g);
    }
    if let Some(r) = crate::ui_cache::call(g, &lname, args) {
        return Some(r);
    }
    if let Some(r) = crate::ui_hudbar::call(g, &lname, args) {
        return Some(r);
    }
    let path = text(args, 0);
    let zero = XVal::Num(0.0);
    let gradient = |g: &Game, owner: Owner| {
        let (t, k) = g.ui_tree.find_trait(&path)?;
        let name = trait_name(&g.ui_tree, t, k);
        g.ui_gradients.active(owner, t, &name).cloned()
    };
    Some(Ok(match lname.as_str() {
        // [documented: SetUIFloatGradual] `path [start [end [duration [mode]]]]`. No return
        // value is documented: 1 when the trait is there [design].
        "setuifloatgradual" => match g.ui_tree.find_trait(&path) {
            Some((t, k)) => {
                let name = trait_name(&g.ui_tree, t, k);
                yes(gradual(g, Owner::Jip, t, &name, args.get(1..).unwrap_or(&[])))
            }
            None => zero,
        },
        // [documented: GetUIFloatGradualActiveForTile, …ForTileTrait]
        "getuifloatgradualactivefortile" => yes(g.ui_tree.find_tile(&path).is_some_and(|t| g.ui_gradients.on_tile(Owner::Jip, t))),
        "getuifloatgradualactivefortiletrait" => yes(gradient(g, Owner::Jip).is_some()),
        // -1 when the trait is not changing [documented: GetUIFloatGradualMode].
        "getuifloatgradualmode" => XVal::Num(gradient(g, Owner::Jip).map_or(-1.0, |gr| f64::from(gr.mode()))),
        // The time since the change began, or with 1 the time left [documented:
        // GetUIFloatGradualTimer]; a repeating change's time left is the current round's
        // and -1 when nothing changes [hypothesis].
        "getuifloatgradualtimer" => XVal::Num(match gradient(g, Owner::Jip) {
            Some(gr) if num(args, 1) != 0.0 => {
                let d = gr.duration.max(0.0);
                if gr.repeats() && d > 0.0 {
                    d - gr.elapsed % d
                } else {
                    (d - gr.elapsed).max(0.0)
                }
            }
            Some(gr) => gr.elapsed,
            None => -1.0,
        }),
        // [documented: SetUIStringReveal] `path [text [time [mode [invert]]]]`.
        "setuistringreveal" => match g.ui_tree.find_trait(&path) {
            Some((t, k)) => {
                let name = trait_name(&g.ui_tree, t, k);
                yes(reveal(g, t, &name, args.get(1..).unwrap_or(&[])))
            }
            None => zero,
        },
        // `path trait value [type]`: every tile in the one named, of the type when given
        // [documented: SetUIFloatMultiple]. [hypothesis] Only tiles that have the trait
        // (as SetUIFloat makes none), not the named tile itself; the count changed is
        // returned [design: no return value is documented].
        "setuifloatmultiple" => match g.ui_tree.find_tile(&path) {
            Some(root) => {
                let trait_ = text(args, 1);
                let v = num(args, 2);
                let filter = opt(args, 3).map(|t| t as i64).filter(|&t| t != 0);
                let mut stack: Vec<TileId> = g.ui_tree.tile(root).children.clone();
                let mut n = 0;
                while let Some(t) = stack.pop() {
                    stack.extend(g.ui_tree.tile(t).children.iter().copied());
                    let tile = g.ui_tree.tile(t);
                    if filter.is_some_and(|f| f != element_type(&tile.kind)) {
                        continue;
                    }
                    if let Some(k) = tile.trait_index(&trait_) {
                        let name = trait_name(&g.ui_tree, t, k);
                        g.ui_tree.set_trait(t, &name, Value::Num(v));
                        n += 1;
                    }
                }
                XVal::Num(f64::from(n))
            }
            None => zero,
        },
        // `ref.AttachUIXML node file` / `ref.AttachUIComponent node "xml"` (the VM formats
        // the component) [documented: AttachUIXML, AttachUIComponent].
        "attachuixml" => {
            let xml = g.ui_files.as_deref().and_then(|src| read_xml(src, &text(args, 1)));
            yes(xml.is_some_and(|x| attach(g, subject, &path, &x)))
        }
        "attachuicomponent" => {
            let xml = text(args, 1);
            yes(attach(g, subject, &path, &xml))
        }
        // `path trait value`: the trait made on the tile, or set when it is there
        // [documented: AddTraitToTile]. The page allows only traits "which already has a
        // ID assign"; the original's trait ids are not known, so any name is taken
        // [hypothesis]. 1 when the tile is there [design: no return value is documented].
        "addtraittotile" => match g.ui_tree.find_tile(&path) {
            Some(t) => {
                g.ui_tree.set_trait(t, &text(args, 1), Value::Num(num(args, 2)));
                yes(true)
            }
            None => zero,
        },
        // -1 when no value is there [documented: GetUIValueType notes].
        "getuivaluetype" => XVal::Num(g.ui_tree.find_trait(&path).map_or(-1.0, |(t, k)| value_type(&g.ui_tree, t, k) as f64)),
        // No path, or none found, means the tile under the cursor [documented]; that is
        // T8's pointer: -1 here [hypothesis].
        "getuielementtype" => XVal::Num(g.ui_tree.find_tile(&path).map_or(-1.0, |t| element_type(&g.ui_tree.tile(t).kind) as f64)),
        "sortuilistbox" => yes(sort_list_box(&mut g.ui_tree, &path, &text(args, 1))),
        // `text font wrap` and three output variables, which the VM takes off
        // (`fnv_script::vm`'s out_call): the answer is [0, width, height, lines].
        "getstringuidimensions" => match g.ui_extra.measure.as_ref() {
            Some(m) => {
                let (w, h, n) = string_dimensions(m.as_ref(), &text(args, 0), num(args, 1), num(args, 2));
                let entries = [0.0, w, h, n].iter().enumerate().map(|(i, v)| (XKey::Num(i as f64), XVal::Num(*v))).collect();
                XVal::Array(g.ext.new_array(ArrayKind::List, entries))
            }
            None => zero,
        },
        // `[toFile [path]]` [documented: DumpTileInfo]. Without a path the cursor's tile
        // (T8): nothing here.
        "dumptileinfo" => {
            let to_file = num(args, 0) != 0.0;
            if let Some(t) = g.ui_tree.find_tile(&text(args, 1)) {
                let lines = dump_lines(&g.ui_tree, t);
                emit(g, to_file, lines);
            }
            zero
        }
        "dumptileinfoall" => {
            let lines = dump_all_lines(&g.ui_tree);
            emit(g, num(args, 0) != 0.0, lines);
            zero
        }
        // A string map of `Tilename` and every trait [documented: DumpTileInfoToArray];
        // numbers as numbers, strings as strings [hypothesis]; empty when the path names
        // no tile.
        "dumptileinfotoarray" => {
            let mut entries = Vec::new();
            if let Some(t) = g.ui_tree.find_tile(&path) {
                let tile = g.ui_tree.tile(t);
                entries.push((XKey::Str("Tilename".into()), XVal::Str(tile.name.clone())));
                for tr in &tile.traits {
                    let v = match &tr.value {
                        Value::Str(s) => XVal::Str(s.clone()),
                        other => XVal::Num(other.num()),
                    };
                    entries.push((XKey::Str(tr.name.clone()), v));
                }
            }
            XVal::Array(g.ext.new_array(ArrayKind::StringMap, entries))
        }
        _ => return None,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_names_lose_quantity_and_mod_marks() {
        assert_eq!(bare_item_name("9mm Pistol (2)"), "9mm Pistol");
        assert_eq!(bare_item_name("9mm Pistol+"), "9mm Pistol");
        assert_eq!(bare_item_name("Pipe (Long)"), "Pipe (Long)");
    }

    #[test]
    fn a_single_rect() {
        assert!(single_rect("<rect name=\"A\"> <text name=\"B\"/> </rect>"));
        assert!(!single_rect("<rect name=\"A\"/> <rect name=\"B\"/>"));
        assert!(!single_rect("<image name=\"A\"/>"));
    }
}
