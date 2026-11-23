//! Archive and FOMOD XML install for the Mods page.
//!
//! Packaging follows the Nexus Mods app note: `.zip`, `.rar`, `.7z`, `.tar`, `.gz`,
//! and a `.fomod` that is a zip. One nested archive (an outer archive whose only file
//! is another archive) is opened. `fomod/script.cs` is not compiled or run.
//!
//! The XML installer is one step on screen at a time. A group is
//! `SelectAtLeastOne`, `SelectAtMostOne`, `SelectExactlyOne`, `SelectAll`, or
//! `SelectAny`. An option is `Required`, `Optional`, `Recommended`, `NotUsable`,
//! or `CouldBeUsable`. Flags set by a chosen option decide which later steps show
//! and which conditional files are copied. Where two files land on the same path,
//! the higher `priority` is copied last and is the one kept. Equal priority keeps
//! the later file. The published spec is fuzzy on that tie.

use eframe::egui::{self, Align2, RichText, ScrollArea};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

const LOOSE: &[&str] = &[
    "meshes", "textures", "sound", "music", "video", "menus", "interface", "scripts", "facegen", "shaders", "lodsettings", "trees", "materials", "distantlod", "lsdata", "nvse", "config", "uio", "plugins",
];

#[allow(clippy::large_enum_variant)]
pub enum Staged {
    Tree { name: String, from: PathBuf, temp: Option<PathBuf> },
    Plan(Plan),
    Wizard(Wizard),
    /// The archive has more than one folder that could be the mod. The player picks one.
    ChooseRoot { name: String, roots: Vec<PathBuf>, temp: Option<PathBuf> },
}

pub struct Plan {
    pub name: String,
    pub root: PathBuf,
    pub items: Vec<Item>,
    pub temp: Option<PathBuf>,
}

pub struct Wizard {
    pub title: String,
    pub folder_name: String,
    pub note: String,
    pub error: Option<String>,
    root: PathBuf,
    temp: Option<PathBuf>,
    steps: Vec<Step>,
    required: Vec<Item>,
    patterns: Vec<When>,
    chosen: Vec<Vec<Vec<bool>>>,
    at: usize,
    facts: Facts,
    image: Option<PathBuf>,
}

struct Step {
    name: String,
    visible: Option<Dep>,
    groups: Vec<Group>,
}

struct Group {
    name: String,
    kind: GroupKind,
    plugins: Vec<Plug>,
}

