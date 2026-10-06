//! fnvtool — the parity lab's candidate CLI. Every command prints one JSON
//! document on stdout (schemas in docs/cli-contract.md) and diagnostics on stderr.
//! Exit codes: 0 success, 1 the input could not be processed, 2 usage error.

mod bsa_cmd;
mod esm_cmd;
mod install_cmd;

use fnv_json::Value;
use std::process::ExitCode;

const USAGE: &str = "\
fnvtool — FNV engine reimplementation, inspection CLI

USAGE:
  fnvtool setup --game <Fallout New Vegas folder> [--home <dir>]
  fnvtool paths [--home <dir>]
  fnvtool bsa list <archive.bsa> [--no-content-hash]
  fnvtool bsa extract <archive.bsa> <path-in-archive> <out-file>
  fnvtool bsa verify <archive.bsa>
  fnvtool esm header <plugin>
  fnvtool esm stats <plugin>
  fnvtool esm inventory <plugin>
  fnvtool esm verify <plugin>
  fnvtool esm coverage <plugin> --schema <dir>
  fnvtool esm decode <plugin> --schema <dir> (--formid <hex> | --edid <EditorID>)
  fnvtool esm loadorder <data-dir> <plugin>... [--schema <dir>]
  fnvtool esm survey <data-dir> <plugin>... [--plugin <name>]
";

pub struct Args {
    pub positional: Vec<String>,
    pub opts: Vec<(String, Option<String>)>,
}

impl Args {
    pub fn parse(raw: &[String], value_opts: &[&str]) -> Result<Args, String> {
        let mut positional = Vec::new();
        let mut opts = Vec::new();
        let mut i = 0;
        while i < raw.len() {
            let a = &raw[i];
            if let Some(name) = a.strip_prefix("--") {
                if value_opts.contains(&name) {
                    let v = raw.get(i + 1).ok_or_else(|| format!("--{name} needs a value"))?;
                    opts.push((name.to_string(), Some(v.clone())));
                    i += 2;
                    continue;
                }
                opts.push((name.to_string(), None));
            } else {
                positional.push(a.clone());
            }
            i += 1;
        }
        Ok(Args { positional, opts })
    }
    pub fn flag(&self, name: &str) -> bool {
        self.opts.iter().any(|(k, _)| k == name)
    }
    pub fn value(&self, name: &str) -> Option<&str> {
        self.opts.iter().find(|(k, _)| k == name).and_then(|(_, v)| v.as_deref())
    }
}

pub enum CmdError {
    Usage(String),
    Failed(String),
}

impl<E: std::fmt::Display> From<E> for CmdError {
    fn from(e: E) -> Self {
        CmdError::Failed(e.to_string())
    }
}

pub fn emit(v: &Value) {
    use std::io::Write;
    let s = v.to_compact();
    let stdout = std::io::stdout();
    let mut lock = stdout.lock();
    let _ = lock.write_all(s.as_bytes());
    let _ = lock.write_all(b"\n");
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let single = matches!(argv.first().map(String::as_str), Some("paths") | Some("setup"));
    if argv.is_empty() || (argv.len() < 2 && !single) || argv[0] == "--help" || argv[0] == "-h" {
        eprint!("{USAGE}");
        return ExitCode::from(2);
    }
    let res = match argv[0].as_str() {
        "setup" => install_cmd::setup(&argv[1..]),
        "paths" => install_cmd::paths(&argv[1..]),
        "bsa" => bsa_cmd::run(&argv[1], &argv[2..]),
        "esm" => esm_cmd::run(&argv[1], &argv[2..]),
        other => Err(CmdError::Usage(format!("unknown command group {other:?}"))),
    };
    match res {
        Ok(v) => {
            emit(&v);
            ExitCode::SUCCESS
        }
        Err(CmdError::Usage(m)) => {
            eprintln!("usage error: {m}\n\n{USAGE}");
            ExitCode::from(2)
        }
        Err(CmdError::Failed(m)) => {
            eprintln!("error: {m}");
            ExitCode::from(1)
        }
    }
}
