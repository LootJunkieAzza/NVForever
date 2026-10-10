//! `fnvtool combat`: the combat content of a load order (docs/design/combat.md §4, step
//! C1). Contract: docs/spec-notes/combat-data.md §4.

use anyhow::{bail, Context, Result};
use clap::Subcommand;
use fnv_combat::CombatForms;
use fnv_esm::{LoadOrder, Schema};
use fnv_sim::FormId;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::esm_cmd::open_load_order;
use crate::LoadOrderArgs;

#[derive(Subcommand)]
pub enum CombatCmd {
    /// One weapon's stats and the shot chain it resolves to: ammo choices, projectile,
    /// projectiles per shot, damage per projectile, explosion (fnv.combat.weapon/1).
    Weapon {
        #[command(flatten)]
        lo: LoadOrderArgs,
        #[arg(long)]
        schema: PathBuf,
        /// Weapon Editor ID or global FormID (hex).
        #[arg(long)]
        form: String,
        /// Ammo to load (Editor ID or FormID); default: the first the weapon accepts.
        #[arg(long)]
        ammo: Option<String>,
    },
    /// What a weapon's shots sound and look like: its sounds, shell casing, its
    /// projectile's and explosion's sounds, and the impacts of its impact data set by
    /// material; with the counts of the effect forms read (fnv.combat.fx/1).
    Fx {
        #[command(flatten)]
        lo: LoadOrderArgs,
        #[arg(long)]
        schema: PathBuf,
        /// Weapon Editor ID or global FormID (hex).
        #[arg(long)]
        form: String,
    },
    /// Any combat form as read: WEAP, AMMO, AMEF, PROJ, EXPL, BPTD, ENCH, SPEL
    /// (fnv.combat.form/1).
    Form {
        #[command(flatten)]
        lo: LoadOrderArgs,
        #[arg(long)]
        schema: PathBuf,
        /// Editor ID or global FormID (hex).
        #[arg(long)]
        form: String,
    },
    /// Every combat form of the load order: counts, how each weapon's chain resolves,
    /// references that point nowhere, and the combat game settings
    /// (fnv.combat.survey/1).
    Survey {
        #[command(flatten)]
        lo: LoadOrderArgs,
        #[arg(long)]
        schema: PathBuf,
    },
    /// The damage of one hit with a weapon under the C2 rules, without and with a
    /// critical (fnv.combat.damage/1).
    Damage {
        #[command(flatten)]
        lo: LoadOrderArgs,
        #[arg(long)]
        schema: PathBuf,
        /// Weapon Editor ID or FormID.
        #[arg(long)]
        form: String,
        #[command(flatten)]
        fight: crate::combat_sim::Fight,
    },
    /// Fire a weapon at a target on an empty range under the C2 rules: hits, criticals,
    /// reloads, time to kill (fnv.combat.sim/1). Seeded, so runs repeat.
    Sim {
        #[command(flatten)]
        lo: LoadOrderArgs,
        #[arg(long)]
        schema: PathBuf,
        #[arg(long)]
        form: String,
        #[command(flatten)]
        fight: crate::combat_sim::Fight,
        #[command(flatten)]
        range: crate::combat_sim::Shooting,
    },
}

fn load(lo: &LoadOrderArgs, schema: &PathBuf) -> Result<CombatForms> {
    let schema = Schema::load_dir(schema).with_context(|| schema.display().to_string())?;
    let plugins = open_load_order(lo)?;
    let lo = LoadOrder::new(&plugins)?;
    Ok(CombatForms::build(&lo, &schema))
}

fn resolve(c: &CombatForms, s: &str) -> Result<FormId> {
    if let Some(id) = c.by_edid(s) {
        return Ok(id);
    }
    let hex = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    match u32::from_str_radix(hex, 16) {
        Ok(v) if c.sigs.contains_key(&FormId(v)) => Ok(FormId(v)),
        _ => bail!("not a combat form (Editor ID or FormID): {s}"),
    }
}

fn sig(c: &CombatForms, id: FormId) -> String {
    c.sigs.get(&id).map(|s| String::from_utf8_lossy(s).into_owned()).unwrap_or_else(|| "none".into())
}

