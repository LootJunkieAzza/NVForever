//! fnvtool — the parity lab's candidate CLI. Every command prints one JSON
//! document on stdout (schemas in docs/cli-contract.md) and diagnostics on stderr.
//! Exit codes: 0 success, 1 the input could not be processed, 2 usage error.

mod bsa_cmd;
mod collide_cmd;
mod dds_cmd;
mod esm_cmd;
mod install_cmd;
mod nif_cmd;
mod phys_cmd;
mod source;
mod world_cmd;

use clap::{Args, Parser, Subcommand};
use serde_json::Value;
use std::path::PathBuf;
use std::process::ExitCode;

/// NVForever (FNV engine reimplementation) inspection CLI.
#[derive(Parser)]
#[command(name = "fnvtool", version)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Record the game install (read-only) and create NVForever's own home.
    Setup {
        /// The Fallout New Vegas folder (or its Data folder).
        #[arg(long)]
        game: PathBuf,
        #[command(flatten)]
        home: HomeArg,
    },
    /// Show where NVForever keeps its files.
    Paths {
        #[command(flatten)]
        home: HomeArg,
    },
    /// BSA archives.
    #[command(subcommand)]
    Bsa(BsaCmd),
    /// ESM/ESP plugins.
    #[command(subcommand)]
    Esm(EsmCmd),
    /// NIF meshes and KF animations.
    #[command(subcommand)]
    Nif(NifCmd),
    /// DDS textures.
    #[command(subcommand)]
    Dds(DdsCmd),
    /// Worldspaces, cells, placed references and landscape across a load order.
    #[command(subcommand)]
    World(WorldCmd),
    /// Physics world built from the game data: terrain, collision, probe characters.
    #[command(subcommand)]
    Phys(PhysCmd),
    /// Layer 4 collision geometry: Havok collision blocks of meshes.
    #[command(subcommand)]
    Collide(CollideCmd),
}

#[derive(Subcommand)]
pub enum PhysCmd {
    /// Build the static world around a cell and drop probe characters (fnv.phys.cell/1).
    Cell {
        #[command(flatten)]
        w: WorldArgs,
        /// Cell Editor ID or load-order FormID.
        #[arg(long)]
        cell: Option<String>,
        /// Worldspace Editor ID or FormID, with --x and --y.
        #[arg(long)]
        world: Option<String>,
        #[arg(long, allow_hyphen_values = true)]
        x: Option<i32>,
        #[arg(long, allow_hyphen_values = true)]
        y: Option<i32>,
        /// Exterior cells loaded around the centre cell, in grid squares.
        #[arg(long, default_value_t = 1)]
        radius: i32,
        /// Drop point "x,y" or "x,y,z" in world units (repeatable; default: the centre
        /// of the cell, 100 units above the ground).
        #[arg(long = "drop", allow_hyphen_values = true)]
        drops: Vec<String>,
        /// Simulated seconds per probe.
        #[arg(long, default_value_t = 5.0)]
        seconds: f32,
        /// NIF schema directory for mesh collision (default: `nif` next to --schema).
        #[arg(long)]
        nif_schema: Option<PathBuf>,
        /// Leave mesh collision out (terrain and collision primitives only).
        #[arg(long)]
        no_meshes: bool,
    },
    /// A scripted character walk through the world built around its start, traced step
    /// by step (fnv.phys.walk/1).
    Walk {
        #[command(flatten)]
        w: WorldArgs,
        /// Walk script (JSON, `fnv.phys.walk-script/1`, physics.md §7).
        #[arg(long)]
        script: PathBuf,
        /// Interior cell to walk in (Editor ID or FormID).
        #[arg(long)]
        cell: Option<String>,
        /// Worldspace to walk in (Editor ID or FormID); the cells around the start load.
        #[arg(long)]
        world: Option<String>,
        /// Exterior cells loaded around the start cell, in grid squares.
        #[arg(long, default_value_t = 1)]
        radius: i32,
        /// Record every Nth step (the last step is always recorded).
        #[arg(long, default_value_t = 1)]
        every: usize,
        /// NIF schema directory for mesh collision (default: `nif` next to --schema).
        #[arg(long)]
        nif_schema: Option<PathBuf>,
        /// Leave mesh collision out (terrain and collision primitives only).
        #[arg(long)]
        no_meshes: bool,
    },
    /// Terrain height under fixed sample points of every landscape cell of a worldspace,
    /// found by vertical rays against the physics terrain (fnv.phys.heights/1).
    Heights {
        #[command(flatten)]
        w: WorldArgs,
        #[arg(long)]
        world: String,
    },
    /// Vertical rays on every landscape cell of a worldspace against the analytic
    /// terrain surface (fnv.phys.terrain/1).
    Terrain {
        #[command(flatten)]
        w: WorldArgs,
        #[arg(long)]
        world: String,
        /// Scattered sample points per cell (grid vertices and lines are added).
        #[arg(long, default_value_t = 64)]
        samples: usize,
    },
}