struct Plug {
    name: String,
    description: String,
    image: Option<PathBuf>,
    files: Vec<Item>,
    flags: Vec<(String, String)>,
    typed: TypeSpec,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GroupKind {
    Any,
    All,
    ExactlyOne,
    AtMostOne,
    AtLeastOne,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Required,
    Optional,
    Recommended,
    NotUsable,
    CouldBeUsable,
}

enum TypeSpec {
    Fixed(Kind),
    Depends { default: Kind, patterns: Vec<(Dep, Kind)> },
}

enum Dep {
    Flag { flag: String, value: String },
    File { file: String, state: String },
    /// A game-version check. The launcher does not read the game version, so this passes.
    Game,
    All(Vec<Dep>),
    Any(Vec<Dep>),
}

#[derive(Clone)]
pub struct Item {
    source: String,
    destination: String,
    priority: i32,
    folder: bool,
}

struct When {
    when: Dep,
    files: Vec<Item>,
}

struct Facts {
    active: HashSet<String>,
    present: HashSet<String>,
    paths: HashSet<String>,
}

pub enum Decision {
    Cancel,
    Install,
}

pub fn stage(src: &Path, facts: FactsInput) -> Result<Staged, String> {
    if src.as_os_str().is_empty() || !src.exists() {
        return Err("That path was not found.".into());
    }
    let facts = Facts::from_input(facts);
    if src.is_dir() {
        return stage_tree(src, None, src.file_name().and_then(|n| n.to_str()).unwrap_or("mod"), facts);
    }
    if !is_archive(src) {
        let file_name = src.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if is_game_file(file_name) {
            let temp = make_temp()?;
            if let Err(e) = std::fs::copy(src, temp.join(file_name)) {
                let _ = std::fs::remove_dir_all(&temp);
                return Err(format!("{}: {e}", src.display()));
            }
            let stem = src.file_stem().and_then(|n| n.to_str()).unwrap_or("mod");
            return stage_tree(&temp, Some(temp.clone()), stem, facts);
        }
        return Err("Drop a folder or an archive (.zip, .7z, .rar, .fomod, .tar, .gz).".into());
    }
    let temp = make_temp()?;
    if let Err(e) = extract_archive(src, &temp) {
        let _ = std::fs::remove_dir_all(&temp);
        return Err(e);
    }
    if let Err(e) = unwrap_nested(&temp) {
        let _ = std::fs::remove_dir_all(&temp);
        return Err(e);
    }
    let stem = archive_stem(src);
    stage_tree(&temp, Some(temp.clone()), &stem, facts)
}

pub struct FactsInput {
    pub active: HashSet<String>,
    pub present: HashSet<String>,
    pub paths: HashSet<String>,
}

impl Facts {
    fn from_input(input: FactsInput) -> Facts {
        Facts {
            active: input.active.into_iter().map(|s| s.to_ascii_lowercase()).collect(),
            present: input.present.into_iter().map(|s| s.to_ascii_lowercase()).collect(),
            paths: input.paths.into_iter().map(|s| s.replace('\\', "/").to_ascii_lowercase()).collect(),
        }
    }
}

fn stage_tree(tree: &Path, temp: Option<PathBuf>, fallback: &str, facts: Facts) -> Result<Staged, String> {
    if fomod_script(tree).is_some() && find_config(tree).is_none() {
        discard(&temp);
        return Err("This installer is a C# script (fomod/script.cs). It is not run.".into());
    }
    if let Some(xml) = find_config(tree) {
        let root = xml.parent().and_then(|p| p.parent()).unwrap_or(tree).to_path_buf();
        let text = match read_xml(&xml) {
            Ok(t) => t,
            Err(e) => {
                discard(&temp);
                return Err(e);
            }
        };
        let wizard = match parse_wizard(&text, &root, temp.clone(), fallback, facts) {
            Ok(w) => w,
            Err(e) => {
                discard(&temp);
                return Err(e);
            }
        };
        if !wizard.needs_choice() {
            let name = wizard.folder_name.clone();
            let items = wizard.plan();
            return Ok(Staged::Plan(Plan { name, root, items, temp }));
        }
        return Ok(Staged::Wizard(wizard));
    }
    let from = hoist(tree);
    let name = clean_name(fallback).unwrap_or_else(|_| "mod".into());
    // One nested folder is already unwrapped. Several folders that each look like a mod stay a choice.
    if !has_top_files(&from) {
        let roots: Vec<PathBuf> = subdirs(&from).into_iter().filter(|p| has_game_files(p)).collect();
        if roots.len() > 1 {
            return Ok(Staged::ChooseRoot { name, roots, temp });
        }
    }
    Ok(Staged::Tree { name, from, temp })
}

pub fn place_tree(mods: &Path, name: &str, from: &Path) -> Result<String, String> {
    let name = clean_name(name)?;
    let dest = mods.join(&name);
    if dest.exists() {
        return Err("That folder is already in the mods folder.".into());
    }
    if let Err(e) = copy_tree(from, &dest) {
        let _ = std::fs::remove_dir_all(&dest);
        return Err(e);
    }
    if let Err(e) = surface_hidden_plugins(&dest) {
        let _ = std::fs::remove_dir_all(&dest);
        return Err(e);
    }
    keep_if_game_files(&dest, &name)
}

pub fn place_plan(mods: &Path, plan: &Plan) -> Result<String, String> {
    let name = clean_name(&plan.name)?;
    let dest = mods.join(&name);
    if dest.exists() {
        return Err("That folder is already in the mods folder.".into());
    }
    std::fs::create_dir_all(&dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    if let Err(e) = copy_items(&plan.root, &dest, &plan.items) {
        let _ = std::fs::remove_dir_all(&dest);
        return Err(e);
    }
    if let Err(e) = surface_hidden_plugins(&dest) {
        let _ = std::fs::remove_dir_all(&dest);
        return Err(e);
    }
    keep_if_game_files(&dest, &name)
}

/// Installs over a mod of the same name. The old folder is moved into `recycle` first and put back if the new copy fails.
pub fn replace_tree(mods: &Path, name: &str, from: &Path, recycle: &Path) -> Result<String, String> {
    let name = clean_name(name)?;
    let dest = mods.join(&name);
    let aside = move_aside(&dest, recycle, &name)?;
    match place_tree(mods, &name, from) {
        Ok(done) => Ok(done),
        Err(e) => {
            restore_aside(aside, &dest);
            Err(e)
        }
    }
}

pub fn replace_plan(mods: &Path, plan: &Plan, recycle: &Path) -> Result<String, String> {
    let name = clean_name(&plan.name)?;
    let dest = mods.join(&name);
    let aside = move_aside(&dest, recycle, &name)?;
    let mut placed = plan.clone_for_place();
    placed.name = name;
    match place_plan(mods, &placed) {
        Ok(done) => Ok(done),
        Err(e) => {
            restore_aside(aside, &dest);
            Err(e)
        }
    }
}

fn move_aside(dest: &Path, recycle: &Path, name: &str) -> Result<Option<PathBuf>, String> {
    if !dest.exists() {
        return Ok(None);
    }
    std::fs::create_dir_all(recycle).map_err(|e| format!("{}: {e}", recycle.display()))?;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let mut aside = recycle.join(format!("{name} {stamp}"));
    let mut n = 2u32;
    while aside.exists() {
        aside = recycle.join(format!("{name} {stamp}-{n}"));
        n += 1;
    }
    std::fs::rename(dest, &aside).map_err(|e| format!("{}: {e}", dest.display()))?;
    Ok(Some(aside))
}

fn restore_aside(aside: Option<PathBuf>, dest: &Path) {
    if let Some(aside) = aside {
        let _ = std::fs::rename(&aside, dest);
    }
}

/// Loose-file paths a plan would write, using `/` and no drive.
pub fn plan_loose_files(plan: &Plan) -> Vec<String> {
    let mut out = Vec::new();
    for item in &plan.items {
        let Ok(rel) = rel_path(&item.source) else { continue };
        let src = plan.root.join(&rel);
        if item.folder {
            let mut stack = vec![(src, String::new())];
            while let Some((dir, prefix)) = stack.pop() {
                let Ok(rd) = std::fs::read_dir(&dir) else { continue };
                for entry in rd.flatten() {
                    let path = entry.path();
                    if path.is_symlink() {
                        continue;
                    }
                    let name = entry.file_name().to_string_lossy().to_string();
                    let next = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
                    if path.is_dir() {
                        stack.push((path, next));
                    } else {
                        let key = if item.destination.trim().is_empty() { next } else { format!("{}/{}", item.destination.trim_end_matches(['/', '\\']), next) };
                        out.push(key.replace('\\', "/"));
                    }
                }
            }
        } else {
            let named = src.file_name().unwrap_or_default().to_string_lossy();
            let key = if item.destination.trim().is_empty() { named.to_string() } else { item.destination.replace('\\', "/") };
            out.push(key);
        }
    }
    out.sort();
    out.dedup();
    out
}

pub fn discard(temp: &Option<PathBuf>) {
    if let Some(t) = temp {
        let _ = std::fs::remove_dir_all(t);
    }
}

fn keep_if_game_files(dest: &Path, name: &str) -> Result<String, String> {
    if !has_game_files(dest) {
        let _ = std::fs::remove_dir_all(dest);
        return Err("That archive had no plugin and no game files, so nothing was added. Use Manual to pick the files.".into());
    }
    Ok(name.to_string())
}

impl Plan {
    fn clone_for_place(&self) -> Plan {
        Plan { name: self.name.clone(), root: self.root.clone(), items: self.items.clone(), temp: None }
    }
}

impl Wizard {
    pub fn discard(&self) {
        discard(&self.temp);
    }

    #[allow(dead_code)]
    pub fn install(&mut self, mods: &Path) -> Result<String, String> {
        self.install_over(mods, None)
    }

    /// Same as [`install`], and a folder of this name already in `mods` is moved to `recycle` first.
    pub fn install_over(&mut self, mods: &Path, recycle: Option<&Path>) -> Result<String, String> {
        if let Some(e) = self.validate_all() {
            return Err(e);
        }
        let plan = Plan { name: self.folder_name.clone(), root: self.root.clone(), items: self.plan(), temp: None };
        match recycle {
            Some(recycle) => replace_plan(mods, &plan, recycle),
            None => place_plan(mods, &plan),
        }
    }

    /// The choices, so a later install of this mod can start from them.
    pub fn choices_text(&self) -> String {
        let mut out = format!("folder={}\n", self.folder_name);
        for (s, step) in self.chosen.iter().enumerate() {
            for (g, group) in step.iter().enumerate() {
                for (p, on) in group.iter().enumerate() {
                    if *on {
                        out.push_str(&format!("{s} {g} {p}\n"));
                    }
                }
            }
        }
        out
    }

    pub fn apply_choices(&mut self, text: &str) {
        for step in &mut self.chosen {
            for group in step.iter_mut() {
                for on in group.iter_mut() {
                    *on = false;
                }
            }
        }
        for line in text.lines() {
            let line = line.trim();
            if let Some(name) = line.strip_prefix("folder=") {
                if clean_name(name).is_ok() {
                    self.folder_name = name.to_string();
                }
                continue;
            }
            let mut parts = line.split_whitespace();
            let (Some(s), Some(g), Some(p)) = (parts.next(), parts.next(), parts.next()) else { continue };
            let (Ok(s), Ok(g), Ok(p)) = (s.parse::<usize>(), g.parse::<usize>(), p.parse::<usize>()) else { continue };
            if let Some(flag) = self.chosen.get_mut(s).and_then(|step| step.get_mut(g)).and_then(|group| group.get_mut(p)) {
                *flag = true;
            }
        }
        self.normalize();
    }

    fn needs_choice(&self) -> bool {
        let visible = self.visible_steps();
        let kinds = self.kinds();
        for (s, step) in self.steps.iter().enumerate() {
            if !visible[s] {
                continue;
            }
            for (g, group) in step.groups.iter().enumerate() {
                let usable: Vec<Kind> = group.plugins.iter().enumerate().map(|(p, _)| kinds[s][g][p]).filter(|k| *k != Kind::NotUsable).collect();
                match group.kind {
                    GroupKind::All => {}
                    GroupKind::ExactlyOne | GroupKind::AtLeastOne if usable.len() > 1 => return true,
                    GroupKind::AtMostOne | GroupKind::Any if !usable.is_empty() => return true,
                    _ => {}
                }
            }
        }
        false
    }

    fn plan(&self) -> Vec<Item> {
        let visible = self.visible_steps();
        let kinds = self.kinds();
        let mut items = Vec::new();
        let mut order = 0i32;
        for item in &self.required {
            let mut item = item.clone();
            item.priority = item.priority.saturating_mul(1000).saturating_add(order);
            order += 1;
            items.push(item);
        }
        for (s, step) in self.steps.iter().enumerate() {
            if !visible[s] {
                continue;
            }
            for (g, group) in step.groups.iter().enumerate() {
                for (p, plug) in group.plugins.iter().enumerate() {
                    if !self.chosen[s][g][p] || kinds[s][g][p] == Kind::NotUsable {
                        continue;
                    }
                    for item in &plug.files {
                        let mut item = item.clone();
                        item.priority = item.priority.saturating_mul(1000).saturating_add(order);
                        order += 1;
                        items.push(item);
                    }
                }
            }
        }
        let flags = self.flags();
        for when in &self.patterns {
            if !when.when.holds(&flags, &self.facts) {
                continue;
            }
            for item in &when.files {
                let mut item = item.clone();
                item.priority = item.priority.saturating_mul(1000).saturating_add(order);
                order += 1;
                items.push(item);
            }
        }
        items.sort_by(|a, b| a.priority.cmp(&b.priority).then(std::cmp::Ordering::Equal));
        items
    }

    fn flags(&self) -> HashMap<String, String> {
        let mut flags = HashMap::new();
        let mut visible = vec![true; self.steps.len()];
        for _ in 0..8 {
            flags.clear();
            let kinds = self.kinds_with(&visible);
            for (s, step) in self.steps.iter().enumerate() {
                if !visible[s] {
                    continue;
                }
                for (g, group) in step.groups.iter().enumerate() {
                    for (p, plug) in group.plugins.iter().enumerate() {
                        if self.chosen[s][g][p] && kinds[s][g][p] != Kind::NotUsable {
                            for (name, value) in &plug.flags {
                                flags.insert(name.to_ascii_lowercase(), value.clone());
                            }
                        }
                    }
                }
            }
            let next: Vec<bool> = self.steps.iter().map(|step| step.visible.as_ref().map(|d| d.holds(&flags, &self.facts)).unwrap_or(true)).collect();
            if next == visible {
                break;
            }
            visible = next;
        }
        flags
    }

    fn visible_steps(&self) -> Vec<bool> {
        let flags = self.flags();
        self.steps.iter().map(|step| step.visible.as_ref().map(|d| d.holds(&flags, &self.facts)).unwrap_or(true)).collect()
    }

    fn kinds(&self) -> Vec<Vec<Vec<Kind>>> {
        self.kinds_with(&self.visible_steps())
    }

    fn kinds_with(&self, visible: &[bool]) -> Vec<Vec<Vec<Kind>>> {
        let flags = {
            let mut flags = HashMap::new();
            for (s, step) in self.steps.iter().enumerate() {
                if !visible.get(s).copied().unwrap_or(false) {
                    continue;
                }
                for (g, group) in step.groups.iter().enumerate() {
                    for (p, plug) in group.plugins.iter().enumerate() {
                        if self.chosen[s][g][p] {
                            for (name, value) in &plug.flags {
                                flags.insert(name.to_ascii_lowercase(), value.clone());
                            }
                        }
                    }
                }
            }
            flags
        };
        self.steps.iter().map(|step| step.groups.iter().map(|group| group.plugins.iter().map(|plug| plug.typed.resolve(&flags, &self.facts)).collect()).collect()).collect()
    }

    fn normalize(&mut self) {
        for _ in 0..8 {
            let visible = self.visible_steps();
            let kinds = self.kinds_with(&visible);
            let mut changed = false;
            for (s, step) in self.steps.iter().enumerate() {
                for (g, group) in step.groups.iter().enumerate() {
                    for (p, _) in group.plugins.iter().enumerate() {
                        let kind = kinds[s][g][p];
                        let want = match (group.kind, kind) {
                            (_, Kind::NotUsable) => Some(false),
                            (_, Kind::Required) => Some(true),
                            (GroupKind::All, _) => Some(true),
                            _ => None,
                        };
                        if let Some(on) = want {
                            if self.chosen[s][g][p] != on {
                                self.chosen[s][g][p] = on;
                                changed = true;
                            }
                        }
                    }
                    if matches!(group.kind, GroupKind::ExactlyOne | GroupKind::AtLeastOne) {
                        let usable: Vec<usize> = group.plugins.iter().enumerate().filter(|(p, _)| kinds[s][g][*p] != Kind::NotUsable).map(|(p, _)| p).collect();
                        let picked = usable.iter().filter(|p| self.chosen[s][g][**p]).count();
                        if picked == 0 {
                            if let Some(p) = default_pick(&group.plugins, &kinds[s][g]) {
                                self.chosen[s][g][p] = true;
                                changed = true;
                            }
                        } else if group.kind == GroupKind::ExactlyOne && picked > 1 {
                            let keep = usable.into_iter().find(|p| self.chosen[s][g][*p]);
                            for (p, slot) in self.chosen[s][g].iter_mut().enumerate().take(group.plugins.len()) {
                                let on = Some(p) == keep;
                                if *slot != on {
                                    *slot = on;
                                    changed = true;
                                }
                            }
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
    }

    fn validate_all(&mut self) -> Option<String> {
        self.normalize();
        let visible = self.visible_steps();
        let kinds = self.kinds();
        for (s, step) in self.steps.iter().enumerate() {
            if !visible[s] {
                continue;
            }
            for (g, group) in step.groups.iter().enumerate() {
                if let Some(e) = group_error(group, &self.chosen[s][g], &kinds[s][g]) {
                    return Some(e);
                }
            }
        }
        clean_name(&self.folder_name).err()
    }
}

fn default_pick(plugins: &[Plug], kinds: &[Kind]) -> Option<usize> {
    plugins.iter().enumerate().find(|(p, _)| kinds[*p] == Kind::Recommended).map(|(p, _)| p).or_else(|| plugins.iter().enumerate().find(|(p, _)| kinds[*p] == Kind::Required).map(|(p, _)| p)).or_else(|| plugins.iter().enumerate().find(|(p, _)| kinds[*p] != Kind::NotUsable).map(|(p, _)| p))
}

fn group_error(group: &Group, chosen: &[bool], kinds: &[Kind]) -> Option<String> {
    let n = group.plugins.iter().enumerate().filter(|(p, _)| chosen[*p] && kinds[*p] != Kind::NotUsable).count();
    let name = &group.name;
    match group.kind {
        GroupKind::ExactlyOne if n != 1 => Some(format!("Choose one in {name}.")),
        GroupKind::AtLeastOne if n < 1 => Some(format!("Choose at least one in {name}.")),
        GroupKind::AtMostOne if n > 1 => Some(format!("Choose at most one in {name}.")),
        _ => None,
    }
}

fn kind_label(kind: Kind) -> &'static str {
    match kind {
        Kind::Required => "required",
        Kind::Optional => "optional",
        Kind::Recommended => "recommended",
        Kind::NotUsable => "not usable",
        Kind::CouldBeUsable => "could be usable",
    }
}

fn group_label(kind: GroupKind) -> &'static str {
    match kind {
        GroupKind::Any => "Any number",
        GroupKind::All => "All of these",
        GroupKind::ExactlyOne => "Choose one",
        GroupKind::AtMostOne => "Choose at most one",
        GroupKind::AtLeastOne => "Choose at least one",
    }
}

pub fn draw(ctx: &egui::Context, w: &mut Wizard) -> Option<Decision> {
    w.normalize();
    let visible = w.visible_steps();
    let ids: Vec<usize> = visible.iter().enumerate().filter(|(_, on)| **on).map(|(i, _)| i).collect();
    if w.at >= ids.len() && !ids.is_empty() {
        w.at = ids.len() - 1;
    }
    let kinds = w.kinds();
    let mut decision = None;
    let mut open = true;
    let title = w.title.clone();
    let screen = ctx.viewport_rect().size();
    egui::Window::new(title)
        .collapsible(false)
        .resizable(true)
        .constrain(true)
        .default_width(540.0)
        .max_size(egui::vec2((screen.x - 32.0).max(280.0), (screen.y - 32.0).max(200.0)))
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut open)
        .show(ctx, |ui| {
        if !w.note.is_empty() {
            ui.label(RichText::new(&w.note).small().color(ui.visuals().weak_text_color()));
        }
        if let Some(path) = &w.image {
            ui.add(egui::Image::from_uri(file_uri(path)).max_height(120.0));
        }
        if ids.is_empty() {
            ui.label("Nothing to choose. The files that always install can be copied.");
        } else {
            let s = ids[w.at];
            ui.label(RichText::new(format!("Step {} of {}: {}", w.at + 1, ids.len(), w.steps[s].name)).strong());
            ScrollArea::vertical().max_height(380.0).show(ui, |ui| {
                for (g, group) in w.steps[s].groups.iter().enumerate() {
                    ui.add_space(8.0);
                    ui.label(RichText::new(format!("{}  ({})", group.name, group_label(group.kind))).strong());
                    match group.kind {
                        GroupKind::ExactlyOne | GroupKind::AtMostOne => {
                            if group.kind == GroupKind::AtMostOne && ui.radio(group.plugins.iter().enumerate().all(|(p, _)| !w.chosen[s][g][p] || kinds[s][g][p] == Kind::NotUsable), "None").clicked() {
                                w.chosen[s][g].fill(false);
                            }
                            for (p, plug) in group.plugins.iter().enumerate() {
                                let kind = kinds[s][g][p];
                                let on = w.chosen[s][g][p] && kind != Kind::NotUsable;
                                let text = format!("{} ({})", plug.name, kind_label(kind));
                                if kind == Kind::NotUsable {
                                    ui.add_enabled(false, egui::RadioButton::new(false, text));
                                } else if ui.radio(on, text).clicked() {
                                    for (i, flag) in w.chosen[s][g].iter_mut().enumerate() {
                                        *flag = i == p;
                                    }
                                }
                                option_body(ui, plug);
                            }
                        }
                        _ => {
                            for (p, plug) in group.plugins.iter().enumerate() {
                                let kind = kinds[s][g][p];
                                let locked = kind == Kind::NotUsable || kind == Kind::Required || group.kind == GroupKind::All;
                                let mut on = w.chosen[s][g][p] && kind != Kind::NotUsable;
                                let text = format!("{} ({})", plug.name, kind_label(kind));
                                if ui.add_enabled(!locked, egui::Checkbox::new(&mut on, text)).changed() {
                                    w.chosen[s][g][p] = on;
                                }
                                option_body(ui, plug);
                            }
                        }
                    }
                }
            });
        }
        if let Some(e) = &w.error {
            ui.label(RichText::new(e).color(ui.visuals().error_fg_color));
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label("Folder name");
            ui.add(egui::TextEdit::singleline(&mut w.folder_name).desired_width(220.0));
        });
        ui.horizontal(|ui| {
            if ui.button("Cancel").clicked() {
                decision = Some(Decision::Cancel);
            }
            if w.at > 0 && ui.button("Back").clicked() {
                w.at -= 1;
                w.error = None;
            }
            let last = ids.is_empty() || w.at + 1 >= ids.len();
            if !last && ui.button("Next").clicked() {
                w.normalize();
                let s = ids[w.at];
                let kinds = w.kinds();
                let mut bad = None;
                for (g, group) in w.steps[s].groups.iter().enumerate() {
                    if let Some(e) = group_error(group, &w.chosen[s][g], &kinds[s][g]) {
                        bad = Some(e);
                        break;
                    }
                }
                if let Some(e) = bad {
                    w.error = Some(e);
                } else {
                    w.error = None;
                    w.at += 1;
                }
            }
            if last && ui.button("Install").clicked() {
                if let Some(e) = w.validate_all() {
                    w.error = Some(e);
                } else {
                    w.error = None;
                    decision = Some(Decision::Install);
                }
            }
        });
    });
    if !open {
        return Some(Decision::Cancel);
    }
    decision
}

fn option_body(ui: &mut egui::Ui, plug: &Plug) {
    if !plug.description.is_empty() {
        ui.label(RichText::new(&plug.description).small().color(ui.visuals().weak_text_color()));
    }
    if let Some(path) = &plug.image {
        ui.add(egui::Image::from_uri(file_uri(path)).max_height(100.0));
    }
}

fn file_uri(path: &Path) -> String {
    let s = path.display().to_string().replace('%', "%25").replace(' ', "%20").replace('#', "%23");
    format!("file://{s}")
}

impl TypeSpec {
    fn resolve(&self, flags: &HashMap<String, String>, facts: &Facts) -> Kind {
        match self {
            TypeSpec::Fixed(kind) => *kind,
            TypeSpec::Depends { default, patterns } => {
                for (dep, kind) in patterns {
                    if dep.holds(flags, facts) {
                        return *kind;
                    }
                }
                *default
            }
        }
    }
}

impl Dep {
    fn holds(&self, flags: &HashMap<String, String>, facts: &Facts) -> bool {
        match self {
            Dep::Flag { flag, value } => flags.get(&flag.to_ascii_lowercase()).is_some_and(|v| v.eq_ignore_ascii_case(value)),
            Dep::File { file, state } => file_state(file, state, facts),
            Dep::Game => true,
            Dep::All(parts) => parts.iter().all(|d| d.holds(flags, facts)),
            Dep::Any(parts) => parts.iter().any(|d| d.holds(flags, facts)),
        }
    }
}

fn file_state(file: &str, state: &str, facts: &Facts) -> bool {
    let name = file.rsplit(['/', '\\']).next().unwrap_or(file).to_ascii_lowercase();
    let path = file.replace('\\', "/").trim_matches('/').to_ascii_lowercase();
    let plugin = name.ends_with(".esm") || name.ends_with(".esp");
    let present = if plugin { facts.present.contains(&name) || facts.active.contains(&name) } else { facts.paths.iter().any(|p| p == &path || p.ends_with(&format!("/{path}"))) };
    let active = if plugin { facts.active.contains(&name) } else { present };
    match state.to_ascii_lowercase().as_str() {
        "active" => active,
        "inactive" => present && !active,
        "missing" => !present,
        _ => false,
    }
}

fn parse_wizard(text: &str, root: &Path, temp: Option<PathBuf>, fallback: &str, facts: Facts) -> Result<Wizard, String> {
    let doc = parse_xml(text).map_err(|e| format!("The installer could not be read: {e}"))?;
    let config = find_el(&doc, "config").ok_or("The installer has no config.")?;
    let title = el_text(config, "moduleName").filter(|s| !s.trim().is_empty()).unwrap_or_else(|| fallback.to_string());
    let folder_name = clean_name(&title).unwrap_or_else(|_| clean_name(fallback).unwrap_or_else(|_| "mod".into()));
    let image = el_child(config, "moduleImage").and_then(|el| attr(el, "path")).and_then(|p| resolve_rel(root, p));
    let required = el_child(config, "requiredInstallFiles").map(parse_items).unwrap_or_default();
    let mut steps = Vec::new();
    if let Some(list) = el_child(config, "installSteps") {
        let order = attr(list, "order").unwrap_or("");
        for step_el in children(list, "installStep") {
            let name = attr(step_el, "name").unwrap_or("Options").to_string();
            let visible = el_child(step_el, "visible").map(parse_dep);
            let mut groups = Vec::new();
            if let Some(glist) = el_child(step_el, "optionalFileGroups") {
                let gorder = attr(glist, "order").unwrap_or("");
                for group_el in children(glist, "group") {
                    let gname = attr(group_el, "name").unwrap_or("Options").to_string();
                    let kind = group_kind(attr(group_el, "type").unwrap_or("SelectAny"));
                    let mut plugins = Vec::new();
                    if let Some(plist) = el_child(group_el, "plugins") {
                        let porder = attr(plist, "order").unwrap_or("");
                        for plug_el in children(plist, "plugin") {
                            plugins.push(parse_plug(plug_el, root));
                        }
                        sort_by(&mut plugins, porder, |p| p.name.as_str());
                    }
                    groups.push(Group { name: gname, kind, plugins });
                }
                sort_by(&mut groups, gorder, |g| g.name.as_str());
            }
            steps.push(Step { name, visible, groups });
        }
        sort_by(&mut steps, order, |s| s.name.as_str());
    }
    let mut patterns = Vec::new();
    if let Some(cond) = el_child(config, "conditionalFileInstalls") {
        if let Some(list) = el_child(cond, "patterns") {
            for pattern in children(list, "pattern") {
                let when = el_child(pattern, "dependencies").map(parse_dep).unwrap_or(Dep::Any(Vec::new()));
                let files = el_child(pattern, "files").map(parse_items).unwrap_or_default();
                patterns.push(When { when, files });
            }
        }
    }
    let note = if fomod_script(root).is_some() { "A script.cs was in the archive and was not run.".to_string() } else { String::new() };
    let mut chosen = Vec::new();
    for step in &steps {
        let mut groups = Vec::new();
        for group in &step.groups {
            let row: Vec<bool> = group.plugins.iter().map(|plug| matches!(plug.typed.resolve(&HashMap::new(), &facts), Kind::Required | Kind::Recommended) || group.kind == GroupKind::All).collect();
            groups.push(row);
        }
        chosen.push(groups);
    }
    let mut wizard = Wizard { title, folder_name, note, error: None, root: root.to_path_buf(), temp, steps, required, patterns, chosen, at: 0, facts, image };
    wizard.normalize();
    Ok(wizard)
}

fn parse_plug(el: &Elem, root: &Path) -> Plug {
    let name = attr(el, "name").unwrap_or("Option").to_string();
    let description = el_text(el, "description").unwrap_or_default().trim().to_string();
    let image = el_child(el, "image").and_then(|image| attr(image, "path")).and_then(|p| resolve_rel(root, p));
    let files = el_child(el, "files").map(parse_items).unwrap_or_default();
    let mut flags = Vec::new();
    if let Some(list) = el_child(el, "conditionFlags") {
        for flag in children(list, "flag") {
            if let Some(name) = attr(flag, "name") {
                flags.push((name.to_string(), flag.text.trim().to_string()));
            }
        }
    }
    let typed = el_child(el, "typeDescriptor").map(parse_type).unwrap_or(TypeSpec::Fixed(Kind::Optional));
    Plug { name, description, image, files, flags, typed }
}

fn parse_type(el: &Elem) -> TypeSpec {
    if let Some(fixed) = el_child(el, "type") {
        return TypeSpec::Fixed(kind_named(attr(fixed, "name").unwrap_or("Optional")));
    }
    if let Some(dep) = el_child(el, "dependencyType") {
        let default = el_child(dep, "defaultType").and_then(|d| attr(d, "name")).map(kind_named).unwrap_or(Kind::Optional);
        let mut patterns = Vec::new();
        if let Some(list) = el_child(dep, "patterns") {
            for pattern in children(list, "pattern") {
                let when = el_child(pattern, "dependencies").map(parse_dep).unwrap_or(Dep::Any(Vec::new()));
                let kind = el_child(pattern, "type").and_then(|t| attr(t, "name")).map(kind_named).unwrap_or(default);
                patterns.push((when, kind));
            }
        }
        return TypeSpec::Depends { default, patterns };
    }
    TypeSpec::Fixed(Kind::Optional)
}

fn parse_items(el: &Elem) -> Vec<Item> {
    let mut items = Vec::new();
    for child in &el.children {
        if child.name.eq_ignore_ascii_case("file") || child.name.eq_ignore_ascii_case("folder") {
            let source = attr(child, "source").unwrap_or("").to_string();
            if source.is_empty() {
                continue;
            }
            let destination = attr(child, "destination").unwrap_or("").to_string();
            let priority = attr(child, "priority").and_then(|s| s.parse().ok()).unwrap_or(0);
            items.push(Item { source, destination, priority, folder: child.name.eq_ignore_ascii_case("folder") });
        }
    }
    items
}

fn parse_dep(el: &Elem) -> Dep {
    if el.name.eq_ignore_ascii_case("flagDependency") {
        return Dep::Flag { flag: attr(el, "flag").unwrap_or("").to_string(), value: attr(el, "value").unwrap_or("").to_string() };
    }
    if el.name.eq_ignore_ascii_case("fileDependency") {
        return Dep::File { file: attr(el, "file").unwrap_or("").to_string(), state: attr(el, "state").unwrap_or("Active").to_string() };
    }
    if el.name.eq_ignore_ascii_case("gameDependency") {
        return Dep::Game;
    }
    let parts: Vec<Dep> = el.children.iter().filter(|c| is_dep_name(&c.name)).map(parse_dep).collect();
    let or = attr(el, "operator").is_some_and(|s| s.eq_ignore_ascii_case("or"));
    if or { Dep::Any(parts) } else { Dep::All(parts) }
}

fn is_dep_name(name: &str) -> bool {
    matches!(name.to_ascii_lowercase().as_str(), "flagdependency" | "filedependency" | "gamedependency" | "dependencies" | "compositedependency")
}

fn group_kind(name: &str) -> GroupKind {
    match name.to_ascii_lowercase().as_str() {
        "selectall" => GroupKind::All,
        "selectexactlyone" => GroupKind::ExactlyOne,
        "selectatmostone" => GroupKind::AtMostOne,
        "selectatleastone" => GroupKind::AtLeastOne,
        _ => GroupKind::Any,
    }
}

fn kind_named(name: &str) -> Kind {
    match name.to_ascii_lowercase().as_str() {
        "required" => Kind::Required,
        "recommended" => Kind::Recommended,
        "notusable" => Kind::NotUsable,
        "couldbeusable" => Kind::CouldBeUsable,
        _ => Kind::Optional,
    }
}

fn sort_by<T>(items: &mut [T], order: &str, name: impl Fn(&T) -> &str) {
    match order.to_ascii_lowercase().as_str() {
        "ascending" => items.sort_by_key(|a| name(a).to_ascii_lowercase()),
        "descending" => items.sort_by_key(|a| std::cmp::Reverse(name(a).to_ascii_lowercase())),
        _ => {}
    }
}

fn copy_items(root: &Path, dest: &Path, items: &[Item]) -> Result<(), String> {
    let mut items = items.to_vec();
    items.sort_by_key(|item| item.priority);
    for item in items {
        let rel = rel_path(&item.source)?;
        let src = root.join(&rel);
        if !src.exists() {
            return Err(format!("The installer asked for {}, and that is not in the archive.", item.source));
        }
        bounded(root, &src)?;
        if item.folder {
            let into = if item.destination.trim().is_empty() {
                dest.to_path_buf()
            } else {
                let into = place_in(dest, &rel_path(&item.destination)?)?;
                std::fs::create_dir_all(&into).map_err(|e| format!("{}: {e}", into.display()))?;
                into
            };
            copy_children(&src, &into)?;
        } else {
            let named = src.file_name().unwrap_or_default();
            let rel = if item.destination.trim().is_empty() || item.destination.ends_with('/') || item.destination.ends_with('\\') {
                rel_path(&item.destination)?.join(named)
            } else {
                rel_path(&item.destination)?
            };
            let to = place_in(dest, &rel)?;
            std::fs::copy(&src, &to).map_err(|e| format!("{}: {e}", src.display()))?;
        }
    }
    Ok(())
}

fn copy_tree(src: &Path, dest: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    copy_children(src, dest)
}

fn copy_children(src: &Path, dest: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    for entry in std::fs::read_dir(src).map_err(|e| format!("{}: {e}", src.display()))?.flatten() {
        let path = entry.path();
        if path.is_symlink() {
            return Err(format!("{} is a link", path.display()));
        }
        let name = entry.file_name();
        let lower = name.to_string_lossy().to_ascii_lowercase();
        if matches!(lower.as_str(), "fomod" | "__macosx" | "meta") {
            continue;
        }
        let to = dest.join(&name);
        if path.is_dir() {
            copy_children(&path, &to)?;
        } else if path.is_file() {
            std::fs::copy(&path, &to).map_err(|e| format!("{}: {e}", path.display()))?;
        }
    }
    Ok(())
}

fn has_game_files(root: &Path) -> bool {
    fn walk(dir: &Path, depth: u8) -> bool {
        if depth == 0 || dir.is_symlink() {
            return false;
        }
        let Ok(rd) = std::fs::read_dir(dir) else { return false };
        for entry in rd.flatten() {
            let path = entry.path();
            if path.is_symlink() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
            if name.eq_ignore_ascii_case("fomod") || name.eq_ignore_ascii_case("__macosx") {
                continue;
            }
            if path.is_dir() {
                if loose_name(name) || walk(&path, depth - 1) {
                    return true;
                }
            } else if path.is_file() && is_game_file(name) {
                return true;
            }
        }
        false
    }
    walk(root, 6)
}

fn loose_name(name: &str) -> bool {
    let l = name.to_ascii_lowercase();
    LOOSE.iter().any(|n| *n == l)
}

fn is_game_file(name: &str) -> bool {
    let l = name.to_ascii_lowercase();
    l.ends_with(".esm") || l.ends_with(".esp") || l.ends_with(".bsa") || l.ends_with(".ba2")
}

fn hoist(dir: &Path) -> PathBuf {
    let mut cur = dir.to_path_buf();
    for _ in 0..6 {
        if has_top_files(&cur) {
            break;
        }
        let kids = subdirs(&cur);
        if kids.len() == 1 {
            cur = kids[0].clone();
            continue;
        }
        // A readme and a pictures folder beside one installer folder (`optional`,
        // or a single option). The plugins in that one folder are the mod.
        let with_plugins: Vec<PathBuf> = kids.iter().filter(|k| dir_has_plugin(k)).cloned().collect();
        if with_plugins.len() == 1 {
            cur = with_plugins[0].clone();
            continue;
        }
        break;
    }
    let data = ci_child(&cur, "Data");
    if let Some(data) = data {
        if data.is_dir() && has_game_files(&data) && !has_top_files(&cur) {
            return data;
        }
    }
    cur
}

/// A plugin the scanner skips (`optional`, and the same names) is copied to the
/// mod folder itself. Otherwise the folder shows in Mod Order and the plugin list stays empty.
fn surface_hidden_plugins(dest: &Path) -> Result<(), String> {
    if !fnv_install::plugins::plugins_in_tree(dest).is_empty() {
        return Ok(());
    }
    let mut found = Vec::new();
    walk_every_plugin(dest, 8, &mut found);
    for path in found {
        let Some(name) = path.file_name() else { continue };
        let to = dest.join(name);
        if to == path || to.exists() {
            continue;
        }
        std::fs::copy(&path, &to).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}

fn dir_has_plugin(dir: &Path) -> bool {
    let mut found = Vec::new();
    walk_every_plugin(dir, 6, &mut found);
    !found.is_empty()
}

fn walk_every_plugin(dir: &Path, depth: u8, out: &mut Vec<PathBuf>) {
    if depth == 0 || dir.is_symlink() {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_symlink() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
        if name.starts_with('.') || name.eq_ignore_ascii_case("fomod") || name.eq_ignore_ascii_case("__macosx") {
            continue;
        }
        if path.is_dir() {
            walk_every_plugin(&path, depth - 1, out);
        } else if path.is_file() && is_game_file(name) && (name.to_ascii_lowercase().ends_with(".esp") || name.to_ascii_lowercase().ends_with(".esm")) {
            out.push(path);
        }
    }
}

fn has_top_files(dir: &Path) -> bool {
    let Ok(rd) = std::fs::read_dir(dir) else { return false };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_symlink() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
        if name.starts_with('.') || name.eq_ignore_ascii_case("fomod") || name.eq_ignore_ascii_case("__macosx") {
            continue;
        }
        if path.is_file() && is_game_file(name) {
            return true;
        }
        if path.is_dir() && loose_name(name) {
            return true;
        }
    }
    false
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    rd.flatten().map(|e| e.path()).filter(|p| p.is_dir() && !p.is_symlink()).filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| !n.starts_with('.') && !n.eq_ignore_ascii_case("fomod") && !n.eq_ignore_ascii_case("__macosx"))).collect()
}

fn fomod_script(dir: &Path) -> Option<PathBuf> {
    let mut roots = vec![dir.to_path_buf()];
    roots.extend(subdirs(dir));
    for root in roots {
        let Some(fomod) = ci_child(&root, "fomod") else { continue };
        if let Some(script) = ci_child(&fomod, "script.cs") {
            if script.is_file() {
                return Some(script);
            }
        }
    }
    None
}

fn find_config(dir: &Path) -> Option<PathBuf> {
    let mut roots = vec![dir.to_path_buf()];
    roots.extend(subdirs(dir));
    for root in roots {
        if let Some(fomod) = ci_child(&root, "fomod") {
            if let Some(xml) = ci_child(&fomod, "ModuleConfig.xml") {
                if xml.is_file() {
                    return Some(xml);
                }
            }
        }
    }
    None
}

fn ci_child(dir: &Path, name: &str) -> Option<PathBuf> {
    let rd = std::fs::read_dir(dir).ok()?;
    for entry in rd.flatten() {
        if entry.file_name().to_str().is_some_and(|n| n.eq_ignore_ascii_case(name)) {
            return Some(entry.path());
        }
    }
    None
}

fn resolve_rel(root: &Path, raw: &str) -> Option<PathBuf> {
    let path = root.join(rel_path(raw).ok()?);
    path.is_file().then_some(path)
}

/// A path from an archive or an installer, as plain names under the folder it is joined to.
/// Refused: `..`, a leading `/` or `\` (a root), and any `:` (a drive such as `C:`, or a
/// Windows stream name), so joining it can never leave that folder on any system.
fn rel_path(raw: &str) -> Result<PathBuf, String> {
    const LEAVES: &str = "The installer path would leave the archive.";
    if raw.starts_with(['/', '\\']) || raw.contains(':') {
        return Err(LEAVES.into());
    }
    let mut path = PathBuf::new();
    for part in raw.split(['/', '\\']) {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            return Err(LEAVES.into());
        }
        path.push(part);
    }
    if !path.components().all(|c| matches!(c, std::path::Component::Normal(_))) {
        return Err(LEAVES.into());
    }
    Ok(path)
}

