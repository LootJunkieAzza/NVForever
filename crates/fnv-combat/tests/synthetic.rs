//! `fnv_combat::data` on a small synthetic plugin: a weapon with two ammo types through a
//! form list, projectiles (one exploding), an ammo effect, body-part data and a game
//! setting. Layouts as in `schemas/fnv`; values chosen for the test.

use fnv_combat::CombatForms;
use fnv_esm::raw::{write_subrecords, Subrecord};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_sim::FormId;

const RIFLE: u32 = 0x800;
const BULLET: u32 = 0x810;
const GRENADE: u32 = 0x811;
const AMMO_LIST: u32 = 0x820;
const AMMO_PLAIN: u32 = 0x830;
const AMMO_SPECIAL: u32 = 0x831;
const AMMO_EFFECT: u32 = 0x840;
const BLAST: u32 = 0x850;
const BODY: u32 = 0x860;
const SETTING: u32 = 0x870;

fn sub(sig: &[u8; 4], data: Vec<u8>) -> Subrecord {
    Subrecord { sig: *sig, data, extended: false }
}

fn z(s: &str) -> Vec<u8> {
    let mut v = s.as_bytes().to_vec();
    v.push(0);
    v
}

fn record(sig: &[u8; 4], fid: u32, subs: &[Subrecord]) -> Vec<u8> {
    let payload = write_subrecords(subs);
    [&sig[..], &(payload.len() as u32).to_le_bytes(), &0u32.to_le_bytes(), &fid.to_le_bytes(), &[0; 4], &15u16.to_le_bytes(), &[0; 2], &payload].concat()
}

fn top(sig: &[u8; 4], body: &[u8]) -> Vec<u8> {
    [&b"GRUP"[..], &((24 + body.len()) as u32).to_le_bytes(), sig, &0i32.to_le_bytes(), &[0; 8], body].concat()
}

/// A byte buffer written field by field at known offsets.
struct Buf(Vec<u8>);

impl Buf {
    fn new(len: usize) -> Buf {
        Buf(vec![0; len])
    }
    fn put(mut self, at: usize, bytes: &[u8]) -> Buf {
        self.0[at..at + bytes.len()].copy_from_slice(bytes);
        self
    }
    fn f32(self, at: usize, v: f32) -> Buf {
        self.put(at, &v.to_le_bytes())
    }
    fn u32(self, at: usize, v: u32) -> Buf {
        self.put(at, &v.to_le_bytes())
    }
}

