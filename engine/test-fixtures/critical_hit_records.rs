//! Independently authored portable records for actual critical-damage hit consumers.
use fnv_esm::raw::{Subrecord, write_subrecords};
use fnv_sim::FormId;

pub(super) const GUN: FormId = FormId(0xa40);
pub(super) const CLUB: FormId = FormId(0xa41);
pub(super) const NEIGHBOR: FormId = FormId(0xa42);
pub(super) const PROJECTILE: FormId = FormId(0xa43);
pub(super) const ACTOR: FormId = FormId(0xa44);
pub(super) const SHOOTER: FormId = FormId(0xa45);
pub(super) const TARGET: FormId = FormId(0xa46);
pub(super) const CELL: FormId = FormId(0xa47);

fn sub(sig: &[u8; 4], data: Vec<u8>) -> Subrecord {
    Subrecord {
        sig: *sig,
        data,
        extended: false,
    }
}
fn z(text: &str) -> Vec<u8> {
    [text.as_bytes(), &[0]].concat()
}
fn record(sig: &[u8; 4], id: FormId, subs: &[Subrecord]) -> Vec<u8> {
    let body = write_subrecords(subs);
    [
        &sig[..],
        &(body.len() as u32).to_le_bytes(),
        &0u32.to_le_bytes(),
        &id.0.to_le_bytes(),
        &[0; 4],
        &15u16.to_le_bytes(),
        &[0; 2],
        &body,
    ]
    .concat()
}
fn group(label: [u8; 4], kind: i32, body: &[u8]) -> Vec<u8> {
    [
        &b"GRUP"[..],
        &((24 + body.len()) as u32).to_le_bytes(),
        &label,
        &kind.to_le_bytes(),
        &[0; 8],
        body,
    ]
    .concat()
}
fn weapon(id: FormId, name: &str, melee: bool) -> Vec<u8> {
    // The accepted WeaponDNAM/CRDT schema: melee-one-hand=1, pistol=3,
    // animation multiplier/reach at4/8, projectile at36, projectile count at42.
    let mut dn = vec![0; 204];
    dn[..4].copy_from_slice(&(if melee { 1u32 } else { 3 }).to_le_bytes());
    dn[4..8].copy_from_slice(&1f32.to_le_bytes());
    dn[8..12].copy_from_slice(&1f32.to_le_bytes());
    if !melee {
        dn[36..40].copy_from_slice(&PROJECTILE.0.to_le_bytes());
    }
    dn[42] = 1;
    let cr = [11u16.to_le_bytes().to_vec(), vec![0; 2], 1f32.to_le_bytes().to_vec(), vec![0; 8]].concat();
    let data = [
        0i32.to_le_bytes().to_vec(),
        100i32.to_le_bytes().to_vec(),
        1f32.to_le_bytes().to_vec(),
        20i16.to_le_bytes().to_vec(),
        vec![1],
    ]
    .concat();
    record(
        b"WEAP",
        id,
        &[sub(b"EDID", z(name)), sub(b"DNAM", dn), sub(b"DATA", data), sub(b"CRDT", cr)],
    )
}
fn placed(id: FormId, y: f32) -> Vec<u8> {
    let pose = [0f32, y, 0.0, 0.0, 0.0, 0.0].into_iter().flat_map(f32::to_le_bytes).collect();
    record(b"ACHR", id, &[sub(b"NAME", ACTOR.0.to_le_bytes().to_vec()), sub(b"DATA", pose)])
}
pub(super) fn plugin() -> fnv_esm::Plugin {
    let header = record(
        b"TES4",
        FormId(0),
        &[sub(
            b"HEDR",
            [1.34f32.to_le_bytes(), 0u32.to_le_bytes(), 0xb00u32.to_le_bytes()].concat(),
        )],
    );
    let npc = |id, name| {
        record(
            b"NPC_",
            id,
            &[
                sub(b"EDID", z(name)),
                sub(b"DATA", [1000i32.to_le_bytes().to_vec(), vec![5; 7]].concat()),
            ],
        )
    };
    let npcs = [npc(FormId::PLAYER_BASE, "Player"), npc(ACTOR, "CriticalHitActor")].concat();
    let weapons = [
        weapon(GUN, "CriticalHitGun", false),
        weapon(CLUB, "CriticalHitClub", true),
        weapon(NEIGHBOR, "CriticalHitNeighbor", false),
    ]
    .concat();
    // Accepted ProjectileData schema, a single physical non-hitscan missile.
    let mut pd = vec![0; 84];
    pd[2..4].copy_from_slice(&1u16.to_le_bytes());
    pd[8..12].copy_from_slice(&1000f32.to_le_bytes());
    pd[12..16].copy_from_slice(&5000f32.to_le_bytes());
    let proj = record(b"PROJ", PROJECTILE, &[sub(b"EDID", z("CriticalHitProjectile")), sub(b"DATA", pd)]);
    let cell = record(b"CELL", CELL, &[sub(b"EDID", z("CriticalHitCell")), sub(b"DATA", vec![1])]);
    let refs = [placed(SHOOTER, -100.0), placed(TARGET, 80.0)].concat();
    let children = group(CELL.0.to_le_bytes(), 6, &group(CELL.0.to_le_bytes(), 9, &refs));
    let cells = group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[cell, children].concat()));
    fnv_esm::Plugin::parse(
        &[
            header,
            group(*b"NPC_", 0, &npcs),
            group(*b"WEAP", 0, &weapons),
            group(*b"PROJ", 0, &proj),
            group(*b"CELL", 0, &cells),
        ]
        .concat(),
        "CriticalHitConsumer.esm".into(),
    )
    .unwrap()
}
