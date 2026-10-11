//! `fnvtool actor factions`: the factions the records give an actor through its template
//! chain, and what the load order's records show about templates, leveled lists and the
//! direction of faction reactions (docs/spec-notes/factions.md).

use fnv_actor::{ActorForms, Resolver};
use fnv_esm::{LoadOrder, Schema};
use fnv_game::data::ActorDef;
use fnv_game::functions::{faction_relation, factions_of};
use fnv_game::game::{Game, GameConfig};
use fnv_game::templates::Picks;
use fnv_game::GameData;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};

const AGGRESSION: [&str; 4] = ["Unaggressive", "Aggressive", "Very Aggressive", "Frenzied"];
const REACTION: [&str; 4] = ["Neutral", "Enemy", "Ally", "Friend"];
/// How many names a list of examples holds.
const EXAMPLES: usize = 12;

fn from_template(d: &ActorDef) -> bool {
    d.template_factions
}

pub fn run(lo: &LoadOrder, schema: &Schema, data: &GameData, refs: &RefTable, actors: &[String], survey: bool) -> anyhow::Result<Value> {
    let forms = FormStore::build(lo, schema);
    // A game in its authored state: no quest started, no script run.
    let g = Game::new(data, World::new(&forms, refs, SimConfig::default()), GameConfig::default());
    let mut out = json!({
        "schema": "fnv.actor.factions/1",
        "state": "the authored game: no quest started, no script run",
        "leveled_lists": "one entry stands for a list: uniform among the entries of the highest level at or under 1, seeded by the reference (an NVForever choice, not the game's roll)",
    });
    if !actors.is_empty() {
        let list: Vec<Value> = actors.iter().map(|name| actor(&g, refs, name)).collect::<anyhow::Result<_>>()?;
        out["actors"] = json!(list);
    }
    if survey {
        out["survey"] = json!({
            "records": records(data),
            "reactions": reactions(data),
            "placed": placed(lo, schema, &forms, &g, refs),
        });
    }
    Ok(out)
}

fn kind(data: &GameData, id: FormId) -> String {
    data.sig(id).map_or("missing".into(), |s| String::from_utf8_lossy(&s).into_owned())
}

fn faction_list(data: &GameData, list: &[(FormId, i8)]) -> Value {
    list.iter().map(|(f, rank)| json!({ "faction": data.name(*f), "rank": rank })).collect()
}

/// A placed actor from a name, as `fnvtool actor values` reads it.
pub(crate) fn resolve(g: &Game, refs: &RefTable, name: &str) -> anyhow::Result<FormId> {
    if name.eq_ignore_ascii_case("player") {
        return Ok(FormId::PLAYER_REF);
    }
    let id = crate::ai_packages::find_form(g.data, refs, name).ok_or_else(|| anyhow::anyhow!("no form named {name}"))?;
    if id == FormId::PLAYER_REF || refs.get(id).is_some() {
        return Ok(id);
    }
    refs.refs.iter().filter(|(_, p)| p.base == id).map(|(r, _)| *r).min().ok_or_else(|| anyhow::anyhow!("{name} ({id}) has no placed reference"))
}

