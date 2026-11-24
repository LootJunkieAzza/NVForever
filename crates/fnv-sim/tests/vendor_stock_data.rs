//! What two vendors' chests roll to from FalloutNV.esm: Chet's `VendorContainerChet`
//! (0x00104C7B, placed as 0x00104C7A) and Doc Mitchell's `VendorContainerDocMitchell`
//! (0x00177BEE, placed as 0x00177BEF), at player levels 1 and 10, through the same
//! resolver a merchant restock uses (`World::roll_inventory`: every CNTO entry through
//! `leveled::resolve_with`). Owner report of 2026-11-12, "vendors inventories are way too
//! small and lack a lot of items": these lock in that nothing of the chests is dropped.
//!
//! The figures [observed 2026-11-12: `fnvtool sim sample` on every leveled entry of the
//! two chests, and the owner's autosave of that night, where Chet's chest held 41 stacks
//! and Doc Mitchell's 8, both what these rolls give]:
//! - Chet at level 1: 32 to 41 stacks, about 37; ammunition of 15+ kinds (the nested
//!   `0x001607C8` alone gives one of each of 16), 5 to 7 weapons, Leather Armor, chems,
//!   weapon mods, Caravan cards, the two fixed ammo boxes, and caps from the two vendor
//!   caps lists ("each item in count"): 630 to 870 from FalloutNV.esm alone, which these
//!   tests load (720 to 960 with the owner's DLCs; the wiki's low end is 630).
//! - Doc Mitchell at level 1: 6 to 10 stacks (100 caps, stimpaks, a doctor's bag, the
//!   medical junk list rolled 7 times).
//!
//! Needs the game data, which is never in the repository: set `FNVLAB_DATA_DIR` to the
//! game's `Data` folder. Without it the test prints why it was skipped and passes.

use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_sim::forms::{BaseData, Entry, ItemKind};
use fnv_sim::{leveled, FormId, FormStore};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const CHET: FormId = FormId(0x0010_4C7B);
const DOC: FormId = FormId(0x0017_7BEE);
const CAPS: FormId = FormId(0x0000_000F);
/// Rolls per container and level.
const ROLLS: u64 = 200;

fn data_dir() -> Option<PathBuf> {
    let Some(dir) = std::env::var_os("FNVLAB_DATA_DIR").map(PathBuf::from) else {
        eprintln!("skipped: FNVLAB_DATA_DIR is not set (no game data)");
        return None;
    };
    if !dir.join("FalloutNV.esm").is_file() {
        eprintln!("skipped: FalloutNV.esm is not in {}", dir.display());
        return None;
    }
    Some(dir)
}

fn load(dir: &Path) -> FormStore {
    let plugins = vec![Plugin::open(dir.join("FalloutNV.esm")).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let schema = Schema::load_dir(root.join("schemas/fnv")).unwrap();
    FormStore::build(&lo, &schema)
}

fn entries(forms: &FormStore, base: FormId) -> Vec<Entry> {
    match forms.get(base).map(|b| &b.data) {
        Some(BaseData::Container(c)) => c.entries.clone(),
        _ => panic!("{base} is not a container"),
    }
}

/// One restock's contents: items summed per form, as `World::roll_inventory` adds them.
fn roll(forms: &FormStore, base: FormId, level: u16, seed: u64) -> BTreeMap<FormId, i64> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut out = Vec::new();
    for e in entries(forms, base) {
        leveled::resolve(forms, e.item, e.count, level, &mut rng, &mut out).unwrap();
    }
    let mut sum = BTreeMap::new();
    for e in out.into_iter().filter(|e| e.count > 0) {
        *sum.entry(e.item).or_default() += i64::from(e.count);
    }
    sum
}

#[derive(Debug, Default)]
struct Stats {
    stacks: Vec<usize>,
    caps: Vec<i64>,
    /// Stacks of each kind, summed over the rolls.
    kinds: BTreeMap<ItemKind, usize>,
    /// Stacks of these kinds in each roll.
    weapons: Vec<usize>,
    ammo: Vec<usize>,
}

