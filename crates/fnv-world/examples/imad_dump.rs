//! Image space modifiers (IMAD) as screen effects: every track with all its keys
//! (docs/spec-notes/image-modifiers.md).
//!
//! - `<EDID or 0xFormID>...`: the named records in full.
//! - `--survey`: over every record: DNAM size, whether each key count in DNAM equals the
//!   key count of its subrecord, animatable and duration, key time range, and which
//!   effects any record uses.
//! - `--list`: one line per record with the effects it uses.
//!
//! Reads `FalloutNV.esm` only.
//!
//! cargo run --release -p fnv-world --example imad_dump -- <Data dir> <esm schema dir> [--survey | --list | <EDID or 0xFormID>...]

use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_world::fields::Fields;
use fnv_world::image_modifier::{image_modifier, ImageModifier, Track};
use std::path::Path;

/// DNAM key counts in order, each with the subrecord whose keys it counts [documented:
/// xEdit wbDefinitionsFNV.pas IMAD]: 21 multiply/add pairs from byte 8, then the single
/// tracks at these byte offsets.
const SINGLE: [(usize, &[u8; 4]); 13] = [
    (176, b"TNAM"),
    (180, b"BNAM"),
    (184, b"VNAM"),
    (188, b"RNAM"),
    (192, b"SNAM"),
    (196, b"UNAM"),
    (212, b"WNAM"),
    (216, b"XNAM"),
    (220, b"YNAM"),
    (228, b"NAM1"),
    (232, b"NAM2"),
    (236, b"NAM3"),
    (240, b"NAM4"),
];

/// A record with its raw DNAM and the size of each of its subrecords.
type Rec = (ImageModifier, Vec<u8>, Vec<([u8; 4], usize)>);

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = Path::new(&a[1]);
    let schema = Schema::load_dir(Path::new(&a[2])).unwrap();
    let plugins = vec![Plugin::open(data.join("FalloutNV.esm")).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let mut all: Vec<Rec> = Vec::new();
    for (pi, recs) in lo.records.iter().enumerate() {
        for r in recs.iter().filter(|r| &r.header.sig == b"IMAD") {
            let fid = lo.to_global(pi, r.header.form_id).unwrap();
            let f = Fields::new(r, pi, &schema).unwrap();
            let dnam = f.raw(b"DNAM").map(|s| s.data.to_vec()).unwrap_or_default();
            let sizes = f.subs.iter().map(|s| (s.sig, s.data.len())).collect();
            if let Some(m) = image_modifier(&lo, &schema, fid) {
                all.push((m, dnam, sizes));
            }
        }
    }
    match a.get(3).map(String::as_str) {
        Some("--survey") => survey(&all),
        Some("--list") | None => all.iter().for_each(|(m, _, _)| println!("{}", line(m))),
        _ => {
            for want in &a[3..] {
                let id = u32::from_str_radix(want.trim_start_matches("0x"), 16).ok();
                match all.iter().find(|(m, _, _)| m.edid.eq_ignore_ascii_case(want) || Some(m.form_id) == id) {
                    Some((m, _, _)) => full(m),
                    None => println!("{want}: not an IMAD"),
                }
            }
        }
    }
}

fn keys(t: &Track) -> String {
    t.0.iter().map(|(t, v)| format!("{t}:{v}")).collect::<Vec<_>>().join(" ")
}

/// Whether a track ever leaves `rest`.
fn moves(t: &Track, rest: f32) -> bool {
    t.0.iter().any(|k| k.1 != rest)
}

fn uses(m: &ImageModifier) -> Vec<&'static str> {
    let mut u = Vec::new();
    let ma = |p: &fnv_world::image_modifier::MultAddTrack| moves(&p.mult, 1.0) || moves(&p.add, 0.0);
    if ma(&m.saturation) {
        u.push("saturation");
    }
    if ma(&m.contrast_avg_lum) {
        u.push("0x12");
    }
    if ma(&m.contrast) {
        u.push("0x13");
    }
    if ma(&m.brightness) {
        u.push("brightness");
    }
    if m.tint.0.iter().any(|k| k.1[3] != 0.0) {
        u.push("tint");
    }
    if m.fade.0.iter().any(|k| k.1[3] != 0.0) {
        u.push("fade");
    }
    if moves(&m.blur_radius, 0.0) {
        u.push("blur");
    }
    if moves(&m.double_vision, 0.0) {
        u.push("double");
    }
    if moves(&m.radial.strength, 0.0) {
        u.push("radial");
    }
    if moves(&m.dof.strength, 0.0) {
        u.push("dof");
    }
    if moves(&m.motion_blur, 0.0) {
        u.push("motion");
    }
    u
}

fn line(m: &ImageModifier) -> String {
    format!("{:#010x} {:36} {} {:7.3} s  {}", m.form_id, m.edid, if m.animatable { "anim  " } else { "static" }, m.duration, uses(m).join(" "))
}