fn actor(g: &Game, refs: &RefTable, name: &str) -> anyhow::Result<Value> {
    let data = g.data;
    let r = resolve(g, refs, name)?;
    let base = if r == FormId::PLAYER_REF { FormId::PLAYER_BASE } else { refs.get(r).map(|p| p.base).unwrap_or(r) };
    // The chain as the picks of this reference walk it; a list (and the lists nested in
    // it) shows the actor picked.
    let mut picks = Picks::new(r);
    let mut chain = Vec::new();
    let mut at = Some(base);
    while let Some(id) = at.take().filter(|_| chain.len() < 40) {
        let mut step = json!({ "form": data.name(id), "record": kind(data, id) });
        if let Some(d) = data.actors.get(&id) {
            step["factions_from_template"] = json!(d.template_factions);
            step["own_factions"] = faction_list(data, &d.factions);
            step["template"] = json!(d.template.map(|t| data.name(t)));
            at = d.template.filter(|_| d.template_factions);
        } else if let Some(l) = data.leveled_actors.get(&id) {
            step["entries"] = l.entries.iter().map(|(level, e)| json!({ "level": level, "form": data.name(*e) })).collect();
            at = data.actor_base(id, &mut picks);
            step["picked"] = json!(at.map(|p| data.name(p)));
        }
        chain.push(step);
    }
    let source = data.template_source(base, r, from_template);
    let ai = data.template_source(base, r, |d| d.template_ai).and_then(|(_, d)| d.aggression);
    let relation = |a: FormId, b: FormId| REACTION[faction_relation(g, a, b).clamp(0, 3) as usize];
    Ok(json!({
        "actor": name,
        "reference": r.to_string(),
        "base": data.name(base),
        "chain": chain,
        "factions_source": source.map(|(id, _)| data.name(id)),
        "factions": source.map(|(_, d)| faction_list(data, &d.factions)),
        "factions_now": factions_of(g, r).iter().map(|f| data.name(*f)).collect::<Vec<_>>(),
        "aggression": ai.map(|a| AGGRESSION[a.clamp(0, 3) as usize]),
        "towards_player": relation(r, FormId::PLAYER_REF),
        "player_towards_it": relation(FormId::PLAYER_REF, r),
    }))
}

fn faction_set(d: &ActorDef) -> BTreeSet<(FormId, i8)> {
    d.factions.iter().copied().collect()
}

/// What the NPC_ and CREA records hold about the "Factions" template flag.
fn records(data: &GameData) -> Value {
    let mut by_sig: BTreeMap<String, [u64; 2]> = BTreeMap::new();
    let mut template_kind: BTreeMap<String, u64> = BTreeMap::new();
    let (mut own_empty, mut own_same, mut own_differs) = (0u64, 0u64, 0u64);
    let (mut through_list, mut pick_free, mut pick_matters, mut no_level_one) = (0u64, 0u64, 0u64, 0u64);
    let mut depth: BTreeMap<usize, u64> = BTreeMap::new();
    let mut differing = Vec::new();
    let mut mattering = Vec::new();
    let mut ids: Vec<&FormId> = data.actors.keys().collect();
    ids.sort();
    for &id in ids {
        let d = &data.actors[&id];
        let n = by_sig.entry(String::from_utf8_lossy(&d.sig).into_owned()).or_default();
        n[0] += 1;
        if !d.template_factions {
            continue;
        }
        n[1] += 1;
        *template_kind.entry(d.template.map_or("no template".into(), |t| kind(data, t))).or_default() += 1;
        let sources = data.template_sources_any_pick(id, from_template);
        // Chain length and lists on the way, along the first entries.
        let (mut steps, mut lists, mut at) = (0usize, false, id);
        for _ in 0..40 {
            if let Some(a) = data.actors.get(&at) {
                match a.template.filter(|t| a.template_factions && (data.actors.contains_key(t) || data.leveled_actors.contains_key(t))) {
                    Some(t) => (steps, at) = (steps + 1, t),
                    None => break,
                }
            } else if let Some(l) = data.leveled_actors.get(&at) {
                lists = true;
                match l.entries.first() {
                    Some(e) => at = e.1,
                    None => break,
                }
            } else {
                break;
            }
        }
        *depth.entry(steps).or_default() += 1;
        let sets: BTreeSet<BTreeSet<(FormId, i8)>> = sources.iter().map(|s| faction_set(&data.actors[s])).collect();
        if lists {
            through_list += 1;
            if sets.len() <= 1 {
                pick_free += 1;
            } else {
                pick_matters += 1;
                if mattering.len() < EXAMPLES {
                    mattering.push(data.name(id));
                }
            }
            // A list on the way with nothing for a level-1 player.
            let empty_at_one = d.template.and_then(|t| data.leveled_actors.get(&t)).is_some_and(|l| fnv_game::templates::leveled_pool(l, 1).is_empty());
            no_level_one += empty_at_one as u64;
        }
        if sets.len() == 1 && sources != [id] {
            let own = faction_set(d);
            if own.is_empty() {
                own_empty += 1;
            } else if Some(&own) == sets.iter().next() {
                own_same += 1;
            } else {
                own_differs += 1;
                if differing.len() < EXAMPLES {
                    differing.push(data.name(id));
                }
            }
        }
    }
    json!({
        "actors": by_sig.iter().map(|(k, v)| (k.clone(), json!({ "records": v[0], "factions_from_template": v[1] }))).collect::<Map<String, Value>>(),
        "leveled_lists": data.leveled_actors.len(),
        "template_of_flagged_actors_is": template_kind,
        "template_steps_followed": depth,
        "own_list_of_flagged_actors_with_one_possible_source": {
            "empty": own_empty,
            "same_as_the_source": own_same,
            "differs_from_the_source": own_differs,
            "differing_examples": differing,
        },
        "flagged_actors_whose_chain_passes_a_leveled_list": {
            "actors": through_list,
            "every_possible_pick_gives_the_same_factions": pick_free,
            "the_pick_decides_the_factions": pick_matters,
            "pick_decides_examples": mattering,
            "template_list_has_no_entry_at_level_1": no_level_one,
        },
    })
}