fn plugin_bytes() -> Vec<u8> {
    let hedr = [&1.34f32.to_le_bytes()[..], &0i32.to_le_bytes(), &0x800u32.to_le_bytes()].concat();
    let tes4 = record(b"TES4", 0, &[sub(b"HEDR", hedr)]);

    // WEAP DATA: value i32, health i32, weight f32, base damage i16, clip size u8.
    let data = [&100i32.to_le_bytes()[..], &250i32.to_le_bytes(), &6.0f32.to_le_bytes(), &20i16.to_le_bytes(), &[5u8]].concat();
    // DNAM in its short form (120 bytes, up to limb_dmg_mult).
    let dnam = Buf::new(120)
        .u32(0, 5) // animation type 5: Rifle - Ballistic (2 Hand)
        .put(12, &[0b10]) // flags 1: Is Automatic
        .put(14, &[1]) // ammo use
        .f32(16, 1.5) // min spread
        .u32(36, BULLET) // projectile
        .put(42, &[1]) // projectile count
        .f32(64, 8.0) // fire rate
        .0;
    // CRDT: critical damage u16, 2 unused, crit % mult f32, flags u8, 3 unused, effect.
    let crdt = [&10u16.to_le_bytes()[..], &[0; 2], &1.0f32.to_le_bytes(), &[0; 4], &0u32.to_le_bytes()].concat();
    let weap = record(
        b"WEAP",
        RIFLE,
        &[sub(b"EDID", z("TestRifle")), sub(b"NAM0", AMMO_LIST.to_le_bytes().to_vec()), sub(b"DATA", data), sub(b"DNAM", dnam), sub(b"CRDT", crdt)],
    );

    // AMMO DAT2: projectiles per shot u32, projectile, weight f32.
    let dat2 = |n: u32, proj: u32| [&n.to_le_bytes()[..], &proj.to_le_bytes(), &0.1f32.to_le_bytes()].concat();
    let ammo_plain = record(b"AMMO", AMMO_PLAIN, &[sub(b"EDID", z("TestAmmo")), sub(b"DAT2", dat2(0, 0))]);
    let ammo_special = record(
        b"AMMO",
        AMMO_SPECIAL,
        &[sub(b"EDID", z("TestAmmoGrenade")), sub(b"DAT2", dat2(4, GRENADE)), sub(b"RCIL", AMMO_EFFECT.to_le_bytes().to_vec())],
    );
    let flst = record(
        b"FLST",
        AMMO_LIST,
        &[sub(b"EDID", z("TestAmmoList")), sub(b"LNAM", AMMO_PLAIN.to_le_bytes().to_vec()), sub(b"LNAM", AMMO_SPECIAL.to_le_bytes().to_vec())],
    );
    // AMEF DATA: type 0 Damage Mod, operation 1 Multiply, value.
    let amef = record(b"AMEF", AMMO_EFFECT, &[sub(b"EDID", z("TestAmef")), sub(b"DATA", Buf::new(12).u32(4, 1).f32(8, 1.25).0)]);

    // PROJ DATA (84 bytes): flags u16, type u16, gravity, speed, range, …, explosion at 36.
    let proj = |fid: u32, edid: &str, flags: u16, kind: u16, explosion: u32| {
        let d = Buf::new(84).put(0, &flags.to_le_bytes()).put(2, &kind.to_le_bytes()).f32(4, 1.0).f32(8, 20000.0).f32(12, 10000.0).u32(36, explosion).0;
        record(b"PROJ", fid, &[sub(b"EDID", z(edid)), sub(b"DATA", d), sub(b"VNAM", 1u32.to_le_bytes().to_vec())])
    };
    let bullet = proj(BULLET, "TestBullet", 0b1, 1, 0); // Hitscan, Missile
    let grenade = proj(GRENADE, "TestGrenade", 0b10, 2, BLAST); // Explosion, Lobber

    // EXPL DATA (60 bytes): force, damage, radius, light, sound 1, flags, IS radius,
    // impact data set, sound 2, radiation (3 floats), sound level.
    let expl = record(
        b"EXPL",
        BLAST,
        &[sub(b"EDID", z("TestBlast")), sub(b"DATA", Buf::new(60).f32(0, 100.0).f32(4, 80.0).f32(8, 300.0).u32(20, 0b1000).0)],
    );

    // BPTD: a named torso, then a head without BPTN (the name is optional).
    let bpnd = |mult: f32, part_type: u8, health: u8| Buf::new(84).f32(0, mult).put(5, &[part_type, health]).0;
    let bptd = record(
        b"BPTD",
        BODY,
        &[
            sub(b"EDID", z("TestBody")),
            sub(b"BPTN", z("Torso")),
            sub(b"BPNN", z("Bip01 Spine1")),
            sub(b"BPNT", z("Bip01 Spine1")),
            sub(b"BPNI", z("")),
            sub(b"BPND", bpnd(1.0, 0, 100)),
            sub(b"NAM1", z("")),
            sub(b"NAM4", z("")),
            sub(b"BPNN", z("Bip01 Head")),
            sub(b"BPNT", z("Bip01 Head")),
            sub(b"BPNI", z("")),
            sub(b"BPND", bpnd(2.0, 1, 60)),
            sub(b"NAM1", z("")),
            sub(b"NAM4", z("")),
        ],
    );
    let gmst = record(b"GMST", SETTING, &[sub(b"EDID", z("fDamageSkillBase")), sub(b"DATA", 0.5f32.to_le_bytes().to_vec())]);

    [
        tes4,
        top(b"GMST", &gmst),
        top(b"WEAP", &weap),
        top(b"AMMO", &[ammo_plain, ammo_special].concat()),
        top(b"AMEF", &amef),
        top(b"FLST", &flst),
        top(b"PROJ", &[bullet, grenade].concat()),
        top(b"EXPL", &expl),
        top(b"BPTD", &bptd),
    ]
    .concat()
}