/// `dest` joined with `rel` (from [`rel_path`]), its folders made, and checked to be inside
/// `dest` once the folders exist (a link already in `dest` cannot lead it out).
fn place_in(dest: &Path, rel: &Path) -> Result<PathBuf, String> {
    const LEAVES: &str = "The installer path would leave the mod folder.";
    if !rel.components().all(|c| matches!(c, std::path::Component::Normal(_))) {
        return Err(LEAVES.into());
    }
    let to = dest.join(rel);
    let parent = to.parent().unwrap_or(dest);
    std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    let dest_c = dest.canonicalize().map_err(|e| format!("{}: {e}", dest.display()))?;
    let parent_c = parent.canonicalize().map_err(|e| format!("{}: {e}", parent.display()))?;
    if !parent_c.starts_with(&dest_c) {
        return Err(LEAVES.into());
    }
    Ok(to)
}

fn bounded(root: &Path, path: &Path) -> Result<(), String> {
    let root_c = root.canonicalize().map_err(|e| format!("{}: {e}", root.display()))?;
    let path_c = path.canonicalize().map_err(|e| format!("{}: {e}", path.display()))?;
    if path_c != root_c && !path_c.starts_with(&root_c) {
        return Err("The installer path would leave the archive.".into());
    }
    Ok(())
}

