//! The tile tree: menus read from their files into tiles with traits, found by path,
//! read and changed by the engine and by scripts, and worked out again when what a
//! trait reads from changes (docs/design/ui-tile-tree.md §2–§4,
//! docs/spec-notes/menu-xml.md).

use crate::expr::{self, Arg, Op, OpKind, Src};
use crate::value::Value;
use crate::xml::{self, Element, Node};
use std::collections::{HashMap, HashSet};

/// The tile kinds, the tags that make a tile [observed: the tags with a `name`
/// attribute in the vanilla files; `window` is in no vanilla file but in older Gamebryo
/// menu files, kept as a tile in case a mod uses it (hypothesis)].
pub const TILE_KINDS: &[&str] = &["menu", "rect", "image", "text", "nif", "radial", "hotrect", "window"];

/// Includes nested deeper than this are not followed [design: a guard against a file
/// including itself].
const MAX_INCLUDE_DEPTH: usize = 16;

/// Passes of [`Ui::update`] before it gives up on traits that keep changing (a loop of
/// traits reading each other) [design].
const MAX_PASSES: usize = 32;

/// Where the menu files come from: the game's archives and loose files under `Data`
/// (the engine passes its VFS), or files in memory for tests.
pub trait Source {
    /// A file by its path under `Data`, e.g. `menus\prefabs\meter.xml`; any case, `\`
    /// or `/`.
    fn read(&self, path: &str) -> Option<Vec<u8>>;
    /// [design] Whether the winning file is loose; sources without provenance return false.
    fn is_loose(&self, _path: &str) -> bool {
        false
    }
    /// A game setting's string, for `&-sName;` (the GMST `sName`).
    fn game_string(&self, _name: &str) -> Option<String> {
        None
    }
}

/// Files in memory, by path in lower case with `\`.
#[derive(Default)]
pub struct MemSource {
    pub files: HashMap<String, Vec<u8>>,
    pub strings: HashMap<String, String>,
    /// [design] Loose-file provenance for synthetic sources, using normalized paths.
    pub loose_files: HashSet<String>,
}

impl MemSource {
    pub fn with(mut self, path: &str, text: &str) -> MemSource {
        self.files.insert(norm(path), text.as_bytes().to_vec());
        self
    }

    /// [design] Add a synthetic loose file.
    pub fn with_loose(mut self, path: &str, text: &str) -> MemSource {
        self.loose_files.insert(norm(path));
        self.files.insert(norm(path), text.as_bytes().to_vec());
        self
    }
}

impl Source for MemSource {
    fn is_loose(&self, path: &str) -> bool {
        self.loose_files.contains(&norm(path))
    }

    fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.files.get(&norm(path)).cloned()
    }

    fn game_string(&self, name: &str) -> Option<String> {
        self.strings.get(name).cloned()
    }
}

fn norm(path: &str) -> String {
    path.replace('/', "\\").to_ascii_lowercase()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TileId(pub u32);

/// [design] Creator of a tile; descendants retain that creator across includes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    /// [design] Path passed to Source, and its loose-file status when loaded.
    File {
        path: String,
        loose: bool,
    },
    Injected,
    Template,
    Code,
}

impl Origin {
    fn file(src: &dyn Source, path: &str) -> Self {
        Self::File { path: path.to_string(), loose: src.is_loose(path) }
    }

    fn mod_added(&self) -> bool {
        matches!(self, Self::Injected | Self::Template | Self::File { loose: true, .. })
    }
}

/// One trait of a tile.
#[derive(Clone, Debug)]
pub struct Trait {
    /// As the file writes it.
    pub name: String,
    pub value: Value,
    /// The operators that work the value out, when it is not a plain value.
    pub expr: Option<Vec<Op>>,
    /// The tree's change count when the value last changed.
    pub changed: u64,
    /// The change count when the expression was last worked out (0: never).
    evaluated: u64,
}

/// A `<template>`: content kept to make tiles from later (`AddTileFromTemplate`, the
/// game's code filling lists).
#[derive(Clone, Debug)]
pub struct Template {
    pub name: String,
    pub body: Vec<Node>,
}

#[derive(Clone, Debug)]
pub struct Tile {
    /// [design] Provenance of this tile, independent of later trait writes.
    pub origin: Origin,
    /// The tag in lower case: one of [`TILE_KINDS`].
    pub kind: String,
    pub name: String,
    pub parent: Option<TileId>,
    pub children: Vec<TileId>,
    pub traits: Vec<Trait>,
    pub templates: Vec<Template>,
    /// False once unloaded; its id is not used again.
    pub live: bool,
}