/// FACT XNAM lists: how often the two directions of a pair agree.
fn reactions(data: &GameData) -> Value {
    let react = |a: FormId, b: FormId| data.faction_reactions.get(&a).and_then(|l| l.iter().find(|x| x.0 == b)).map(|x| x.1);
    let (mut entries, mut to_self) = (0u64, 0u64);
    let mut by_reaction: BTreeMap<&str, u64> = BTreeMap::new();
    // Ordered pairs a → b (a ≠ b) by what b lists for a.
    let mut pairs: BTreeMap<String, u64> = BTreeMap::new();
    let mut one_way_enemies = Vec::new();
    let mut ids: Vec<&FormId> = data.faction_reactions.keys().collect();
    ids.sort();
    for &a in ids {
        for &(b, r) in &data.faction_reactions[&a] {
            entries += 1;
            let name = REACTION[r.clamp(0, 3) as usize];
            *by_reaction.entry(name).or_default() += 1;
            if a == b {
                to_self += 1;
                continue;
            }
            let back = react(b, a).map_or("not listed", |x| REACTION[x.clamp(0, 3) as usize]);
            *pairs.entry(format!("{name} / {back}")).or_default() += 1;
            if r == 1 && react(b, a) != Some(1) && one_way_enemies.len() < EXAMPLES {
                one_way_enemies.push(format!("{} -> {} ({back} back)", data.name(a), data.name(b)));
            }
        }
    }
    json!({
        "factions_with_a_list": data.faction_reactions.values().filter(|l| !l.is_empty()).count(),
        "entries": entries,
        "entries_by_reaction": by_reaction,
        "entries_naming_the_faction_itself": to_self,
        "listed_reaction_/_what_the_other_faction_lists_back": pairs,
        "one_way_enemy_examples": one_way_enemies,
    })
}

