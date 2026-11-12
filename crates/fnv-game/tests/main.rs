//! fnv-game's integration tests, built as one test binary (`autotests = false` in Cargo.toml).
//! One binary instead of one per file: the crate and its dependencies are linked and
//! optimised once, not once per file. Each file stays a module of its own; run one with
//! `cargo test -p fnv-game --release --test integration <file>::`.
//! The files that change process-wide state (mod_files' roots, nvse_jg7's combat-music
//! flag) are test binaries of their own, listed in Cargo.toml: in one process they would
//! change each other's.

mod common;
#[path = "common/digest.rs"]
mod digest;

mod abilities;
mod actor_values;
mod aid;
mod appearance;
mod area_effects;
mod ash_piles;
mod build_identity;
mod build_shortcuts;
mod camera_paths;
mod caravan_data;
mod combat_blocks;
mod combat_lines_data;
mod combat_transitions;
mod companion_wheel;
mod crime;
mod crippled_limbs_data;
mod dead_on_spawn;
mod death_items;
mod dialogue_data;
mod dialogue;
mod difficulty;
mod effect_updates;
mod experience;
mod factions;
mod functions;
mod game_clock;
mod hit_effects_data;
mod hit_effects;
mod idles_holstered;
mod idles;
mod item_ref_health;
mod jg_nif;
mod jg10;
mod jg3;
mod jg4;
mod jg5;
mod jg6;
mod jg8;
mod jip10;
mod jip11;
mod jip12;
mod jip13;
mod jip14;
mod jip6;
mod jip7;
mod jip8;
mod jip9;
mod knockdowns;
mod lutana;
mod lutana2;
mod lutana3;
mod magic_archetypes;
mod npc_doors;
mod nvse_itr2;
mod one111;
mod package_choice;
mod packages;
mod perk_combat;
mod perk_talk;
mod pp;
mod radiation;
mod radio_data;
mod respawn;
mod save_pairs;
mod script_events;
mod script_stack;
mod showoff3;
mod showoff4;
mod showoff5;
mod step_budget;
mod sup1;
mod sup4;
mod sup6;
mod sup7;
mod sweep;
mod traps;
mod ui_functions;
mod ui_hud;
mod ui_loading;
mod ui_mcm;
mod ui_more;
mod unconscious;
mod vendor_services;
mod zones;