impl Tile {
    pub fn trait_index(&self, name: &str) -> Option<usize> {
        self.traits.iter().position(|t| t.name.eq_ignore_ascii_case(name))
    }

    /// A trait's value by name (any case).
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.trait_index(name).map(|k| &self.traits[k].value)
    }
}

/// The menus' tile tree.
#[derive(Default)]
pub struct Ui {
    tiles: Vec<Tile>,
    /// The loaded menus (and `globals()`, `screen()`, `strings()`), in load order.
    roots: Vec<TileId>,
    globals: Option<TileId>,
    screen: Option<TileId>,
    strings: Option<TileId>,
    /// Counts value changes: a trait whose sources changed after it was last worked out
    /// is worked out again.
    gen: u64,
    /// What loading mended or left out, one line each.
    pub warnings: Vec<String>,
}

impl Ui {
    /// An empty tree with the three pseudo-tiles `screen()`, `strings()` and (until
    /// [`Ui::load_globals`]) an empty `globals()`.
    pub fn new() -> Ui {
        let mut ui = Ui::default();
        let screen = ui.new_tile("rect", "screen()", None, Origin::Code);
        let strings = ui.new_tile("rect", "strings()", None, Origin::Code);
        let globals = ui.new_tile("rect", "globals()", None, Origin::Code);
        ui.roots.extend([screen, strings, globals]);
        ui.screen = Some(screen);
        ui.strings = Some(strings);
        ui.globals = Some(globals);
        ui
    }

    pub fn tile(&self, id: TileId) -> &Tile {
        &self.tiles[id.0 as usize]
    }

    fn tile_mut(&mut self, id: TileId) -> &mut Tile {
        &mut self.tiles[id.0 as usize]
    }

    pub fn screen(&self) -> TileId {
        self.screen.expect("Ui::new makes screen()")
    }

    pub fn strings(&self) -> TileId {
        self.strings.expect("Ui::new makes strings()")
    }

    pub fn globals(&self) -> TileId {
        self.globals.expect("Ui::new makes globals()")
    }