pub fn clean_name(name: &str) -> Result<String, String> {
    let last = name.split(['/', '\\']).rev().find(|s| !s.is_empty()).unwrap_or("");
    let cleaned: String = last.chars().filter(|c| !matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*' | '\0')).collect();
    let cleaned = cleaned.trim().to_string();
    if cleaned.is_empty() || cleaned.starts_with('.') || cleaned == ".." {
        return Err("That name cannot be used.".into());
    }
    Ok(cleaned)
}

fn is_archive(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_ascii_lowercase();
    const SUFFIXES: &[&str] = &[
        ".zip", ".7z", ".rar", ".fomod", ".tar", ".gz", ".tgz", ".bz2", ".tbz2", ".xz", ".txz", ".cab", ".tar.gz", ".tar.bz2", ".tar.xz", ".tar.zst", ".taz",
    ];
    if SUFFIXES.iter().any(|s| name.ends_with(s)) {
        return true;
    }
    archive_kind(path).is_some()
}

fn archive_stem(src: &Path) -> String {
    let name = src.file_name().and_then(|n| n.to_str()).unwrap_or("mod");
    let lower = name.to_ascii_lowercase();
    for ext in [".tar.gz", ".tar.bz2", ".tar.xz", ".tar.zst"] {
        if let Some(stripped) = lower.strip_suffix(ext) {
            let stem = &name[..stripped.len()];
            if !stem.is_empty() {
                return stem.to_string();
            }
        }
    }
    src.file_stem().and_then(|n| n.to_str()).filter(|s| !s.is_empty()).unwrap_or("mod").to_string()
}

fn archive_kind(path: &Path) -> Option<&'static str> {
    let mut buf = [0u8; 8];
    let mut f = std::fs::File::open(path).ok()?;
    use std::io::Read;
    let n = f.read(&mut buf).ok()?;
    let b = &buf[..n];
    if b.starts_with(b"PK\x03\x04") || b.starts_with(b"PK\x05\x06") || b.starts_with(b"PK\x07\x08") {
        Some("zip")
    } else if b.starts_with(&[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C]) {
        Some("7z")
    } else if b.starts_with(b"Rar!") {
        Some("rar")
    } else if b.starts_with(&[0x1F, 0x8B]) {
        Some("gz")
    } else if b.starts_with(b"BZh") {
        Some("bz2")
    } else if b.starts_with(&[0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00]) {
        Some("xz")
    } else {
        None
    }
}