fn full(m: &ImageModifier) {
    println!("{}", line(m));
    for (name, p) in [("saturation", &m.saturation), ("0x12", &m.contrast_avg_lum), ("0x13", &m.contrast), ("brightness", &m.brightness)] {
        println!("  {name:11} mult [{}] add [{}]", keys(&p.mult), keys(&p.add));
    }
    let colors = |c: &fnv_world::image_modifier::ColorTrack| {
        c.0.iter().map(|(t, c)| format!("{t}:({:.3} {:.3} {:.3} {:.3})", c[0], c[1], c[2], c[3])).collect::<Vec<_>>().join(" ")
    };
    println!("  tint        [{}]", colors(&m.tint));
    println!("  fade        [{}]", colors(&m.fade));
    println!("  blur radius [{}]", keys(&m.blur_radius));
    println!("  double      [{}]", keys(&m.double_vision));
    let r = &m.radial;
    println!(
        "  radial      strength [{}] ramp up [{}] start [{}] ramp down [{}] down start [{}] use target {} centre {:?}",
        keys(&r.strength),
        keys(&r.ramp_up),
        keys(&r.start),
        keys(&r.ramp_down),
        keys(&r.down_start),
        r.use_target,
        r.center
    );
    let d = &m.dof;
    println!("  dof         strength [{}] distance [{}] range [{}] use target {} flags {:#04x}", keys(&d.strength), keys(&d.distance), keys(&d.range), d.use_target, d.flags);
    println!("  motion blur [{}]", keys(&m.motion_blur));
}

fn survey(all: &[Rec]) {
    let u32_at = |d: &[u8], o: usize| d.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
    let (mut sizes, mut bad, mut checked) = (std::collections::BTreeMap::new(), 0usize, 0usize);
    let (mut anim, mut tmin, mut tmax) = (0usize, f32::MAX, f32::MIN);
    let (mut dmin, mut dmax) = (f32::MAX, f32::MIN);
    let mut used = std::collections::BTreeMap::new();
    let mut flags = std::collections::BTreeMap::new();
    let mut odd: Vec<String> = Vec::new();
    for (m, dnam, subs) in all {
        *sizes.entry(dnam.len()).or_insert(0usize) += 1;
        let len = |sig: &[u8; 4], key: usize| subs.iter().find(|s| &s.0 == sig).map(|s| s.1 / key);
        let mut check = |count: Option<usize>, have: Option<usize>| {
            checked += 1;
            if count != have {
                bad += 1;
            }
        };
        for i in 0..21u8 {
            check(u32_at(dnam, 8 + 8 * i as usize), len(&[i, b'I', b'A', b'D'], 8));
            check(u32_at(dnam, 12 + 8 * i as usize), len(&[i + 0x40, b'I', b'A', b'D'], 8));
        }
        for (o, sig) in SINGLE {
            check(u32_at(dnam, o), len(sig, if sig == b"TNAM" || sig == b"NAM3" { 20 } else { 8 }));
        }
        if m.animatable {
            anim += 1;
            dmin = dmin.min(m.duration);
            dmax = dmax.max(m.duration);
        }
        let mut times: Vec<f32> = Vec::new();
        for p in [&m.saturation, &m.contrast_avg_lum, &m.contrast, &m.brightness] {
            times.extend(p.mult.0.iter().chain(&p.add.0).map(|k| k.0));
        }
        let r = &m.radial;
        for t in [&m.blur_radius, &m.double_vision, &r.strength, &r.ramp_up, &r.start, &r.ramp_down, &r.down_start, &m.dof.strength, &m.dof.distance, &m.dof.range, &m.motion_blur] {
            times.extend(t.0.iter().map(|k| k.0));
        }
        times.extend(m.tint.0.iter().chain(&m.fade.0).map(|k| k.0));
        if times.iter().any(|t| !(0.0..=1.0).contains(t)) {
            odd.push(format!("{} (keys to {})", m.edid, times.iter().copied().fold(0.0, f32::max)));
        }
        if m.animatable && m.duration <= 0.0 {
            odd.push(format!("{} (animatable, duration {})", m.edid, m.duration));
        }
        for t in times {
            tmin = tmin.min(t);
            tmax = tmax.max(t);
        }
        for u in uses(m) {
            *used.entry(u).or_insert(0usize) += 1;
        }
        *flags.entry((r.use_target, m.dof.use_target, m.dof.flags, r.center.map(|c| (c * 100.0) as i32))).or_insert(0usize) += 1;
    }
    println!("{} IMAD records; DNAM sizes {sizes:?}", all.len());
    println!("key counts in DNAM against subrecord sizes: {checked} checked, {bad} differ");
    println!("animatable {anim}; their durations {dmin} .. {dmax} s; key times {tmin} .. {tmax}");
    println!("key times outside 0..1, or animatable without a duration: {odd:?}");
    println!("records using each effect: {used:?}");
    println!("(radial use target, dof use target, dof flags, centre %): {flags:?}");
    let peak = |f: fn(&ImageModifier) -> f32| all.iter().map(|(m, _, _)| f(m)).fold(0.0, f32::max);
    println!(
        "largest: blur radius {} double vision {} radial strength {} dof strength {} motion blur {}",
        peak(|m| m.blur_radius.peak()),
        peak(|m| m.double_vision.peak()),
        peak(|m| m.radial.strength.peak()),
        peak(|m| m.dof.strength.peak()),
        peak(|m| m.motion_blur.peak())
    );
}