    /// The live menus' root tiles (not the pseudo-tiles).
    pub fn menus(&self) -> impl Iterator<Item = TileId> + '_ {
        self.roots.iter().copied().filter(|&r| self.tile(r).kind == "menu")
    }

    /// The loaded menu of that name (any case).
    pub fn menu(&self, name: &str) -> Option<TileId> {
        self.menus().find(|&r| self.tile(r).name.eq_ignore_ascii_case(name))
    }

    fn new_tile(&mut self, kind: &str, name: &str, parent: Option<TileId>, origin: Origin) -> TileId {
        let id = TileId(self.tiles.len() as u32);
        self.tiles.push(Tile {
            origin,
            kind: kind.to_ascii_lowercase(),
            name: name.to_string(),
            parent,
            children: Vec::new(),
            traits: Vec::new(),
            templates: Vec::new(),
            live: true,
        });
        if let Some(p) = parent {
            self.tile_mut(p).children.push(id);
        }
        id
    }

    // ---- loading --------------------------------------------------------------------

    /// Reads `menus\globals.xml` into `globals()`.
    pub fn load_globals(&mut self, src: &dyn Source) -> bool {
        let Some(doc) = self.read_doc(src, "menus\\globals.xml") else { return false };
        let g = self.globals();
        let origin = Origin::file(src, "menus\\globals.xml");
        self.tile_mut(g).origin = origin.clone();
        // The file is one `<rect name="Strings">` [observed]; its content is globals()'.
        for n in &doc.nodes {
            if let Node::Element(e) = n {
                if is_tile(e) {
                    self.fill(src, g, &e.children, 0, &origin);
                } else {
                    self.fill(src, g, std::slice::from_ref(n), 0, &origin);
                }
            }
        }
        true
    }

    /// Reads a menu file (`menus\…\x_menu.xml`) and builds its menu; a menu of the same
    /// name already loaded is unloaded first. Returns the menu's tile.
    pub fn load_menu(&mut self, src: &dyn Source, path: &str) -> Result<TileId, String> {
        let doc = self.read_doc(src, path).ok_or_else(|| format!("{path}: not found"))?;
        let menu = doc
            .nodes
            .iter()
            .find_map(|n| match n {
                Node::Element(e) if e.is("menu") => Some(e),
                _ => None,
            })
            .ok_or_else(|| format!("{path}: no <menu> element"))?;
        let name = menu.attr("name").unwrap_or("").to_string();
        if let Some(old) = self.menu(&name) {
            self.unload_tile(old);
        }
        let origin = Origin::file(src, path);
        let id = self.new_tile("menu", &name, None, origin.clone());
        self.roots.push(id);
        self.fill(src, id, &menu.children, 0, &origin);
        Ok(id)
    }

    fn read_doc(&mut self, src: &dyn Source, path: &str) -> Option<xml::Doc> {
        let bytes = src.read(path)?;
        let doc = xml::parse(&xml::decode(&bytes));
        for w in &doc.warnings {
            self.warnings.push(format!("{path}: {w}"));
        }
        Some(doc)
    }

    /// Puts content into a tile: child tiles, templates, traits, and the content of
    /// included files where they are included.
    fn fill(&mut self, src: &dyn Source, tile: TileId, nodes: &[Node], depth: usize, origin: &Origin) {
        for n in nodes {
            let Node::Element(e) = n else { continue };
            if is_tile(e) {
                let child = self.new_tile(&e.name, e.attr("name").unwrap_or(""), Some(tile), origin.clone());
                self.fill(src, child, &e.children, depth, origin);
            } else if e.is("template") {
                let name = e.attr("name").unwrap_or("").to_string();
                let body = self.expand_includes(src, &e.children, depth);
                self.tile_mut(tile).templates.push(Template { name, body });
            } else if e.is("include") {
                let file = e.attr("src").unwrap_or("");
                if depth >= MAX_INCLUDE_DEPTH {
                    self.warnings.push(format!("include {file}: nested too deep; left out"));
                    continue;
                }
                let path = prefab_path(file);
                let included_origin = match origin {
                    Origin::File { .. } => Origin::file(src, &path),
                    other => other.clone(),
                };
                match self.read_doc(src, &path) {
                    Some(doc) => self.fill(src, tile, &doc.nodes, depth + 1, &included_origin),
                    None => self.warnings.push(format!("include {file}: not found")),
                }
            } else {
                self.put_trait(src, tile, e);
            }
        }
    }

    /// A template's content with its includes spliced in, so that tiles made from it
    /// later need no files.
    fn expand_includes(&mut self, src: &dyn Source, nodes: &[Node], depth: usize) -> Vec<Node> {
        let mut out = Vec::new();
        for n in nodes {
            match n {
                Node::Element(e) if e.is("include") && depth < MAX_INCLUDE_DEPTH => {
                    let file = e.attr("src").unwrap_or("");
                    match self.read_doc(src, &prefab_path(file)) {
                        Some(doc) => out.extend(self.expand_includes(src, &doc.nodes, depth + 1)),
                        None => self.warnings.push(format!("include {file}: not found")),
                    }
                }
                Node::Element(e) => {
                    let mut e = e.clone();
                    e.children = self.expand_includes(src, &e.children, depth);
                    out.push(Node::Element(e));
                }
                other => out.push(other.clone()),
            }
        }
        out
    }

    /// A trait element: a later one of the same name replaces the earlier [hypothesis:
    /// how the original treats a trait written twice is not known; includes that set a
    /// trait the including file set too depend on it].
    fn put_trait(&mut self, src: &dyn Source, tile: TileId, e: &Element) {
        let string = |n: &str| src.game_string(n);
        let mut warns = Vec::new();
        let (value, expr) = if expr::has_ops(e) {
            (Value::default(), Some(expr::ops_of(&e.children, &string, &mut |w| warns.push(w))))
        } else {
            (expr::literal(&e.children, &string), None)
        };
        for w in warns {
            self.warnings.push(format!("<{}>: {w}", e.name));
        }
        self.gen += 1;
        let gen = self.gen;
        let t = self.tile_mut(tile);
        let new = Trait { name: e.name.clone(), value, expr, changed: gen, evaluated: 0 };
        match t.trait_index(&e.name) {
            Some(k) => t.traits[k] = new,
            None => t.traits.push(new),
        }
    }

    /// Unloads a tile and everything in it.
    pub fn unload_tile(&mut self, id: TileId) {
        if let Some(p) = self.tile(id).parent {
            self.tile_mut(p).children.retain(|&c| c != id);
        }
        self.roots.retain(|&r| r != id);
        let mut stack = vec![id];
        while let Some(t) = stack.pop() {
            let tile = self.tile_mut(t);
            tile.live = false;
            stack.extend(std::mem::take(&mut tile.children));
        }
        self.gen += 1;
    }

    // ---- paths ----------------------------------------------------------------------

    /// The tile a path names: `Menu/Tile/…/Tile`, with `/` or `\`, names in any case, `*`
    /// for any child and `name:N` (or `*:N`) for the N-th (from 0) of the children that
    /// match [documented: GetUIFloat notes].
    pub fn find_tile(&self, path: &str) -> Option<TileId> {
        let segs = split(path);
        let (first, rest) = segs.split_first()?;
        let root = self.root_named(first)?;
        self.walk(root, rest)
    }

    /// The tile and trait a path names: [`Ui::find_tile`] of all but the last segment,
    /// and the last a trait of it.
    pub fn find_trait(&self, path: &str) -> Option<(TileId, usize)> {
        let segs = split(path);
        let (name, tiles) = segs.split_last()?;
        let (first, rest) = tiles.split_first()?;
        let root = self.root_named(first)?;
        self.walk_to_trait(root, rest, name)
    }

    fn root_named(&self, seg: &str) -> Option<TileId> {
        self.roots.iter().copied().find(|&r| self.tile(r).live && self.tile(r).name.eq_ignore_ascii_case(seg))
    }

    /// The children of `at` a segment matches, in order.
    fn matching(&self, at: TileId, seg: &str) -> Vec<TileId> {
        let (name, index) = match seg.rsplit_once(':') {
            Some((n, k)) if k.trim().parse::<usize>().is_ok() => (n, k.trim().parse::<usize>().ok()),
            _ => (seg, None),
        };
        let all = self.tile(at).children.iter().copied();
        let named: Vec<TileId> = all.filter(|&c| name == "*" || self.tile(c).name.eq_ignore_ascii_case(name)).collect();
        match index {
            Some(k) => named.get(k).copied().into_iter().collect(),
            None => named,
        }
    }

    fn walk(&self, at: TileId, segs: &[&str]) -> Option<TileId> {
        let Some((seg, rest)) = segs.split_first() else { return Some(at) };
        self.matching(at, seg).into_iter().find_map(|c| self.walk(c, rest))
    }

    fn walk_to_trait(&self, at: TileId, segs: &[&str], name: &str) -> Option<(TileId, usize)> {
        match segs.split_first() {
            None => self.tile(at).trait_index(name).map(|k| (at, k)),
            Some((seg, rest)) => self.matching(at, seg).into_iter().find_map(|c| self.walk_to_trait(c, rest, name)),
        }
    }

    /// The path of a tile, from its menu: `HUDMainMenu/HitPoints`.
    pub fn path_of(&self, id: TileId) -> String {
        let mut names = vec![self.tile(id).name.clone()];
        let mut at = self.tile(id).parent;
        while let Some(p) = at {
            names.push(self.tile(p).name.clone());
            at = self.tile(p).parent;
        }
        names.reverse();
        names.join("/")
    }

    /// The menu (or pseudo-tile) a tile is in.
    pub fn root_of(&self, mut id: TileId) -> TileId {
        while let Some(p) = self.tile(id).parent {
            id = p;
        }
        id
    }

    // ---- reading and changing traits (the UI functions' ground) ---------------------

    /// A trait's value by path: `GetUIFloat` / `GetUIString`.
    pub fn get(&self, path: &str) -> Option<&Value> {
        let (t, k) = self.find_trait(path)?;
        Some(&self.tile(t).traits[k].value)
    }

    pub fn get_float(&self, path: &str) -> Option<f64> {
        self.get(path).map(Value::num)
    }

    pub fn get_string(&self, path: &str) -> Option<String> {
        self.get(path).map(Value::text)
    }

    /// Sets a trait by path (`SetUIFloat`, `SetUIString`); false when the path names no
    /// trait [hypothesis: the setters make no trait that is not there, AddTraitToTile
    /// exists for that]. The value holds until what the trait's expression reads changes.
    pub fn set(&mut self, path: &str, v: Value) -> bool {
        let Some((t, k)) = self.find_trait(path) else { return false };
        self.set_at(t, k, v);
        true
    }

    pub fn set_float(&mut self, path: &str, v: f64) -> bool {
        self.set(path, Value::Num(v))
    }

    pub fn set_string(&mut self, path: &str, s: &str) -> bool {
        self.set(path, Value::Str(s.to_string()))
    }

    /// Adds to a trait by path (`ModUIFloat`).
    pub fn mod_float(&mut self, path: &str, by: f64) -> bool {
        let Some((t, k)) = self.find_trait(path) else { return false };
        let v = self.tile(t).traits[k].value.num() + by;
        self.set_at(t, k, Value::Num(v));
        true
    }

    /// Sets a tile's trait by name, making it when it is not there (the engine's own
    /// writes: the game code's `io()` traits, `screen()`'s size).
    pub fn set_trait(&mut self, tile: TileId, name: &str, v: Value) {
        let k = match self.tile(tile).trait_index(name) {
            Some(k) => k,
            None => {
                let t = self.tile_mut(tile);
                t.traits.push(Trait { name: name.to_string(), value: Value::default(), expr: None, changed: 0, evaluated: 0 });
                t.traits.len() - 1
            }
        };
        self.set_at(tile, k, v);
    }

    fn set_at(&mut self, t: TileId, k: usize, v: Value) {
        self.gen += 1;
        let gen = self.gen;
        let tr = &mut self.tile_mut(t).traits[k];
        if v.differs(&tr.value) {
            tr.changed = gen;
        }
        tr.value = v;
        // A trait set from outside keeps its value until a source changes again.
        tr.evaluated = gen;
    }

    /// Whether a path names a live menu, tile or trait (`IsComponentLoaded`).
    pub fn is_loaded(&self, path: &str) -> bool {
        self.find_tile(path).is_some() || self.find_trait(path).is_some()
    }

    // ---- injection --------------------------------------------------------------------

    /// Puts XML text into the tile a path names (`InjectUIComponent`): tiles, templates,
    /// traits and includes, as if the tile's file had them at its end. False when the
    /// path names no tile.
    pub fn inject_text(&mut self, src: &dyn Source, at: &str, text: &str) -> bool {
        let Some(tile) = self.find_tile(at) else { return false };
        let doc = xml::parse(text);
        for w in &doc.warnings {
            self.warnings.push(format!("injected into {at}: {w}"));
        }
        self.fill(src, tile, &doc.nodes, 0, &Origin::Injected);
        true
    }

    /// Puts a file's XML into the tile a path names (`InjectUIXML`); the file's path is
    /// under the game's folder (`data\menus\…`) [documented: InjectUIXML notes] or under
    /// `Data`. False when the path names no tile or the file is not there.
    pub fn inject_file(&mut self, src: &dyn Source, at: &str, file: &str) -> bool {
        let f = file.replace('/', "\\");
        let f = match f.get(..5) {
            Some(head) if head.eq_ignore_ascii_case("data\\") => &f[5..],
            _ => &f[..],
        };
        let Some(bytes) = src.read(f) else { return false };
        self.inject_text(src, at, &xml::decode(&bytes))
    }

    /// Makes a tile from a template under the tile a path names
    /// (`AddTileFromTemplate`): the template is looked for on that tile and then on the
    /// tiles above it [hypothesis], the new tile named `name` when given. Returns it.
    pub fn add_from_template(&mut self, src: &dyn Source, at: &str, template: &str, name: Option<&str>) -> Option<TileId> {
        self.add_template(src, at, template, name, Origin::Template)
    }

    /// [design] Make engine-owned template copies, including every top-level tile and
    /// descendant in the template body. These copies are excluded from mod tile roots.
    pub fn add_code_tile_from_template(&mut self, src: &dyn Source, at: &str, template: &str, name: Option<&str>) -> Option<TileId> {
        self.add_template(src, at, template, name, Origin::Code)
    }

    fn add_template(&mut self, src: &dyn Source, at: &str, template: &str, name: Option<&str>, origin: Origin) -> Option<TileId> {
        let parent = self.find_tile(at)?;
        let mut look = Some(parent);
        let body = loop {
            let t = look?;
            if let Some(tp) = self.tile(t).templates.iter().find(|tp| tp.name.eq_ignore_ascii_case(template)) {
                break tp.body.clone();
            }
            look = self.tile(t).parent;
        };
        let before = self.tile(parent).children.len();
        self.fill(src, parent, &body, 0, &origin);
        let made = *self.tile(parent).children.get(before)?;
        if let Some(n) = name {
            self.tile_mut(made).name = n.to_string();
        }
        Some(made)
    }

    /// [design] Top-most live mod-added roots in file order, including the menu itself
    /// when loaded loose. Loose provenance is captured at load; code tiles are not roots.
    pub fn mod_added_tiles(&self, menu: TileId) -> Vec<TileId> {
        if !self.tile(menu).live || self.tile(menu).kind != "menu" {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut stack = vec![menu];
        while let Some(id) = stack.pop() {
            let tile = self.tile(id);
            if !tile.live {
                continue;
            }
            if tile.origin.mod_added() {
                out.push(id);
            } else {
                stack.extend(tile.children.iter().rev().copied());
            }
        }
        out
    }

    // ---- working traits out -----------------------------------------------------------

    /// Works out every trait with an expression whose sources changed since it was last
    /// worked out (all of them the first time), again and again until nothing changes.
    /// Returns how many trait values changed.
    pub fn update(&mut self) -> usize {
        let mut changed = 0;
        for _ in 0..MAX_PASSES {
            let mut this_pass = 0;
            for id in 0..self.tiles.len() {
                if !self.tiles[id].live {
                    continue;
                }
                for k in 0..self.tiles[id].traits.len() {
                    let tid = TileId(id as u32);
                    let tr = &self.tiles[id].traits[k];
                    let Some(ops) = tr.expr.as_ref() else { continue };
                    let mut deps = 0;
                    let v = self.eval(tid, ops, &mut deps);
                    let tr = &self.tiles[id].traits[k];
                    if tr.evaluated != 0 && deps <= tr.evaluated {
                        continue;
                    }
                    let differs = v.differs(&tr.value);
                    self.gen += 1;
                    let gen = self.gen;
                    let tr = &mut self.tiles[id].traits[k];
                    tr.evaluated = gen;
                    if differs {
                        tr.value = v;
                        tr.changed = gen;
                        this_pass += 1;
                    }
                }
            }
            changed += this_pass;
            if this_pass == 0 {
                return changed;
            }
        }
        self.warnings.push(format!("update: traits still changing after {MAX_PASSES} passes (a loop?)"));
        changed
    }

    /// The tile a source names, seen from `me`.
    pub fn resolve(&self, me: TileId, src: &Src) -> Option<TileId> {
        let parent = |t: TileId| self.tile(t).parent;
        let child_named = |t: TileId, n: &str| self.tile(t).children.iter().copied().find(|&c| self.tile(c).name.eq_ignore_ascii_case(n));
        match src {
            Src::Me => Some(me),
            Src::Parent => parent(me),
            Src::Grandparent => parent(parent(me)?),
            Src::Ancestor(n) => {
                let mut at = parent(me);
                while let Some(t) = at {
                    if self.tile(t).name.eq_ignore_ascii_case(n) {
                        return Some(t);
                    }
                    at = parent(t);
                }
                None
            }
            Src::Child(None) => self.tile(me).children.first().copied(),
            Src::Child(Some(n)) => child_named(me, n),
            Src::Sibling(n) => {
                let p = parent(me)?;
                let kids = &self.tile(p).children;
                match n {
                    Some(n) => kids.iter().copied().find(|&c| c != me && self.tile(c).name.eq_ignore_ascii_case(n)),
                    None => {
                        let at = kids.iter().position(|&c| c == me)?;
                        at.checked_sub(1).map(|k| kids[k])
                    }
                }
            }
            Src::Screen => self.screen,
            Src::Globals => self.globals,
            Src::Strings => self.strings,
            Src::Io => Some(self.root_of(me)),
            Src::Named(n) => {
                let root = self.root_of(me);
                self.find_named(root, n).or_else(|| self.root_named(n))
            }
        }
    }

    /// The first tile of that name under `root`, in file order.
    fn find_named(&self, root: TileId, name: &str) -> Option<TileId> {
        let mut stack = vec![root];
        while let Some(t) = stack.pop() {
            if t != root && self.tile(t).name.eq_ignore_ascii_case(name) {
                return Some(t);
            }
            stack.extend(self.tile(t).children.iter().rev().copied());
        }
        None
    }

    fn read_trait(&self, me: TileId, src: &Src, name: &str, deps: &mut u64) -> Value {
        let Some(t) = self.resolve(me, src) else { return Value::default() };
        match self.tile(t).trait_index(name) {
            Some(k) => {
                let tr = &self.tile(t).traits[k];
                *deps = (*deps).max(tr.changed);
                tr.value.clone()
            }
            // A trait not there reads 0 [hypothesis].
            None => Value::default(),
        }
    }

    /// Works out an operator list on tile `me`; `deps` takes the latest change count of
    /// anything read.
    pub fn eval(&self, me: TileId, ops: &[Op], deps: &mut u64) -> Value {
        let mut acc = Value::default();
        for (k, op) in ops.iter().enumerate() {
            let arg = match &op.arg {
                Arg::None => Value::default(),
                Arg::Const(v) => v.clone(),
                Arg::Seq(inner) => self.eval(me, inner, deps),
                Arg::Ref { src, name } => {
                    // `<copy … trait="_filename_"/>` after a number reads `_filename_<n>`
                    // [hypothesis: the meter and barter files pick a picture so].
                    let name = match (&acc, name.ends_with('_')) {
                        (Value::Num(n), true) if op.kind == OpKind::Copy => format!("{name}{}", *n as i64),
                        _ => name.clone(),
                    };
                    self.read_trait(me, src, &name, deps)
                }
            };
            acc = apply(op.kind, acc, arg, me, k);
        }
        acc
    }
}