#[derive(Subcommand)]
pub enum CollideCmd {
    /// Statistics of every collision object and the measured stored-to-game scale
    /// (fnv.collide.survey/1).
    Survey {
        /// BSA archives, loose .nif files or directories.
        #[arg(required = true)]
        sources: Vec<PathBuf>,
        #[arg(long)]
        schema: PathBuf,
        /// Only files whose path contains this text (case-insensitive).
        #[arg(long)]
        filter: Option<String>,
    },    /// Every solid collision object of every mesh in an archive, in game units
    /// (fnv.collide.objects/1).
    Objects {
        archive: PathBuf,
        #[arg(long)]
        schema: PathBuf,
        /// Only files whose path contains this text (case-insensitive).
        #[arg(long)]
        filter: Option<String>,
    },
}

#[derive(Args)]
pub struct WorldArgs {
    #[command(flatten)]
    pub lo: LoadOrderArgs,
    #[arg(long)]
    pub schema: PathBuf,
}

#[derive(Args)]
pub struct GridArgs {
    /// Worldspace Editor ID or load-order FormID.
    #[arg(long)]
    pub world: String,
    #[arg(long, allow_hyphen_values = true)]
    pub x: i32,
    #[arg(long, allow_hyphen_values = true)]
    pub y: i32,
}

