//! `fnvtool setup` / `fnvtool paths`: NVForever's own home, kept apart from the game.

use crate::{Args, CmdError};
use fnv_install::{check_separation, original_user_dir, Config, EngineHome, GameInstall};
use fnv_json::Value;
use std::path::PathBuf;

fn home_from(args: &Args) -> Result<EngineHome, CmdError> {
    match args.value("home") {
        Some(h) => Ok(EngineHome::new(h)),
        None => EngineHome::default_root()
            .map(EngineHome::new)
            .ok_or_else(|| CmdError::Failed("cannot determine a home directory; pass --home or set NVFOREVER_HOME".into())),
    }
}

fn path_str(p: &std::path::Path) -> Value {
    Value::from(p.to_string_lossy().into_owned())
}

fn report(home: &EngineHome, game: Option<&GameInstall>) -> Value {
    let exe = std::env::current_exe().ok();
    let mut v = Value::object();
    v.set("schema", "nvforever.paths/1")
        .set("home", path_str(&home.root))
        .set("config_file", path_str(&home.config_file()))
        .set("saves", path_str(&home.saves()))
        .set("logs", path_str(&home.logs()))
        .set("cache", path_str(&home.cache()))
        .set("mods", path_str(&home.mods()))
        .set("executable", exe.as_deref().map(path_str).unwrap_or(Value::Null))
        .set("original_game_user_dir_never_touched", original_user_dir().as_deref().map(path_str).unwrap_or(Value::Null));
    match game {
        Some(g) => {
            let viol = check_separation(home, g, exe.as_deref());
            v.set("game_install", path_str(&g.root))
                .set("game_data_read_only", path_str(&g.data))
                .set("separated", viol.is_empty())
                .set("violations", Value::Array(viol.iter().map(|x| Value::from(x.0.clone())).collect()));
        }
        None => {
            v.set("game_install", Value::Null).set("separated", Value::Null);
        }
    }
    v
}

pub fn paths(rest: &[String]) -> Result<Value, CmdError> {
    let args = Args::parse(rest, &["home"]).map_err(CmdError::Usage)?;
    let home = home_from(&args)?;
    let cfg = Config::load(&home)?;
    let game = match &cfg.game_dir {
        Some(d) => Some(GameInstall::open(d)?),
        None => None,
    };
    Ok(report(&home, game.as_ref()))
}

pub fn setup(rest: &[String]) -> Result<Value, CmdError> {
    let args = Args::parse(rest, &["home", "game"]).map_err(CmdError::Usage)?;
    let home = home_from(&args)?;
    let game_dir = args.value("game").ok_or_else(|| CmdError::Usage("setup needs --game <Fallout New Vegas folder>".into()))?;
    let game = GameInstall::open(game_dir)?;
    let viol = check_separation(&home, &game, std::env::current_exe().ok().as_deref());
    if !viol.is_empty() {
        return Err(CmdError::Failed(fnv_install::InstallError::Overlap(viol).to_string()));
    }
    home.create()?;
    Config { game_dir: Some(PathBuf::from(&game.root)) }.save(&home, Some(&game))?;
    Ok(report(&home, Some(&game)))
}