fn is_tile(e: &Element) -> bool {
    TILE_KINDS.iter().any(|k| e.is(k))
}

/// An include's file: under `menus\prefabs\` [observed: every vanilla include names a
/// file there; MCM's `MCM\MCM.xml` a file in a folder under it].
fn prefab_path(file: &str) -> String {
    format!("menus\\prefabs\\{}", file.trim().replace('/', "\\"))
}

fn split(path: &str) -> Vec<&str> {
    path.split(['/', '\\']).map(str::trim).filter(|s| !s.is_empty()).collect()
}

fn apply(kind: OpKind, acc: Value, a: Value, me: TileId, k: usize) -> Value {
    let (x, y) = (acc.num(), a.num());
    let num = Value::Num;
    match kind {
        OpKind::Copy | OpKind::Ref => a,
        OpKind::Add => num(x + y),
        OpKind::Sub => num(x - y),
        OpKind::Mul => num(x * y),
        // Dividing by 0 leaves the value [hypothesis].
        OpKind::Div => num(if y == 0.0 { x } else { x / y }),
        OpKind::Mod => num(if y == 0.0 { x } else { x % y }),
        OpKind::Min => num(x.min(y)),
        OpKind::Max => num(x.max(y)),
        // The rounding operators work on the value; what is written in them (always 0
        // or 1 in the vanilla files [observed]) is not used [hypothesis].
        OpKind::Floor => num(x.floor()),
        OpKind::Ceil => num(x.ceil()),
        OpKind::Round => num(x.round()),
        OpKind::Trunc => num(x.trunc()),
        OpKind::Abs => num(x.abs()),
        // A whole number from 0 below the argument, the same for this tile and operator
        // every time [design: when the original draws is not known].
        OpKind::Rand => {
            let n = y.max(1.0) as u64;
            let h = (me.0 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (k as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
            num((h >> 33) as f64 % n as f64)
        }
        OpKind::Eq => Value::from_bool(acc.same(&a)),
        OpKind::Neq => Value::from_bool(!acc.same(&a)),
        OpKind::Lt => Value::from_bool(x < y),
        OpKind::Lte => Value::from_bool(x <= y),
        OpKind::Gt => Value::from_bool(x > y),
        OpKind::Gte => Value::from_bool(x >= y),
        OpKind::And => Value::from_bool(acc.truthy() && a.truthy()),
        OpKind::Or => Value::from_bool(acc.truthy() || a.truthy()),
        OpKind::Not => Value::from_bool(!a.truthy()),
        // A false condition makes what was worked out so far 0, and the operators after
        // it go on from there [hypothesis: fits every vanilla use, menu-xml.md §4.3].
        OpKind::OnlyIf => {
            if a.truthy() {
                acc
            } else {
                Value::default()
            }
        }
        OpKind::OnlyIfNot => {
            if a.truthy() {
                Value::default()
            } else {
                acc
            }
        }
    }
}