fn form_json(c: &CombatForms, id: FormId) -> Value {
    let v = match c.sigs.get(&id).map(|s| &s[..]) {
        Some(b"WEAP") => json!(c.weapons.get(&id)),
        Some(b"AMMO") => json!(c.ammo.get(&id)),
        Some(b"AMEF") => json!(c.ammo_effects.get(&id)),
        Some(b"PROJ") => json!(c.projectiles.get(&id)),
        Some(b"EXPL") => json!(c.explosions.get(&id)),
        Some(b"BPTD") => json!(c.body_parts.get(&id)),
        Some(b"ENCH") | Some(b"SPEL") => json!(c.effects.get(&id)),
        Some(b"FLST") => json!(c.form_lists.get(&id)),
        _ => Value::Null,
    };
    json!({"formid": id.to_string(), "sig": sig(c, id), "value": v})
}

pub fn run(cmd: CombatCmd) -> Result<Value> {
    match cmd {
        CombatCmd::Fx { lo, schema, form } => {
            let schema_v = Schema::load_dir(&schema).with_context(|| schema.display().to_string())?;
            let plugins = open_load_order(&lo)?;
            let order = LoadOrder::new(&plugins)?;
            let c = CombatForms::build(&order, &schema_v);
            let fx = fnv_combat::fx::FxForms::build(&order, &schema_v);
            let id = resolve(&c, &form)?;
            let chain = c.chain(id, None);
            let w = fx.weapons.get(&id);
            let sound = |f: Option<fnv_sim::FormId>| f.map(|f| json!({"formid": f.to_string(), "sound": fx.sounds.get(&f)}));
            let proj = chain.as_ref().and_then(|ch| ch.projectile);
            let expl = chain.as_ref().and_then(|ch| ch.explosion);
            let set = w.and_then(|w| w.impact_dataset);
            let impacts: serde_json::Map<String, Value> = fnv_combat::fx::IMPACT_MATERIALS
                .iter()
                .filter_map(|m| {
                    let i = fx.impact(set?, m)?;
                    Some((m.to_string(), json!({"impact": i, "sound_1": sound(i.sound_1), "sound_2": sound(i.sound_2)})))
                })
                .collect();
            let mut addons: Vec<&fnv_combat::fx::AddonNode> = fx.addons.values().collect();
            addons.sort_by_key(|a| a.index);
            Ok(json!({
                "schema": "fnv.combat.fx/1",
                "counts": {"sounds": fx.sounds.len(), "weapons": fx.weapons.len(), "projectiles": fx.projectiles.len(), "explosions": fx.explosions.len(), "addon_nodes": fx.addons.len(), "impacts": fx.impacts.len(), "impact_sets": fx.impact_sets.len()},
                "weapon": w,
                "shoot_3d": sound(w.and_then(|w| w.shoot_3d)),
                "shoot_dist": sound(w.and_then(|w| w.shoot_dist)),
                "shoot_2d": sound(w.and_then(|w| w.shoot_2d)),
                "shoot_loop": sound(w.and_then(|w| w.shoot_loop)),
                "no_ammo": sound(w.and_then(|w| w.no_ammo)),
                "equip": sound(w.and_then(|w| w.equip)),
                "projectile_sound": sound(proj.and_then(|p| fx.projectiles.get(&p)).and_then(|p| p.sound)),
                "explosion": expl.and_then(|e| fx.explosions.get(&e)),
                "impacts": impacts,
                "addon_nodes": addons,
            }))
        }
        CombatCmd::Weapon { lo, schema, form, ammo } => {
            let c = load(&lo, &schema)?;
            let id = resolve(&c, &form)?;
            if !c.weapons.contains_key(&id) {
                bail!("{form} is a {}, not a WEAP", sig(&c, id));
            }
            let ammo = ammo.map(|a| resolve(&c, &a)).transpose()?;
            let chain = c.chain(id, ammo).expect("weapon present");
            let mut related = Vec::new();
            for f in chain.ammo.iter().chain(&chain.projectile).chain(&chain.explosion).chain(&chain.ammo_effects) {
                related.push(form_json(&c, *f));
            }
            let w = &c.weapons[&id];
            for f in w.crit_effect.iter().chain(&w.enchantment) {
                related.push(form_json(&c, *f));
            }
            Ok(json!({
                "schema": "fnv.combat.weapon/1",
                "weapon": form_json(&c, id),
                "chain": chain,
                "related": related,
                "issues": c.issues.to_json(),
            }))
        }
        CombatCmd::Form { lo, schema, form } => {
            let c = load(&lo, &schema)?;
            let id = resolve(&c, &form)?;
            Ok(json!({"schema": "fnv.combat.form/1", "form": form_json(&c, id)}))
        }
        CombatCmd::Survey { lo, schema } => Ok(survey(&load(&lo, &schema)?)),
        CombatCmd::Damage { lo, schema, form, fight } => {
            let c = load(&lo, &schema)?;
            let id = resolve(&c, &form)?;
            let ammo = fight.ammo.as_deref().map(|a| resolve(&c, a)).transpose()?;
            crate::combat_sim::damage_report(&c, id, ammo, &fight)
        }
        CombatCmd::Sim { lo, schema, form, fight, range } => {
            let c = load(&lo, &schema)?;
            let id = resolve(&c, &form)?;
            let ammo = fight.ammo.as_deref().map(|a| resolve(&c, a)).transpose()?;
            crate::combat_sim::sim(&c, id, ammo, &fight, &range)
        }
    }
}