fn extract_archive(src: &Path, dest: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    let name = src.file_name().and_then(|n| n.to_str()).unwrap_or("").to_ascii_lowercase();
    let kind = archive_kind(src);
    let zip_like = kind == Some("zip") || name.ends_with(".zip") || name.ends_with(".fomod");
    if zip_like {
        if extract_zip(src, dest).is_ok() {
            return reject_links(dest);
        }
        if extract_external(src, dest).is_ok() {
            return reject_links(dest);
        }
        return extract_zip(src, dest);
    }
    let gzip = kind == Some("gz") || name.ends_with(".tar.gz") || name.ends_with(".tgz") || name.ends_with(".taz");
    if (gzip || name.ends_with(".tar")) && extract_tar(src, dest, gzip).is_ok() {
        return reject_links(dest);
    }
    extract_external(src, dest)?;
    reject_links(dest)
}

fn extract_zip(archive: &Path, dest: &Path) -> Result<(), String> {
    let bad = |e: &dyn std::fmt::Display| format!("{}: {e}", archive.display());
    let f = std::fs::File::open(archive).map_err(|e| bad(&e))?;
    let mut z = zip::ZipArchive::new(f).map_err(|e| bad(&e))?;
    std::fs::create_dir_all(dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    for i in 0..z.len() {
        let mut entry = z.by_index(i).map_err(|e| bad(&e))?;
        let name = entry.name().map(|n| n.into_owned()).unwrap_or_default();
        let Some(rel) = entry.enclosed_name() else {
            return Err(bad(&format!("entry {name} would land outside the folder")));
        };
        if entry.is_symlink() {
            return Err(bad(&format!("entry {name} is a link")));
        }
        let to = place_in(dest, &rel).map_err(|e| bad(&format!("entry {name}: {e}")))?;
        if entry.is_dir() {
            std::fs::create_dir_all(&to).map_err(|e| format!("{}: {e}", to.display()))?;
            continue;
        }
        let mut out = std::fs::File::create(&to).map_err(|e| format!("{}: {e}", to.display()))?;
        std::io::copy(&mut entry, &mut out).map_err(|e| bad(&e))?;
    }
    Ok(())
}

fn extract_tar(src: &Path, dest: &Path, gzip: bool) -> Result<(), String> {
    let bad = |e: &dyn std::fmt::Display| format!("{}: {e}", src.display());
    let f = std::fs::File::open(src).map_err(|e| bad(&e))?;
    if gzip {
        unpack_tar(flate2::read::GzDecoder::new(f), dest, &bad)
    } else {
        unpack_tar(f, dest, &bad)
    }
}

fn unpack_tar<R: std::io::Read>(reader: R, dest: &Path, bad: &impl Fn(&dyn std::fmt::Display) -> String) -> Result<(), String> {
    let mut archive = tar::Archive::new(reader);
    for entry in archive.entries().map_err(|e| bad(&e))? {
        let mut entry = entry.map_err(|e| bad(&e))?;
        let path = entry.path().map_err(|e| bad(&e))?.into_owned();
        let rel = rel_path(&path.to_string_lossy()).map_err(|e| bad(&e))?;
        let kind = entry.header().entry_type();
        if !(kind.is_file() || kind.is_dir()) {
            return Err(bad(&format!("entry {} is not a file or folder", path.display())));
        }
        let to = place_in(dest, &rel).map_err(|e| bad(&e))?;
        entry.unpack(&to).map_err(|e| format!("{}: {e}", to.display()))?;
    }
    Ok(())
}

fn extract_external(src: &Path, dest: &Path) -> Result<(), String> {
    let src_s = src.display().to_string();
    let dest_s = dest.display().to_string();
    let attempts: [(&str, Vec<String>); 5] = [
        ("7z", vec!["x".into(), "-y".into(), format!("-o{dest_s}"), src_s.clone()]),
        ("7zz", vec!["x".into(), "-y".into(), format!("-o{dest_s}"), src_s.clone()]),
        ("bsdtar", vec!["-xf".into(), src_s.clone(), "-C".into(), dest_s.clone()]),
        ("unar", vec!["-f".into(), "-o".into(), dest_s.clone(), src_s.clone()]),
        ("unrar", vec!["x".into(), "-o+".into(), "-y".into(), src_s, format!("{dest_s}/")]),
    ];
    let mut last = String::new();
    let mut missing = true;
    for (bin, args) in &attempts {
        match Command::new(bin).args(args).output() {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(format!("{bin}: {e}")),
            Ok(out) if out.status.success() => return Ok(()),
            Ok(out) => {
                missing = false;
                let err = String::from_utf8_lossy(&out.stderr);
                last = format!("{bin}: {}", err.lines().next().unwrap_or("could not open the archive"));
            }
        }
    }
    if missing {
        Err("Could not open that archive. Install 7z or unar, then try again.".into())
    } else {
        Err(last)
    }
}

fn reject_links(root: &Path) -> Result<(), String> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for entry in rd.flatten() {
            let path = entry.path();
            if path.is_symlink() {
                return Err(format!("{} is a link", path.display()));
            }
            if path.is_dir() {
                stack.push(path);
            }
        }
    }
    Ok(())
}