#[derive(Subcommand)]
pub enum WorldCmd {
    /// Every worldspace with cell and reference counts (fnv.world.list/1).
    List {
        #[command(flatten)]
        w: WorldArgs,
    },
    /// Cells of a worldspace, or all interior cells (fnv.world.cells/1).
    Cells {
        #[command(flatten)]
        w: WorldArgs,
        /// Worldspace Editor ID or FormID; omit for interior cells.
        #[arg(long)]
        world: Option<String>,
    },
    /// References placed in one cell, every cell of a worldspace, or every interior cell
    /// (fnv.world.refs/1).
    #[command(group(clap::ArgGroup::new("which").required(true).args(["cell", "world", "interiors"])))]
    Refs {
        #[command(flatten)]
        w: WorldArgs,
        /// Cell Editor ID or load-order FormID.
        #[arg(long)]
        cell: Option<String>,
        /// Every cell of this worldspace (its persistent cell included); with --x and
        /// --y only the exterior cell at that grid position.
        #[arg(long)]
        world: Option<String>,
        /// Every interior cell.
        #[arg(long)]
        interiors: bool,
        #[arg(long, allow_hyphen_values = true)]
        x: Option<i32>,
        #[arg(long, allow_hyphen_values = true)]
        y: Option<i32>,
    },
    /// Landscape of one exterior cell (fnv.world.land/1).
    Land {
        #[command(flatten)]
        w: WorldArgs,
        #[command(flatten)]
        g: GridArgs,
        /// Also normals, vertex colours and texture layers.
        #[arg(long)]
        full: bool,
    },
    /// Height summary of every landscape cell in a worldspace (fnv.world.heights/1).
    Heights {
        #[command(flatten)]
        w: WorldArgs,
        #[arg(long)]
        world: String,
    },
    /// Observational checks of the world conventions (fnv.world.check/1).
    Check {
        #[command(flatten)]
        w: WorldArgs,
        /// Worldspace to check (default: every worldspace).
        #[arg(long)]
        world: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum DdsCmd {
    /// Header, format and mip layout of every .dds in an archive (fnv.dds.listing/1).
    List { archive: PathBuf },
}

#[derive(Args)]
pub struct HomeArg {
    /// NVForever home directory (default: NVFOREVER_HOME or the platform data dir).
    #[arg(long)]
    pub home: Option<PathBuf>,
}

#[derive(Subcommand)]
pub enum BsaCmd {
    /// List every file with hashes, sizes and (by default) a content SHA-256.
    List {
        archive: PathBuf,
        /// Skip decoding files (no size/sha256 columns).
        #[arg(long)]
        no_content_hash: bool,
    },
    /// Extract one file.
    Extract {
        archive: PathBuf,
        /// Path inside the archive (case-insensitive, / or \).
        path_in_archive: String,
        out_file: PathBuf,
    },
    /// Byte-identical rewrite check plus decode of every file.
    Verify { archive: PathBuf },
}

#[derive(Subcommand)]
pub enum EsmCmd {
    /// TES4 header.
    Header { plugin: PathBuf },
    /// Record/group counts.
    Stats { plugin: PathBuf },
    /// Every record with FormID, type, EDID and FULL.
    Inventory { plugin: PathBuf },
    /// Byte-identical rewrite and per-record payload checks.
    Verify { plugin: PathBuf },
    /// Schema coverage report (work queue of undecoded subrecords).
    Coverage {
        plugin: PathBuf,
        #[arg(long)]
        schema: PathBuf,
        #[arg(long, default_value_t = 200)]
        max_rows: usize,
    },
    /// Decode records with the schema, selected by FormID or Editor ID.
    #[command(group(clap::ArgGroup::new("select").required(true).args(["formid", "edid"])))]
    Decode {
        plugin: PathBuf,
        #[arg(long)]
        schema: PathBuf,
        /// File-local FormID in hex, with or without 0x.
        #[arg(long, value_parser = parse_formid)]
        formid: Option<u32>,
        /// Editor ID (case-insensitive).
        #[arg(long)]
        edid: Option<String>,
    },
    /// Load-order resolution report.
    Loadorder {
        #[command(flatten)]
        lo: LoadOrderArgs,
        /// Also resolve every FormID-typed field the schema knows about.
        #[arg(long)]
        schema: Option<PathBuf>,
    },
    /// Observational statistics per (record type, subrecord) for schema work.
    Survey {
        #[command(flatten)]
        lo: LoadOrderArgs,
        /// Only survey this plugin (case-insensitive name).
        #[arg(long)]
        plugin: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum NifCmd {
    /// Schema coverage over many files: per block type, how many decode exactly and
    /// re-encode byte-identically (the Layer 2 work queue).
    Coverage {
        /// BSA archives, loose .nif/.kf files or directories.
        #[arg(required = true)]
        sources: Vec<PathBuf>,
        #[arg(long)]
        schema: PathBuf,
        /// Only files whose path contains this text (case-insensitive).
        #[arg(long)]
        filter: Option<String>,
        #[arg(long, default_value_t = 200)]
        max_rows: usize,
        /// Skip the re-encode check (faster).
        #[arg(long)]
        no_reencode: bool,
    },
    /// Per-file header summary of every .nif/.kf in an archive (fnv.nif.stats/1).
    Stats { archive: PathBuf },
    /// Scene objects and geometry summaries of every .nif/.kf in an archive (fnv.nif.scene/1).
    Scene {
        archive: PathBuf,
        #[arg(long)]
        schema: PathBuf,
    },
    /// Animation sequences of every .kf/.nif in an archive, with sanity statistics
    /// for the evaluated tracks (fnv.nif.anim/1).
    Anim {
        archive: PathBuf,
        #[arg(long)]
        schema: PathBuf,
        /// Only files whose path contains this text.
        #[arg(long)]
        filter: Option<String>,
    },
    /// Every texture path referenced by every mesh in the virtual file system (default
    /// archive order + loose files), and whether it resolves (fnv.nif.textures/1).
    Textures {
        data_dir: PathBuf,
        #[arg(long)]
        schema: PathBuf,
        #[arg(long, default_value_t = 200)]
        max_rows: usize,
    },
    /// Decode one file's blocks with the schema.
    Decode {
        /// A loose file, or a BSA archive together with --path.
        source: PathBuf,
        /// Path inside the archive.
        #[arg(long)]
        path: Option<String>,
        #[arg(long)]
        schema: PathBuf,
        /// Only this block index.
        #[arg(long)]
        block: Option<usize>,
    },
}

#[derive(Args)]
pub struct LoadOrderArgs {
    pub data_dir: PathBuf,
    /// Plugins in load order.
    #[arg(required = true)]
    pub plugins: Vec<String>,
}

fn parse_formid(s: &str) -> Result<u32, String> {
    let hex = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    u32::from_str_radix(hex, 16).map_err(|_| format!("not a hex FormID: {s}"))
}

fn emit(v: &Value) {
    use std::io::Write;
    let mut lock = std::io::stdout().lock();
    let _ = lock.write_all(v.to_string().as_bytes());
    let _ = lock.write_all(b"\n");
}

/// Diagnostics go to stderr. Default level is `warn`; set `NVFOREVER_LOG` (an
/// `EnvFilter` directive such as `debug` or `fnv_esm=trace`) for more.
fn init_logging() {
    use std::io::IsTerminal;
    let filter = tracing_subscriber::EnvFilter::try_from_env("NVFOREVER_LOG")
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .init();
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    init_logging();
    let res = match cli.cmd {
        Cmd::Setup { game, home } => install_cmd::setup(&game, &home),
        Cmd::Paths { home } => install_cmd::paths(&home),
        Cmd::Bsa(c) => bsa_cmd::run(c),
        Cmd::Esm(c) => esm_cmd::run(c),
        Cmd::Nif(c) => nif_cmd::run(c),
        Cmd::Dds(c) => dds_cmd::run(c),
        Cmd::World(c) => world_cmd::run(c),
        Cmd::Phys(c) => phys_cmd::run(c),
        Cmd::Collide(c) => collide_cmd::run(c),
    };
    match res {
        Ok(v) => {
            emit(&v);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_is_consistent() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }

    #[test]
    fn formid_forms() {
        assert_eq!(parse_formid("0x0001F4A2"), Ok(0x1F4A2));
        assert_eq!(parse_formid("0X1f4a2"), Ok(0x1F4A2));
        assert_eq!(parse_formid("1F4A2"), Ok(0x1F4A2));
        assert!(parse_formid("zz").is_err());
    }
}