fn forms() -> CombatForms {
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Test.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    CombatForms::build(&lo, &schema)
}

#[test]
fn weapon_stats() {
    let c = forms();
    assert!(c.issues.is_empty(), "{}", c.issues.to_json());
    let w = &c.weapons[&FormId(RIFLE)];
    assert_eq!(w.edid.as_deref(), Some("TestRifle"));
    assert_eq!((w.base_damage, w.clip_size, w.health), (20, 5, 250));
    assert_eq!(w.anim_type.as_deref(), Some("Rifle - Ballistic (2 Hand)"));
    assert!(w.automatic);
    assert_eq!((w.min_spread, w.fire_rate, w.projectile_count, w.ammo_use), (1.5, 8.0, 1, 1));
    assert_eq!(w.projectile, Some(FormId(BULLET)));
    assert!(w.short_dnam);
    assert_eq!((w.crit_damage, w.crit_mult), (10, 1.0));
    assert_eq!(c.ammo_choices(w), vec![FormId(AMMO_PLAIN), FormId(AMMO_SPECIAL)]);
    assert_eq!(c.by_edid("testrifle"), Some(FormId(RIFLE)));
    assert_eq!(c.setting("FDAMAGESKILLBASE"), Some(0.5));
}

#[test]
fn chain_uses_the_weapon_projectile_by_default() {
    let c = forms();
    let ch = c.chain(FormId(RIFLE), None).unwrap();
    assert_eq!(ch.ammo, Some(FormId(AMMO_PLAIN)));
    assert_eq!((ch.projectile, ch.projectile_from), (Some(FormId(BULLET)), Some("weapon")));
    assert_eq!((ch.projectiles_per_shot, ch.damage_per_projectile), (1, 20.0));
    assert_eq!(ch.explosion, None);
    let p = &c.projectiles[&FormId(BULLET)];
    assert_eq!((p.kind.as_deref(), p.speed), (Some("Missile"), 20000.0));
    assert!(p.has("Hitscan"));
}

#[test]
fn ammo_replaces_projectile_and_count() {
    let c = forms();
    let ch = c.chain(FormId(RIFLE), Some(FormId(AMMO_SPECIAL))).unwrap();
    assert_eq!((ch.projectile, ch.projectile_from), (Some(FormId(GRENADE)), Some("ammo")));
    assert_eq!((ch.projectiles_per_shot, ch.damage_per_projectile), (4, 5.0));
    assert_eq!(ch.explosion, Some(FormId(BLAST)));
    assert_eq!(ch.ammo_effects, vec![FormId(AMMO_EFFECT)]);
    let e = &c.ammo_effects[&FormId(AMMO_EFFECT)];
    assert_eq!((e.kind.as_deref(), e.operation.as_deref(), e.value), (Some("Damage Mod"), Some("Multiply"), 1.25));
    let x = &c.explosions[&FormId(BLAST)];
    assert_eq!((x.force, x.damage, x.radius), (100.0, 80.0, 300.0));
    assert_eq!(x.flags, vec!["Knock Down - By Formula".to_string()]);
    // Ammo the weapon does not accept falls back to the first choice.
    assert_eq!(c.chain(FormId(RIFLE), Some(FormId(BLAST))).unwrap().ammo, Some(FormId(AMMO_PLAIN)));
}

#[test]
fn body_parts_group_with_and_without_names() {
    let c = forms();
    let b = &c.body_parts[&FormId(BODY)];
    assert_eq!(b.parts.len(), 2);
    assert_eq!((b.parts[0].name.as_deref(), b.parts[0].node.as_deref()), (Some("Torso"), Some("Bip01 Spine1")));
    assert_eq!(b.parts[0].part_type.as_deref(), Some("Torso"));
    assert_eq!((b.parts[1].name.as_deref(), b.parts[1].node.as_deref()), (None, Some("Bip01 Head")));
    assert_eq!((b.parts[1].part_type.as_deref(), b.parts[1].damage_mult, b.parts[1].health_percent), (Some("Head 1"), 2.0, 60));
}