fn unwrap_nested(dir: &Path) -> Result<(), String> {
    let mut archives = Vec::new();
    let mut others = 0;
    let Ok(rd) = std::fs::read_dir(dir) else { return Ok(()) };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_symlink() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if name == "__macosx" || name.starts_with('.') {
            continue;
        }
        if path.is_file() && is_archive(&path) {
            archives.push(path);
        } else {
            others += 1;
        }
    }
    if archives.len() == 1 && others == 0 {
        let inner = &archives[0];
        let sub = dir.join("_nvf_inner");
        extract_archive(inner, &sub)?;
        std::fs::remove_file(inner).map_err(|e| format!("{}: {e}", inner.display()))?;
        for entry in std::fs::read_dir(&sub).map_err(|e| format!("{}: {e}", sub.display()))?.flatten() {
            let to = dir.join(entry.file_name());
            std::fs::rename(entry.path(), &to).map_err(|e| format!("{}: {e}", to.display()))?;
        }
        let _ = std::fs::remove_dir_all(&sub);
    }
    Ok(())
}

fn make_temp() -> Result<PathBuf, String> {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("nvforever-import-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    Ok(dir)
}

fn read_xml(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let (chunks, _) = bytes[2..].as_chunks::<2>();
        let units: Vec<u16> = chunks.iter().map(|c| u16::from_le_bytes(*c)).collect();
        return Ok(String::from_utf16_lossy(&units));
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let (chunks, _) = bytes[2..].as_chunks::<2>();
        let units: Vec<u16> = chunks.iter().map(|c| u16::from_be_bytes(*c)).collect();
        return Ok(String::from_utf16_lossy(&units));
    }
    Ok(String::from_utf8_lossy(&bytes).trim_start_matches('\u{feff}').to_string())
}

struct Elem {
    name: String,
    attrs: Vec<(String, String)>,
    text: String,
    children: Vec<Elem>,
}

fn parse_xml(text: &str) -> Result<Elem, String> {
    let mut stack = vec![Elem { name: String::new(), attrs: Vec::new(), text: String::new(), children: Vec::new() }];
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'<' {
            let start = i;
            while i < b.len() && b[i] != b'<' {
                i += 1;
            }
            let chunk = decode(&text[start..i]);
            if let Some(top) = stack.last_mut() {
                top.text.push_str(&chunk);
            }
            continue;
        }
        if text[i..].starts_with("<!--") {
            i = text[i + 4..].find("-->").map(|n| i + 4 + n + 3).ok_or("The installer XML is broken.")?;
            continue;
        }
        if text[i..].starts_with("<?") {
            i = text[i + 2..].find("?>").map(|n| i + 2 + n + 2).ok_or("The installer XML is broken.")?;
            continue;
        }
        if text[i..].starts_with("<!") {
            i = text[i..].find('>').map(|n| i + n + 1).ok_or("The installer XML is broken.")?;
            continue;
        }
        let end = text[i..].find('>').map(|n| i + n).ok_or("The installer XML is broken.")?;
        let body = &text[i + 1..end];
        let close = body.starts_with('/');
        let empty = body.ends_with('/');
        let body = body.trim().trim_start_matches('/').trim_end_matches('/').trim();
        if close {
            if stack.len() <= 1 {
                return Err("The installer XML is broken.".into());
            }
            let done = stack.pop().unwrap();
            stack.last_mut().unwrap().children.push(done);
        } else {
            let (name, attrs) = split_tag(body)?;
            let el = Elem { name, attrs, text: String::new(), children: Vec::new() };
            if empty {
                stack.last_mut().unwrap().children.push(el);
            } else {
                stack.push(el);
            }
        }
        i = end + 1;
    }
    if stack.len() != 1 {
        return Err("The installer XML is broken.".into());
    }
    Ok(stack.pop().unwrap())
}

fn split_tag(body: &str) -> Result<(String, Vec<(String, String)>), String> {
    let mut parts = body.split_whitespace();
    let raw = parts.next().unwrap_or("").to_string();
    let name = raw.split(':').next_back().unwrap_or(&raw).to_string();
    let rest = body[raw.len()..].trim();
    let mut attrs = Vec::new();
    let b = rest.as_bytes();
    let mut i = 0;
    while i < b.len() {
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() {
            break;
        }
        let start = i;
        while i < b.len() && b[i] != b'=' && !b[i].is_ascii_whitespace() {
            i += 1;
        }
        let key_raw = &rest[start..i];
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() || b[i] != b'=' {
            return Err("The installer XML is broken.".into());
        }
        i += 1;
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() {
            return Err("The installer XML is broken.".into());
        }
        let quote = b[i];
        if quote != b'"' && quote != b'\'' {
            return Err("The installer XML is broken.".into());
        }
        i += 1;
        let vstart = i;
        while i < b.len() && b[i] != quote {
            i += 1;
        }
        if i >= b.len() {
            return Err("The installer XML is broken.".into());
        }
        let key = key_raw.split(':').next_back().unwrap_or(key_raw).to_string();
        attrs.push((key, decode(&rest[vstart..i])));
        i += 1;
    }
    Ok((name, attrs))
}