fn stats(forms: &FormStore, base: FormId, level: u16) -> Stats {
    let mut s = Stats::default();
    for seed in 0..ROLLS {
        let r = roll(forms, base, level, seed);
        let kind = |k| r.keys().filter(|i| **i != CAPS && forms.item(**i).is_some_and(|it| it.kind == k)).count();
        s.stacks.push(r.len());
        s.caps.push(r.get(&CAPS).copied().unwrap_or(0));
        s.weapons.push(kind(ItemKind::Weapon));
        s.ammo.push(kind(ItemKind::Ammo));
        for i in r.keys().filter(|i| **i != CAPS) {
            if let Some(it) = forms.item(*i) {
                *s.kinds.entry(it.kind).or_default() += 1;
            }
        }
    }
    let mean = s.stacks.iter().sum::<usize>() as f64 / s.stacks.len() as f64;
    eprintln!(
        "{base} level {level}: stacks {}..{} (mean {mean:.1}), caps {}..{}, weapons {}..{}, ammo kinds {}..{}, stacks by kind over {ROLLS} rolls {:?}",
        s.stacks.iter().min().unwrap(),
        s.stacks.iter().max().unwrap(),
        s.caps.iter().min().unwrap(),
        s.caps.iter().max().unwrap(),
        s.weapons.iter().min().unwrap(),
        s.weapons.iter().max().unwrap(),
        s.ammo.iter().min().unwrap(),
        s.ammo.iter().max().unwrap(),
        s.kinds
    );
    s
}

#[test]
fn chets_chest_rolls_its_whole_stock() {
    let Some(dir) = data_dir() else { return };
    let forms = load(&dir);
    assert_eq!(entries(&forms, CHET).len(), 20, "VendorContainerChet's CNTO entries");
    for level in [1, 10] {
        let s = stats(&forms, CHET, level);
        // Every nested list followed, every count kept: 30+ stacks each time.
        assert!(*s.stacks.iter().min().unwrap() >= 30, "level {level}: {:?}", s.stacks);
        assert!(*s.ammo.iter().min().unwrap() >= 10, "level {level}: ammo kinds {:?}", s.ammo);
        assert!(*s.weapons.iter().min().unwrap() >= 3, "level {level}: weapons {:?}", s.weapons);
        for k in [ItemKind::Armor, ItemKind::Aid, ItemKind::ItemMod, ItemKind::CaravanCard, ItemKind::Misc] {
            assert!(s.kinds.get(&k).copied().unwrap_or(0) >= ROLLS as usize / 2, "level {level}: {k:?} {:?}", s.kinds);
        }
    }
    // The two fixed entries (the boxed .38 Special and bulk small energy cells the wiki
    // says he always has after a restock) are in every roll.
    for seed in 0..20 {
        let r = roll(&forms, CHET, 1, seed);
        assert!(r.contains_key(&FormId(0x0016_664F)) && r.contains_key(&FormId(0x0016_6654)), "seed {seed}");
    }
    // Caps: `VendorCapsBaseMedium` x2 and the second caps list x3, each item in count.
    let one = stats(&forms, CHET, 1);
    assert!(one.caps.iter().all(|c| (630..=870).contains(c)), "level 1 caps {:?}", one.caps);
    let ten = stats(&forms, CHET, 10);
    assert!(ten.caps.iter().all(|c| *c >= 2400), "level 10 caps {:?}", ten.caps);
}

#[test]
fn doc_mitchells_chest_rolls_its_whole_stock() {
    let Some(dir) = data_dir() else { return };
    let forms = load(&dir);
    assert_eq!(entries(&forms, DOC).len(), 8, "VendorContainerDocMitchell's CNTO entries");
    for level in [1, 10] {
        let s = stats(&forms, DOC, level);
        assert!(*s.stacks.iter().min().unwrap() >= 5, "level {level}: {:?}", s.stacks);
        assert!(s.caps.iter().all(|c| *c >= 100), "level {level}: caps {:?}", s.caps);
        // Stimpaks every time (`0x000C499B` x2 has no chance none).
        for seed in 0..20 {
            assert!(roll(&forms, DOC, level, seed).contains_key(&FormId(0x0001_5169)), "seed {seed}");
        }
    }
}