fn bump(m: &mut BTreeMap<String, u64>, k: impl Into<String>) {
    *m.entry(k.into()).or_default() += 1;
}

/// Counts plus a few samples per kind.
#[derive(Default)]
struct Tally {
    counts: BTreeMap<String, u64>,
    samples: BTreeMap<String, Vec<String>>,
}

impl Tally {
    fn note(&mut self, kind: &str, sample: impl FnOnce() -> String) {
        bump(&mut self.counts, kind);
        let s = self.samples.entry(kind.to_string()).or_default();
        if s.len() < 5 {
            s.push(sample());
        }
    }

    fn json(&self) -> Value {
        Value::Array(self.counts.iter().map(|(k, n)| json!({"kind": k, "count": n, "samples": self.samples.get(k)})).collect())
    }
}

/// Weapon types that attack in melee or unarmed (no projectile expected).
fn is_melee(anim: Option<&str>) -> bool {
    matches!(anim, Some("Hand to Hand") | Some("Melee (1 Hand)") | Some("Melee (2 Hand)"))
}

/// Game settings the design names (docs/design/combat.md §4.2), in its order.
const SETTINGS: &[&str] = &[
    "fDamageWeaponMult",
    "fDamageSkillBase",
    "fDamageSkillMult",
    "fAVDMeleeDamageStrengthMult",
    "fAVDMeleeDamageStrengthOffset",
    "fAVDUnarmedDamageBase",
    "fAVDUnarmedDamageMult",
    "fDamageArmConditionBase",
    "fDamageArmConditionMult",
    "fCombatDamageBonusSneakingMult",
    "fCombatDamageBonusMeleeSneakingMult",
    "fAVDCritLuckBase",
    "fAVDCritLuckMult",
    "fVATSCriticalChanceBonus",
    "fMaxArmorRating",
    "fCombatDistance",
    "fUnaimedSpreadPenalty",
    "fWalkingSpreadPenalty",
    "fRunningSpreadPenalty",
    "fStandingSpreadPenalty",
    "fCrippledArm1HSpreadPenalty",
    "fCrippledArm2HSpreadPenalty",
    "fCrippledArms1HSpreadPenalty",
    "fCrippledArms2HSpreadPenalty",
    "fWobbleToSkillConversion",
    "fMinGunSpreadValue",
    "fWeapStrengthReqPenalty",
    "fWeapSkillReqPenalty",
    "fNPCMaxGunWobbleAngle",
    "fKnockdownDamageMult",
    "fKnockdownDamageBase",
    "fKnockdownAgilMult",
    "fKnockdownAgilBase",
    "fKnockdownChance",
    "fKnockdownCurrentHealthThreshold",
    "fKnockdownBaseHealthThreshold",
    "fKnockbackDamageMult",
    "fKnockbackDamageBase",
    "fKnockbackAgilMult",
    "fKnockbackAgilBase",
    "fKnockbackForceMax",
    "fKnockbackTime",
    "fGunShellEjectSpeed",
    "fGunShellLifetime",
    "iXPDeathRewardHealthThreshold",
];