fn decode(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at + 1..];
        let Some(end) = rest.find(';') else {
            out.push('&');
            break;
        };
        let token = &rest[..end];
        rest = &rest[end + 1..];
        match token {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            t if t.starts_with("#x") || t.starts_with("#X") => {
                if let Some(c) = u32::from_str_radix(&t[2..], 16).ok().and_then(char::from_u32) {
                    out.push(c);
                }
            }
            t if let Some(num) = t.strip_prefix('#') => {
                if let Some(c) = num.parse::<u32>().ok().and_then(char::from_u32) {
                    out.push(c);
                }
            }
            _ => {}
        }
    }
    out.push_str(rest);
    out
}

fn attr<'a>(el: &'a Elem, name: &str) -> Option<&'a str> {
    el.attrs.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
}

fn children<'a>(el: &'a Elem, name: &str) -> Vec<&'a Elem> {
    el.children.iter().filter(|c| c.name.eq_ignore_ascii_case(name)).collect()
}

fn el_child<'a>(el: &'a Elem, name: &str) -> Option<&'a Elem> {
    el.children.iter().find(|c| c.name.eq_ignore_ascii_case(name))
}

fn el_text(el: &Elem, name: &str) -> Option<String> {
    el_child(el, name).map(|c| c.text.clone())
}

fn find_el<'a>(el: &'a Elem, name: &str) -> Option<&'a Elem> {
    if el.name.eq_ignore_ascii_case(name) {
        return Some(el);
    }
    for child in &el.children {
        if let Some(found) = find_el(child, name) {
            return Some(found);
        }
    }
    None
}

/// One archive entry in the manual installer. `<data>` is the folder that becomes the mod.
#[derive(Clone)]
pub struct ManFile {
    pub name: String,
    pub dir: bool,
    pub checked: bool,
    pub children: Vec<ManFile>,
}

/// The files under `dir`, for the manual installer. Nothing is copied.
pub fn scan_manual(dir: &Path) -> ManFile {
    let mut left = 8000usize;
    ManFile { name: "<data>".into(), dir: true, checked: true, children: read_manual(dir, 8, &mut left) }
}

fn read_manual(dir: &Path, depth: u8, left: &mut usize) -> Vec<ManFile> {
    if depth == 0 || *left == 0 || dir.is_symlink() {
        return Vec::new();
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name().to_string_lossy().to_ascii_lowercase());
    let mut out = Vec::new();
    for entry in entries {
        if *left == 0 {
            break;
        }
        let path = entry.path();
        if path.is_symlink() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || name.eq_ignore_ascii_case("fomod") || name.eq_ignore_ascii_case("__macosx") {
            continue;
        }
        *left -= 1;
        if path.is_dir() {
            let children = read_manual(&path, depth - 1, left);
            out.push(ManFile { name, dir: true, checked: true, children });
        } else if path.is_file() {
            out.push(ManFile { name, dir: false, checked: true, children: Vec::new() });
        }
    }
    out
}

pub fn set_branch(node: &mut ManFile, on: bool) {
    node.checked = on;
    for child in &mut node.children {
        set_branch(child, on);
    }
}

/// True when every ticked entry under `<data>` is a plugin, an archive, or a known folder.
pub fn data_valid(node: &ManFile) -> bool {
    let checked: Vec<&ManFile> = node.children.iter().filter(|c| c.checked).collect();
    !checked.is_empty() && checked.iter().all(|c| manual_ok(c))
}

fn manual_ok(node: &ManFile) -> bool {
    if !node.checked {
        return true;
    }
    if node.dir {
        if loose_name(&node.name) {
            return true;
        }
        let checked: Vec<&ManFile> = node.children.iter().filter(|c| c.checked).collect();
        return !checked.is_empty() && checked.iter().all(|c| manual_ok(c));
    }
    manual_file_ok(&node.name)
}

fn manual_file_ok(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    is_game_file(name) || lower.ends_with(".ini") || lower.ends_with(".xml") || lower.ends_with(".dll") || lower.ends_with(".json")
}

/// Copies ticked files from `data` into `dest`. Unticked files are left out. The game folder is not used.
pub fn copy_checked(data: &Path, node: &ManFile, dest: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    copy_checked_into(data, node, dest)
}

fn copy_checked_into(dir: &Path, node: &ManFile, dest: &Path) -> Result<(), String> {
    for child in &node.children {
        if !child.checked {
            continue;
        }
        if child.name.is_empty() || child.name.contains(['/', '\\', ':']) || child.name == "." || child.name == ".." {
            return Err("That file name cannot be used.".into());
        }
        let from = dir.join(&child.name);
        let to = dest.join(&child.name);
        if from.is_symlink() {
            return Err(format!("{} is a link", from.display()));
        }
        if child.dir {
            std::fs::create_dir_all(&to).map_err(|e| format!("{}: {e}", to.display()))?;
            copy_checked_into(&from, child, &to)?;
        } else if from.is_file() {
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
            }
            std::fs::copy(&from, &to).map_err(|e| format!("{}: {e}", from.display()))?;
        }
    }
    Ok(())
}

/// Extracts an archive, or uses a folder, so Manual can list it. Nothing is installed.
pub fn open_manual(src: &Path) -> Result<(Option<PathBuf>, PathBuf, ManFile), String> {
    if src.as_os_str().is_empty() || !src.exists() {
        return Err("That path was not found.".into());
    }
    if src.is_dir() {
        return Ok((None, src.to_path_buf(), scan_manual(src)));
    }
    let temp = make_temp()?;
    if is_archive(src) {
        if let Err(e) = extract_archive(src, &temp) {
            let _ = std::fs::remove_dir_all(&temp);
            return Err(e);
        }
        if let Err(e) = unwrap_nested(&temp) {
            let _ = std::fs::remove_dir_all(&temp);
            return Err(e);
        }
    } else {
        let file_name = src.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !is_game_file(file_name) && !manual_file_ok(file_name) {
            let _ = std::fs::remove_dir_all(&temp);
            return Err("Drop a folder or an archive (.zip, .7z, .rar, .fomod, .tar, .gz).".into());
        }
        if let Err(e) = std::fs::copy(src, temp.join(file_name)) {
            let _ = std::fs::remove_dir_all(&temp);
            return Err(format!("{}: {e}", src.display()));
        }
    }
    let tree = scan_manual(&temp);
    Ok((Some(temp.clone()), temp, tree))
}

#[cfg(test)]
mod tests {
    #[test]
    fn installer_paths_never_leave_the_folder() {
        use super::rel_path;
        for bad in [r"C:\Windows\x", "C:/Windows/x", r"\Windows\x", "/etc/x", "a/../../x", "..", "file.esp:stream", r"\\?\C:\x"] {
            assert!(rel_path(bad).is_err(), "{bad} should be refused");
        }
        assert_eq!(rel_path(r"textures\armor\a.dds").unwrap(), std::path::Path::new("textures").join("armor").join("a.dds"));
        assert_eq!(rel_path("./Data//b.esp").unwrap(), std::path::Path::new("Data").join("b.esp"));
        assert_eq!(rel_path("").unwrap(), std::path::PathBuf::new());
    }

