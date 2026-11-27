//! The live kernel held against nvf-rtgi on scenes made in code (#3009), on a GPU with ray queries; no game data.
//!
//!     cargo run --release -p nvf-rtgi-live --example rtgi-live-check -- [--validation] [--adapter <part of a name>]
//!
//! Each fixture's rays are held against the CPU's (`nvf_rtgi::bounces`) one by one: what they meet, the light they
//! bring and their lamps and shadow rays. Each point's hits, backs, lamps and shadow rays from the solve are held
//! against the sums of those answers, its reach (hits and backs) against `nvf_rtgi::classified`, and the triangle
//! test against `nvf_rtgi::meet`, all bit for bit but the light (within 5e-4). A difference that is known is
//! reported as KNOWN with its reason and decides nothing. Exit 0 when all the rest pass, 1 on a FAIL or an error,
//! 2 when no Vulkan adapter has ray queries. `--validation` turns the Vulkan validation layer on; any error it or
//! wgpu reports is a FAIL, and so is a layer that is not installed.

mod corpus;
mod fixtures;
mod gpu;

use std::process::ExitCode;
use std::sync::Mutex;

/// What wgpu and the validation layer say.
static MESSAGES: Mutex<Vec<(log::Level, String)>> = Mutex::new(Vec::new());

struct Log;
static LOG: Log = Log;

impl log::Log for Log {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Warn || m.target().starts_with("wgpu_hal::vulkan::instance")
    }

    fn log(&self, r: &log::Record) {
        // The instance's lines of debug level hold whether the layer was found, but also much else.
        let text = r.args().to_string();
        if self.enabled(r.metadata()) && (r.level() <= log::Level::Warn || text.contains("unable to find layer")) {
            if let Ok(mut m) = MESSAGES.lock() {
                m.push((r.level(), format!("{}: {text}", r.target())));
            }
        }
    }

    fn flush(&self) {}
}

/// Whether nothing was said that is an error, and the layer was there when it was asked for.
fn quiet(validate: bool) -> bool {
    let messages = MESSAGES.lock().map(|m| m.clone()).unwrap_or_default();
    for (level, text) in &messages {
        println!("{level} {text}");
    }
    let missing = messages.iter().any(|m| m.1.contains("unable to find layer"));
    let errors = messages.iter().filter(|m| m.0 == log::Level::Error).count();
    if validate {
        println!("VALIDATION: {}; {errors} errors", if missing { "the layer is NOT installed, nothing was validated" } else { "the layer was on" });
    }
    errors == 0 && !(validate && missing)
}

fn main() -> ExitCode {
    if log::set_logger(&LOG).is_ok() {
        log::set_max_level(log::LevelFilter::Debug);
    }
    let (mut validate, mut name, mut args) = (false, String::new(), std::env::args().skip(1));
    while let Some(a) = args.next() {
        match (a.as_str(), args.len()) {
            ("--validation", _) => validate = true,
            ("--adapter", 1..) => name = args.next().unwrap_or_default(),
            _ => {
                eprintln!("rtgi-live-check [--validation] [--adapter <part of a name>]");
                return ExitCode::FAILURE;
            }
        }
    }
    let gpu = match gpu::Gpu::open(&name, validate) {
        Ok(gpu) => gpu,
        Err(gpu::Refused::Unsupported(why)) => {
            println!("{why}");
            return ExitCode::from(2);
        }
        Err(gpu::Refused::Failed(why)) => {
            println!("FAIL: {why}");
            return ExitCode::FAILURE;
        }
    };
    println!("{}", gpu.describe());
    let passed = fixtures::run(&gpu).unwrap_or_else(|e| {
        println!("FAIL: {e}");
        false
    });
    let sound = gpu.sound().map_err(|e| println!("FAIL: {e}")).is_ok();
    drop(gpu);
    let quiet = quiet(validate);
    let all = passed && sound && quiet;
    println!("check: {}", if all { "PASS" } else { "FAIL" });
    if all {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