fn survey(c: &CombatForms) -> Value {
    let mut missing = Tally::default();
    let want = |what: &str, id: Option<FormId>, sigs: &[&[u8; 4]], from: FormId, t: &mut Tally| {
        if let Some(x) = id {
            match c.sigs.get(&x) {
                Some(s) if sigs.contains(&s) => {}
                other => t.note(&format!("{what} is not {}", sigs.iter().map(|s| String::from_utf8_lossy(&s[..]).into_owned()).collect::<Vec<_>>().join("/")), || {
                    format!("{from} -> {x} ({})", other.map(|s| String::from_utf8_lossy(s).into_owned()).unwrap_or_else(|| "absent".into()))
                }),
            }
        }
    };

    // Weapons and their chains.
    let mut w_counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut by_anim: BTreeMap<String, u64> = BTreeMap::new();
    let mut by_skill: BTreeMap<String, u64> = BTreeMap::new();
    let mut chain_t = Tally::default();
    let mut fire_rate: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
    for (&id, w) in &c.weapons {
        let label = || format!("{id} {}", w.edid.as_deref().unwrap_or("?"));
        bump(&mut w_counts, "total");
        if w.playable {
            bump(&mut w_counts, "playable");
        }
        if w.embedded {
            bump(&mut w_counts, "embedded");
        }
        if w.automatic {
            bump(&mut w_counts, "automatic");
        }
        if w.short_dnam {
            bump(&mut w_counts, "short DNAM");
        }
        if !w.mods.is_empty() {
            bump(&mut w_counts, "with mod slots");
        }
        bump(&mut by_anim, w.anim_type.clone().unwrap_or_else(|| "none".into()));
        bump(&mut by_skill, w.skill.clone().unwrap_or_else(|| "none".into()));
        let rate = if w.fire_rate == w.fire_rate.trunc() { format!("{}", w.fire_rate) } else { format!("{:.2}", w.fire_rate) };
        bump(fire_rate.entry(if w.automatic { "automatic" } else { "not automatic" }.into()).or_default(), rate);

        want("WEAP NAM0", w.ammo, &[b"AMMO", b"FLST"], id, &mut missing);
        want("WEAP projectile", w.projectile, &[b"PROJ"], id, &mut missing);
        want("WEAP crit effect", w.crit_effect, &[b"SPEL"], id, &mut missing);
        want("WEAP enchantment", w.enchantment, &[b"ENCH"], id, &mut missing);
        want("WEAP impact data set", w.impact_dataset, &[b"IPDS"], id, &mut missing);

        let ch = c.chain(id, None).expect("weapon present");
        let melee = is_melee(w.anim_type.as_deref());
        if w.ammo.is_some() && ch.ammo_choices.is_empty() {
            chain_t.note("ammo named but no AMMO resolves", label);
        }
        if ch.ammo_choices.len() > 1 {
            chain_t.note("several ammo choices", label);
        }
        match (melee, ch.projectile, ch.projectile_from) {
            (true, None, _) => chain_t.note("melee/unarmed, no projectile", label),
            (true, Some(_), _) => chain_t.note("melee/unarmed with a projectile", label),
            (false, None, _) => chain_t.note("ranged type without a projectile", label),
            (false, Some(_), Some(from)) => chain_t.note(&format!("projectile from the {from}"), label),
            (false, Some(_), None) => {}
        }
        if !melee && ch.projectiles_per_shot == 0 {
            chain_t.note("ranged, 0 projectiles per shot", label);
        }
        if ch.projectiles_per_shot > 1 {
            chain_t.note("several projectiles per shot", || format!("{} x{}", label(), ch.projectiles_per_shot));
        }
        if ch.explosion.is_some() {
            chain_t.note("chain ends in an explosion", label);
        }
        if !ch.ammo_effects.is_empty() {
            chain_t.note("default ammo has effects", label);
        }
        // Every acceptable ammo, not only the default one.
        for a in &ch.ammo_choices {
            let alt = c.chain(id, Some(*a)).expect("weapon present");
            if alt.projectile_from == Some("ammo") && ch.ammo != Some(*a) {
                chain_t.note("alternative ammo changes the projectile", || format!("{} with {a}", label()));
            }
        }
    }

    let mut ammo_t = Tally::default();
    for (&id, a) in &c.ammo {
        want("AMMO projectile", a.projectile, &[b"PROJ"], id, &mut missing);
        for e in &a.effects {
            want("AMMO effect", Some(*e), &[b"AMEF"], id, &mut missing);
        }
        if !a.effects.is_empty() {
            ammo_t.note("with ammo effects", || format!("{id} {}", a.edid.as_deref().unwrap_or("?")));
        }
        if a.projectile.is_some() {
            ammo_t.note("with own projectile", || format!("{id} {}", a.edid.as_deref().unwrap_or("?")));
        }
        if a.projectiles_per_shot > 0 {
            ammo_t.note("with own projectile count", || format!("{id} {} x{}", a.edid.as_deref().unwrap_or("?"), a.projectiles_per_shot));
        }
        if a.consumed.is_some() {
            ammo_t.note("adds an item when used", || format!("{id} {}", a.edid.as_deref().unwrap_or("?")));
        }
    }
    let mut amef: BTreeMap<String, u64> = BTreeMap::new();
    for a in c.ammo_effects.values() {
        bump(&mut amef, format!("{} / {}", a.kind.as_deref().unwrap_or("?"), a.operation.as_deref().unwrap_or("?")));
    }

    let mut proj_kind: BTreeMap<String, u64> = BTreeMap::new();
    let mut proj_flags: BTreeMap<String, u64> = BTreeMap::new();
    let mut proj_t = Tally::default();
    for (&id, p) in &c.projectiles {
        let label = || format!("{id} {}", p.edid.as_deref().unwrap_or("?"));
        bump(&mut proj_kind, p.kind.clone().unwrap_or_else(|| "none".into()));
        for fl in &p.flags {
            bump(&mut proj_flags, fl.clone());
        }
        want("PROJ explosion", p.explosion, &[b"EXPL"], id, &mut missing);
        want("PROJ default weapon", p.default_weapon, &[b"WEAP"], id, &mut missing);
        match (p.has("Explosion"), p.explosion.is_some()) {
            (true, false) => proj_t.note("Explosion flag without an explosion", label),
            (false, true) => proj_t.note("explosion set without the Explosion flag", label),
            _ => {}
        }
        if p.gravity != 0.0 && !p.has("Hitscan") {
            proj_t.note("simulated in flight (gravity, not hitscan)", label);
        }
    }

    let mut expl_flags: BTreeMap<String, u64> = BTreeMap::new();
    let mut expl_t = Tally::default();
    for (&id, e) in &c.explosions {
        for fl in &e.flags {
            bump(&mut expl_flags, fl.clone());
        }
        want("EXPL enchantment", e.enchantment, &[b"ENCH"], id, &mut missing);
        want("EXPL impact data set", e.impact_dataset, &[b"IPDS"], id, &mut missing);
        if e.radiation_level > 0.0 {
            expl_t.note("radiation", || format!("{id} {}", e.edid.as_deref().unwrap_or("?")));
        }
        if e.placed_object.is_some() {
            expl_t.note("placed impact object", || format!("{id} {}", e.edid.as_deref().unwrap_or("?")));
        }
    }

    let mut part_types: BTreeMap<String, u64> = BTreeMap::new();
    let mut parts_per: BTreeMap<String, u64> = BTreeMap::new();
    let mut head_mult: BTreeMap<String, u64> = BTreeMap::new();
    for (&id, b) in &c.body_parts {
        bump(&mut parts_per, b.parts.len().to_string());
        for p in &b.parts {
            bump(&mut part_types, p.part_type.clone().unwrap_or_else(|| "none".into()));
            if p.part_type.as_deref().is_some_and(|t| t.starts_with("Head")) {
                bump(&mut head_mult, format!("{}", p.damage_mult));
            }
            if p.node.is_none() {
                missing.note("BPTD part without a node", || id.to_string());
            }
        }
    }

    let mut eff_kinds: BTreeMap<String, u64> = BTreeMap::new();
    for e in c.effects.values() {
        bump(&mut eff_kinds, format!("{} {}", e.sig, e.kind.as_deref().unwrap_or("?")));
    }

    let mut settings = serde_json::Map::new();
    let mut absent = Vec::new();
    for name in SETTINGS {
        match c.settings.get(&name.to_ascii_lowercase()) {
            Some(s) => {
                settings.insert((*name).to_string(), json!(s));
            }
            None => absent.push(*name),
        }
    }
    // Every setting whose name mentions health, for the C5 health rule (§8).
    let health: BTreeMap<&String, &fnv_sim::settings::Setting> = c.settings.iter().filter(|(k, _)| k.contains("health")).collect();

    json!({
        "schema": "fnv.combat.survey/1",
        "counts": {
            "WEAP": c.weapons.len(), "AMMO": c.ammo.len(), "AMEF": c.ammo_effects.len(), "PROJ": c.projectiles.len(),
            "EXPL": c.explosions.len(), "BPTD": c.body_parts.len(),
            "ENCH": c.effects.values().filter(|e| e.sig == "ENCH").count(),
            "SPEL": c.effects.values().filter(|e| e.sig == "SPEL").count(),
            "FLST": c.form_lists.len(), "GMST": c.settings.len(),
        },
        "weapons": {"counts": w_counts, "by_anim_type": by_anim, "by_skill": by_skill, "fire_rate": fire_rate},
        "chains": chain_t.json(),
        "ammo": ammo_t.json(),
        "ammo_effects": amef,
        "projectiles": {"by_type": proj_kind, "flags": proj_flags, "notes": proj_t.json()},
        "explosions": {"flags": expl_flags, "notes": expl_t.json()},
        "body_parts": {"parts_per_record": parts_per, "part_types": part_types, "head_damage_mult": head_mult},
        "effects": eff_kinds,
        "references_not_resolving": missing.json(),
        "settings": settings,
        "settings_absent": absent,
        "health_settings": health,
        "issues": c.issues.to_json(),
    })
}