    #[test]
    fn a_tar_entry_with_a_drive_is_refused() {
        let dir = std::env::temp_dir().join(format!("nvf-fomod-tar-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let dest = dir.join("out");
        std::fs::create_dir_all(&dest).unwrap();
        let mut bytes = Vec::new();
        {
            let mut b = tar::Builder::new(&mut bytes);
            let mut h = tar::Header::new_gnu();
            // The name goes into the header as raw bytes: on Windows the tar crate's
            // `append_data` refuses an absolute path, so the test would build no archive there.
            let name = r"C:\Windows\x".as_bytes();
            h.as_gnu_mut().unwrap().name[..name.len()].copy_from_slice(name);
            h.set_size(1);
            h.set_mode(0o644);
            h.set_entry_type(tar::EntryType::Regular);
            h.set_cksum();
            b.append(&h, &b"x"[..]).unwrap();
            b.finish().unwrap();
        }
        let bad = |e: &dyn std::fmt::Display| e.to_string();
        // Refused by the path check itself (`rel_path`), not by a broken header.
        assert_eq!(super::unpack_tar(&bytes[..], &dest, &bad), Err(super::rel_path(r"C:\Windows\x").unwrap_err()));
        assert_eq!(std::fs::read_dir(&dest).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn a_link_inside_the_folder_cannot_lead_out() {
        let dir = std::env::temp_dir().join(format!("nvf-fomod-link-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let dest = dir.join("mod");
        let outside = dir.join("outside");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, dest.join("link")).unwrap();
        assert!(super::place_in(&dest, std::path::Path::new("link/x.esp")).is_err());
        let ok = super::place_in(&dest, std::path::Path::new("textures/a.dds")).unwrap();
        assert_eq!(ok, dest.join("textures/a.dds"));
        assert!(dest.join("textures").is_dir());
        let _ = std::fs::remove_dir_all(&dir);
    }

    use super::*;
    use std::io::Write;

    #[test]
    fn an_option_folder_is_a_choice() {
        let xml = r#"
        <config>
          <moduleName>Example Mod</moduleName>
          <requiredInstallFiles><file source="readme.txt"/></requiredInstallFiles>
          <installSteps order="Explicit">
            <installStep name="Choose Option">
              <optionalFileGroups>
                <group name="Select an option:" type="SelectExactlyOne">
                  <plugins>
                    <plugin name="Option A">
                      <description>A</description>
                      <files><folder source="option_a"/></files>
                      <conditionFlags><flag name="option_a">selected</flag></conditionFlags>
                      <typeDescriptor><type name="Recommended"/></typeDescriptor>
                    </plugin>
                    <plugin name="Option B">
                      <files><folder source="option_b"/></files>
                      <typeDescriptor><type name="Optional"/></typeDescriptor>
                    </plugin>
                  </plugins>
                </group>
              </optionalFileGroups>
            </installStep>
          </installSteps>
        </config>"#;
        let facts = Facts { active: HashSet::new(), present: HashSet::new(), paths: HashSet::new() };
        let w = parse_wizard(xml, Path::new("."), None, "Example", facts).unwrap();
        assert!(w.needs_choice());
        assert_eq!(w.steps.len(), 1);
        assert!(w.chosen[0][0][0]);
        assert!(!w.chosen[0][0][1]);
        let flags = w.flags();
        assert_eq!(flags.get("option_a").map(String::as_str), Some("selected"));
    }

    #[test]
    fn a_plugin_hidden_beside_a_readme_is_the_mod() {
        let t = std::env::temp_dir().join(format!("nvf-fomod-hoist-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&t);
        let src = t.join("src");
        std::fs::create_dir_all(src.join("optional/meshes")).unwrap();
        std::fs::create_dir_all(src.join("images")).unwrap();
        std::fs::write(src.join("readme.txt"), b"hi").unwrap();
        std::fs::write(src.join("images/preview.png"), b"png").unwrap();
        std::fs::write(src.join("optional/Foo.esp"), b"TES4").unwrap();
        std::fs::write(src.join("optional/meshes/a.nif"), b"nif").unwrap();
        let from = hoist(&src);
        assert!(from.ends_with("optional"), "{}", from.display());
        let mods = t.join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        let name = place_tree(&mods, "Example", &from).unwrap();
        assert_eq!(name, "Example");
        assert!(mods.join("Example/Foo.esp").is_file());
        assert!(mods.join("Example/meshes/a.nif").is_file());
        let listed = fnv_install::plugins::plugins_in_tree(&mods.join("Example"));
        assert_eq!(listed.len(), 1);
        assert!(listed[0].0.eq_ignore_ascii_case("Foo.esp"));
        let _ = std::fs::remove_dir_all(&t);
    }

    #[test]
    fn a_zip_installs_its_plugin_not_an_empty_folder() {
        let t = std::env::temp_dir().join(format!("nvf-fomod-zip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&t);
        std::fs::create_dir_all(&t).unwrap();
        let zip_path = t.join("Sample Mod.zip");
        {
            let mut z = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
            let o = zip::write::SimpleFileOptions::default();
            z.start_file("Sample Mod/readme.txt", o).unwrap();
            z.write_all(b"hi").unwrap();
            z.add_directory("Sample Mod/optional/", o).unwrap();
            z.start_file("Sample Mod/optional/Bar.esp", o).unwrap();
            z.write_all(b"TES4").unwrap();
            z.finish().unwrap();
        }
        let facts = FactsInput { active: HashSet::new(), present: HashSet::new(), paths: HashSet::new() };
        let staged = stage(&zip_path, facts).unwrap();
        let Staged::Tree { name, from, temp } = staged else { panic!("a zip with one plugin is not a choice") };
        let mods = t.join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        let name = place_tree(&mods, &name, &from).unwrap();
        discard(&temp);
        assert!(mods.join(&name).join("Bar.esp").is_file(), "plugin was not copied out of the archive");
        assert!(!fnv_install::plugins::plugins_in_tree(&mods.join(&name)).is_empty());
        let _ = std::fs::remove_dir_all(&t);
    }

    #[test]
    fn a_fomod_choice_copies_only_the_chosen_plugin() {
        let t = std::env::temp_dir().join(format!("nvf-fomod-choice-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&t);
        let root = t.join("root");
        std::fs::create_dir_all(root.join("option_a")).unwrap();
        std::fs::create_dir_all(root.join("option_b")).unwrap();
        std::fs::write(root.join("option_a/A.esp"), b"TES4").unwrap();
        std::fs::write(root.join("option_b/B.esp"), b"TES4").unwrap();
        std::fs::write(root.join("readme.txt"), b"r").unwrap();
        let xml = r#"
        <config>
          <moduleName>Example Mod</moduleName>
          <requiredInstallFiles><file source="readme.txt"/></requiredInstallFiles>
          <installSteps order="Explicit">
            <installStep name="Choose Option">
              <optionalFileGroups>
                <group name="Select an option:" type="SelectExactlyOne">
                  <plugins>
                    <plugin name="Option A">
                      <files><folder source="option_a"/></files>
                      <typeDescriptor><type name="Recommended"/></typeDescriptor>
                    </plugin>
                    <plugin name="Option B">
                      <files><folder source="option_b"/></files>
                      <typeDescriptor><type name="Optional"/></typeDescriptor>
                    </plugin>
                  </plugins>
                </group>
              </optionalFileGroups>
            </installStep>
          </installSteps>
        </config>"#;
        let facts = Facts { active: HashSet::new(), present: HashSet::new(), paths: HashSet::new() };
        let mut w = parse_wizard(xml, &root, None, "Example", facts).unwrap();
        assert!(w.needs_choice());
        let mods = t.join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        let name = w.install(&mods).unwrap();
        assert!(mods.join(&name).join("A.esp").is_file());
        assert!(!mods.join(&name).join("B.esp").exists());
        assert!(mods.join(&name).join("readme.txt").is_file());
        let _ = std::fs::remove_dir_all(&t);
    }

    #[test]
    fn an_exactly_one_group_with_two_picks_keeps_the_first() {
        let t = std::env::temp_dir().join(format!("nvf-fomod-one-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&t);
        for o in ["a", "b", "c"] {
            std::fs::create_dir_all(t.join(o)).unwrap();
            std::fs::write(t.join(o).join(format!("{o}.esp")), b"TES4").unwrap();
        }
        let option = |o: &str| format!(r#"<plugin name="{o}"><files><folder source="{o}"/></files><typeDescriptor><type name="Optional"/></typeDescriptor></plugin>"#);
        let xml = format!(
            r#"<config><moduleName>One</moduleName><installSteps order="Explicit"><installStep name="Step"><optionalFileGroups><group name="Pick" type="SelectExactlyOne"><plugins>{}{}{}</plugins></group></optionalFileGroups></installStep></installSteps></config>"#,
            option("a"),
            option("b"),
            option("c")
        );
        let facts = Facts { active: HashSet::new(), present: HashSet::new(), paths: HashSet::new() };
        let mut w = parse_wizard(&xml, &t, None, "One", facts).unwrap();
        w.chosen[0][0] = vec![false, true, true];
        w.normalize();
        assert_eq!(w.chosen[0][0], vec![false, true, false]);
        let _ = std::fs::remove_dir_all(&t);
    }

    #[test]
    fn two_roots_are_a_choice_and_a_reinstall_keeps_the_old_folder_until_the_new_one_is_in() {
        let t = std::env::temp_dir().join(format!("nvf-fomod-roots-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&t);
        let src = t.join("src");
        std::fs::create_dir_all(src.join("One")).unwrap();
        std::fs::create_dir_all(src.join("Two")).unwrap();
        std::fs::write(src.join("One/A.esp"), b"TES4").unwrap();
        std::fs::write(src.join("Two/B.esp"), b"TES4").unwrap();
        let facts = FactsInput { active: HashSet::new(), present: HashSet::new(), paths: HashSet::new() };
        let staged = stage(&src, facts).unwrap();
        let Staged::ChooseRoot { roots, temp, .. } = staged else { panic!("two plugin folders should be a choice") };
        assert_eq!(roots.len(), 2);
        discard(&temp);
        let mods = t.join("mods");
        let recycle = t.join("recycle");
        std::fs::create_dir_all(mods.join("Example")).unwrap();
        std::fs::write(mods.join("Example/old.txt"), b"old").unwrap();
        let from = t.join("from");
        std::fs::create_dir_all(from.join("meshes")).unwrap();
        std::fs::write(from.join("meshes/a.nif"), b"nif").unwrap();
        replace_tree(&mods, "Example", &from, &recycle).unwrap();
        assert!(mods.join("Example/meshes/a.nif").is_file());
        assert!(!mods.join("Example/old.txt").exists());
        let kept = std::fs::read_dir(&recycle).unwrap().flatten().any(|e| e.path().join("old.txt").is_file());
        assert!(kept);
        let missing = t.join("missing");
        std::fs::create_dir_all(mods.join("Example")).ok();
        let err = replace_tree(&mods, "Example", &missing, &recycle);
        assert!(err.is_err());
        assert!(mods.join("Example").exists());
        let _ = std::fs::remove_dir_all(&t);
    }

    #[test]
    fn manual_data_accepts_nvse_and_refuses_docs() {
        let t = std::env::temp_dir().join(format!("nvf-fomod-manual-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&t);
        let src = t.join("src");
        std::fs::create_dir_all(src.join("Data/NVSE")).unwrap();
        std::fs::write(src.join("Data/NVSE/nvse_loader.exe"), b"exe").unwrap();
        std::fs::write(src.join("README.md"), b"docs").unwrap();
        std::fs::write(src.join("LICENSE.txt"), b"lic").unwrap();
        let whole = scan_manual(&src);
        assert!(!data_valid(&whole));
        let just = scan_manual(&src.join("Data"));
        assert!(data_valid(&just));
        let mut skipped = just.clone();
        skipped.children[0].checked = false;
        set_branch(&mut skipped.children[0], false);
        assert!(!data_valid(&skipped));
        let dest = t.join("out");
        copy_checked(&src.join("Data"), &just, &dest).unwrap();
        assert!(dest.join("NVSE/nvse_loader.exe").is_file());
        assert!(!dest.join("README.md").exists());
        let mcm = t.join("mcm");
        std::fs::create_dir_all(&mcm).unwrap();
        std::fs::write(mcm.join("menu.xml"), b"<x/>").unwrap();
        assert!(data_valid(&scan_manual(&mcm)));
        let _ = std::fs::remove_dir_all(&t);
    }
}
