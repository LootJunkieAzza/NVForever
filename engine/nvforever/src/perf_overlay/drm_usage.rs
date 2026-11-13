//! [design, #2249] Pure DRM client sampling, used by `usage::linux` and platform-independent tests.
//! [documented] Fields, units, client deduplication, capacities and counter high-water rule:
//! https://docs.kernel.org/gpu/drm-usage-stats.html .

use std::collections::HashMap;
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Engine {
    device: String,
    client: u64,
    name: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Busy {
    ns: u64,
    capacity: u64,
}

// [design, #2249] Refuse ambiguous clients and malformed telemetry rather than inventing values.
fn parse(text: &str) -> Option<HashMap<Engine, Busy>> {
    let fields: HashMap<_, _> = text.lines().filter_map(|line| line.split_once(':')).map(|(k, v)| (k, v.trim())).collect();
    let driver = fields.get("drm-driver")?;
    if driver.is_empty() {
        return None;
    }
    let client = fields.get("drm-client-id")?.parse().ok()?;
    // [design, #2249] Without a device identity, keep globally unique clients separate
    // when choosing an engine: their work might belong to different devices.
    let device = fields.get("drm-pdev").map_or_else(|| format!("{driver}/client-{client}"), |pdev| format!("{driver}/{pdev}"));
    let mut engines = HashMap::new();
    for (&key, &value) in &fields {
        let Some(name) = key.strip_prefix("drm-engine-") else {
            continue;
        };
        if name.starts_with("capacity-") {
            continue;
        }
        if name.is_empty() || name.chars().any(char::is_whitespace) {
            return None;
        }
        let mut parts = value.split_whitespace();
        let ns = parts.next()?.parse().ok()?;
        if parts.next()? != "ns" || parts.next().is_some() {
            return None;
        }
        let capacity = match fields.get(format!("drm-engine-capacity-{name}").as_str()) {
            Some(v) => v.parse::<u64>().ok().filter(|&c| c > 0)?,
            None => 1,
        };
        engines.insert(Engine { device: device.clone(), client, name: name.to_string() }, Busy { ns, capacity });
    }
    (!engines.is_empty()).then_some(engines)
}

#[derive(Default)]
pub(super) struct DrmUsage {
    previous: HashMap<Engine, Busy>,
    at: Option<Duration>,
}

impl DrmUsage {
    pub(super) fn supports(text: &str) -> bool {
        parse(text).is_some()
    }
    /// [design, #2249] Sum distinct clients per device/engine, normalize grouped engines
    /// by capacity, then show the busiest engine. New or changed engines need a pair.
    pub(super) fn sample<'a>(&mut self, texts: impl IntoIterator<Item = &'a str>, at: Duration) -> Option<f32> {
        let mut current: HashMap<Engine, Busy> = HashMap::new();
        for (engine, busy) in texts.into_iter().filter_map(parse).flatten() {
            // [design] Duplicate FDs can be read at different instants: keep the larger sample.
            current.entry(engine).and_modify(|v| v.ns = v.ns.max(busy.ns)).or_insert(busy);
        }
        let elapsed = self.at.and_then(|before| at.checked_sub(before)).filter(|d| !d.is_zero());
        if self.at.is_some() && elapsed.is_none() {
            return None;
        }
        let mut per_engine: HashMap<(&str, &str), f64> = HashMap::new();
        for (engine, busy) in &mut current {
            let Some(before) = self.previous.get(engine).filter(|b| b.capacity == busy.capacity) else {
                continue;
            };
            let ns = busy.ns.max(before.ns);
            if let Some(elapsed) = elapsed {
                let percent = 100.0 * (ns - before.ns) as f64 / elapsed.as_nanos() as f64 / busy.capacity as f64;
                *per_engine.entry((&engine.device, &engine.name)).or_default() += percent;
            }
            busy.ns = ns;
        }
        let value = per_engine.into_values().reduce(f64::max).map(|v| v.clamp(0.0, 100.0) as f32);
        self.previous = current;
        self.at = Some(at);
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // [design] Synthetic host telemetry; no game data.
    fn info(client: u64, device: &str, engines: &str) -> String {
        format!("pos:\t0\ndrm-driver:\tfixture\ndrm-pdev:\t{device}\ndrm-client-id:\t{client}\n{engines}")
    }

    #[test]
    fn parses_only_busy_nanoseconds_and_capacity() {
        let text = info(7, "0000:01:00.0", "drm-engine-render: 125 ns\ndrm-engine-capacity-render: 2\ndrm-engine-copy: 9 ns\ndrm-memory-vram: 8192 KiB\n");
        let engines = parse(&text).unwrap();
        assert!(DrmUsage::supports(&text));
        assert!(!DrmUsage::supports("pos: 0"));
        assert_eq!(engines.len(), 2);
        assert_eq!(engines.iter().find(|(e, _)| e.name == "render").unwrap().1, &Busy { ns: 125, capacity: 2 });
        for bad in [
            "drm-engine-render: nope ns",
            "drm-engine-render: 1 ms",
            "drm-engine-render: 1",
            "drm-engine-render: -1 ns",
            "drm-engine-render: 18446744073709551616 ns",
            "drm-engine-render: 1 ns extra",
            "drm-engine-render: 1 ns\ndrm-engine-capacity-render: 0",
        ] {
            assert!(parse(&info(7, "a", bad)).is_none(), "{bad}");
        }
        assert!(parse("drm-driver: fixture\ndrm-engine-render: 1 ns").is_none());
        assert!(parse("pos: 0\n").is_none());
    }

    #[test]
    fn sums_clients_deduplicates_fds_and_chooses_busiest_device_engine() {
        let mut sampler = DrmUsage::default();
        let baseline = [info(1, "a", "drm-engine-render: 0 ns\ndrm-engine-copy: 0 ns"), info(2, "a", "drm-engine-render: 0 ns"), info(1, "b", "drm-engine-render: 0 ns")];
        assert_eq!(sampler.sample(baseline.iter().map(String::as_str), Duration::ZERO), None);
        let now = [info(1, "a", "drm-engine-render: 300000000 ns\ndrm-engine-copy: 500000000 ns"), info(2, "a", "drm-engine-render: 400000000 ns"), info(1, "b", "drm-engine-render: 600000000 ns")];
        assert_eq!(sampler.sample(now.iter().chain([&now[0]]).map(String::as_str), Duration::from_secs(1)), Some(70.0));
    }

    #[test]
    fn capacity_and_elapsed_time_normalize_busy_time_and_percent_is_bounded() {
        let mut sampler = DrmUsage::default();
        let text = |ns| info(1, "a", &format!("drm-engine-render: {ns} ns\ndrm-engine-capacity-render: 2"));
        assert_eq!(sampler.sample([text(0).as_str()], Duration::ZERO), None);
        assert_eq!(sampler.sample([text(1000000000).as_str()], Duration::from_secs(2)), Some(25.0));
        assert_eq!(sampler.sample([text(9000000000u64).as_str()], Duration::from_secs(3)), Some(100.0));
        assert_eq!(sampler.sample([text(9000000000u64).as_str()], Duration::from_secs(4)), Some(0.0));
    }

    #[test]
    fn counters_keep_the_high_water_mark_and_new_clients_need_a_pair() {
        let mut sampler = DrmUsage::default();
        let text = |client, ns| info(client, "a", &format!("drm-engine-render: {ns} ns"));
        sampler.sample([text(1, 1000000000).as_str()], Duration::ZERO);
        assert_eq!(sampler.sample([text(1, 900000000).as_str()], Duration::from_secs(1)), Some(0.0));
        assert_eq!(sampler.sample([text(1, 1100000000).as_str()], Duration::from_secs(2)), Some(10.0));
        assert_eq!(sampler.sample([text(2, 8000000000u64).as_str()], Duration::from_secs(3)), None);
        assert_eq!(sampler.sample([text(2, 8200000000u64).as_str()], Duration::from_secs(4)), Some(20.0));
        assert_eq!(sampler.sample([], Duration::from_secs(5)), None);
        assert_eq!(sampler.sample([text(2, 8300000000u64).as_str()], Duration::from_secs(6)), None);
    }

    #[test]
    fn a_capacity_change_needs_a_new_pair_and_clients_can_omit_pci_identity() {
        let mut sampler = DrmUsage::default();
        let text = |ns, capacity| format!("drm-driver: fixture\ndrm-client-id: 4\ndrm-engine-render: {ns} ns\ndrm-engine-capacity-render: {capacity}");
        assert_eq!(sampler.sample([text(0, 1).as_str()], Duration::ZERO), None);
        assert_eq!(sampler.sample([text(500000000, 2).as_str()], Duration::from_secs(1)), None);
        assert_eq!(sampler.sample([text(1000000000, 2).as_str()], Duration::from_secs(2)), Some(25.0));
    }

    #[test]
    fn clients_without_device_identity_are_not_summed_across_unknown_devices() {
        let mut sampler = DrmUsage::default();
        let text = |client, ns| format!("drm-driver: fixture\ndrm-client-id: {client}\ndrm-engine-render: {ns} ns");
        sampler.sample([text(1, 0).as_str(), text(2, 0).as_str()], Duration::ZERO);
        assert_eq!(sampler.sample([text(1, 400000000).as_str(), text(2, 600000000).as_str()], Duration::from_secs(1)), Some(60.0));
    }

    #[test]
    fn invalid_intervals_do_not_consume_the_baseline_and_reset_starts_afresh() {
        let mut sampler = DrmUsage::default();
        let text = |ns| info(1, "a", &format!("drm-engine-render: {ns} ns"));
        sampler.sample([text(0).as_str()], Duration::from_secs(2));
        assert_eq!(sampler.sample([text(100).as_str()], Duration::from_secs(2)), None);
        assert_eq!(sampler.sample([text(100).as_str()], Duration::from_secs(1)), None);
        assert_eq!(sampler.sample([text(500000000).as_str()], Duration::from_secs(3)), Some(50.0));
        sampler = DrmUsage::default();
        assert_eq!(sampler.sample([text(600000000).as_str()], Duration::from_secs(4)), None);
    }
}