/// The placed actors: what the template rule changes, whether the pick agrees with the
/// actor drawn, and how they stand towards the player.
fn placed(lo: &LoadOrder, schema: &Schema, forms: &FormStore, g: &Game, refs: &RefTable) -> Value {
    let data = g.data;
    let drawn = ActorForms::build(lo, schema);
    let resolver = Resolver { actors: &drawn, forms };
    let (mut references, mut actors, mut list_base, mut through_template) = (0u64, 0u64, 0u64, 0u64);
    let (mut gained, mut changed, mut same, mut none_now) = (0u64, 0u64, 0u64, 0u64);
    let (mut pick_agrees, mut pick_differs, mut not_drawn) = (0u64, 0u64, 0u64);
    // aggression → what the actor's factions say of the player's → count.
    let mut stance: BTreeMap<String, BTreeMap<&str, u64>> = BTreeMap::new();
    let mut player_only_enemy = 0u64;
    // Creatures neutral to the player: aggression → factions source → count.
    let mut neutral_creatures: BTreeMap<&str, BTreeMap<String, u64>> = BTreeMap::new();
    for (&r, p) in &refs.refs {
        if !matches!(&p.sig, b"ACHR" | b"ACRE") {
            continue;
        }
        references += 1;
        let Some((source, def)) = data.template_source(p.base, r, from_template) else { continue };
        actors += 1;
        let own = data.actors.get(&p.base);
        list_base += own.is_none() as u64;
        // The first list on the way: the same entry as the actor drawn takes from it?
        let first_list = match own {
            None => Some(p.base),
            Some(d) => d.template.filter(|t| d.template_factions && data.leveled_actors.contains_key(t)),
        };
        if let Some(l) = first_list {
            match (resolver.first_pick(l, r), data.actor_base(l, &mut Picks::new(r))) {
                (None, None) => not_drawn += 1,
                (a, b) if a == b => pick_agrees += 1,
                _ => pick_differs += 1,
            }
        }
        through_template += (Some(source) != data.actor_base(p.base, &mut Picks::new(r))) as u64;
        // Against the record's own list, which was all that was read before.
        let before: BTreeSet<(FormId, i8)> = own.map(faction_set).unwrap_or_default();
        let now = faction_set(def);
        match (before.is_empty(), now.is_empty()) {
            (_, true) if before.is_empty() => none_now += 1,
            (true, false) => gained += 1,
            _ if before == now => same += 1,
            _ => changed += 1,
        }
        let aggression = data.template_source(p.base, r, |d| d.template_ai).and_then(|(_, d)| d.aggression).unwrap_or(0).clamp(0, 3) as usize;
        let towards = faction_relation(g, r, FormId::PLAYER_REF).clamp(0, 3) as usize;
        let who = if &def.sig == b"CREA" { "creatures" } else { "NPCs" };
        *stance.entry(format!("{who}, {}", AGGRESSION[aggression])).or_default().entry(REACTION[towards]).or_default() += 1;
        player_only_enemy += (towards != 1 && faction_relation(g, FormId::PLAYER_REF, r) == 1) as u64;
        if towards == 0 && &def.sig == b"CREA" {
            *neutral_creatures.entry(AGGRESSION[aggression]).or_default().entry(data.name(source)).or_default() += 1;
        }
    }
    let top = |m: BTreeMap<String, u64>| {
        let mut top: Vec<(String, u64)> = m.into_iter().collect();
        top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        top.truncate(EXAMPLES);
        top.into_iter().map(|(name, n)| format!("{name} {n}")).collect::<Vec<_>>()
    };
    let neutral_creatures: BTreeMap<&str, Vec<String>> = neutral_creatures.into_iter().map(|(k, m)| (k, top(m))).collect();
    json!({
        "actor_references": references,
        "placed_actors": actors,
        "base_is_a_leveled_list": list_base,
        "factions_come_from_a_template": through_template,
        "against_the_records_own_list": {
            "no_factions_either_way": none_now,
            "same_factions": same,
            "factions_where_the_own_list_was_empty": gained,
            "other_factions": changed,
        },
        "first_list_on_the_way_pick_against_the_actor_drawn": { "same": pick_agrees, "differs": pick_differs, "nothing_to_pick": not_drawn },
        "player_factions": factions_of(g, FormId::PLAYER_REF).iter().map(|f| data.name(*f)).collect::<Vec<_>>(),
        "aggression_/_the_actors_factions_towards_the_players": stance,
        "players_factions_name_it_an_enemy_but_not_the_other_way": player_only_enemy,
        "placed_creatures_neutral_to_the_player_by_aggression_and_factions_source": neutral_creatures,
    })
}
