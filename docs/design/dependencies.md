# Dependency plan: using the Rust ecosystem fully

Owner decision (2026-10-06): use Rust "to the full extent, external libraries and
everything". The first code was std-only only because its build sandbox could not reach
crates.io; that restriction was lifted the same day.

Migration happened crate by crate, each step behind the existing tests and an old-vs-new
comparison of `fnvtool` output on synthetic plugins and archives (strict: key order,
int-vs-float type, values, exit codes). The parity lab needs the owner's game data, so it
still has to be re-run there (84/84 Layer 1 scenarios expected).

## Layer 0/1: done

| Was | Now | Notes |
|---|---|---|
| `fnv-json` | `serde` + `serde_json` (`preserve_order`) | documents built with `json!`; `nvforever.json` is a derived struct; schema files keep duplicate-key rejection via a strict deserializer. Floats >= 1e16 are now written as floats (fnv-json printed them as integer tokens). |
| `fnv-inflate` decoder | `flate2`, **zlib-rs** backend | our zlib header + Adler-32 trailer diagnostics kept (`adler2`). miniz_oxide was rejected: in streaming mode it accepts back-references before the output start. |
| `fnv-util::sha256` | `sha2` (+ `hex`) | |
| `fnv-util::cp1252` | `encoding_rs` (WINDOWS_1252) | |
| hand-rolled CLI parsing | `clap` (derive) | `--help` everywhere; exits 0 for help, 2 for usage errors |
| ad-hoc error enums | `thiserror` (libs) + `anyhow` (fnvtool) | messages unchanged (they appear in JSON) |
| `std::fs::read` of masters | `memmap2` + `bytes::Bytes` | records are zero-copy slices; about half the peak memory |
| sequential scans | `rayon` | per-record/per-file work in parallel, folded in file order so output is identical |
| manual home-dir logic | `directories` | Windows known folders; a redirected Documents folder is followed |
| — | `tracing` + `tracing-subscriber` | debug events for data accepted without being reported; `NVFOREVER_LOG=debug` |
| — | `proptest` | malformed-input properties for every parser; `cargo-fuzz` (nightly) not set up |
| — | `cargo-deny` | `deny.toml`: licences, RustSec advisories, sources; runs in CI |

## Engine layers (candidates, to evaluate when each layer starts)

| Layer | Need | Candidates |
|---|---|---|
| 2 | NIF parsing | **done**: own schema-as-data interpreter (`schemas/nif`, re-expressed from nif.xml as documentation); `quick-xml` codegen rejected to keep the licence open |
| 2 | DDS / BCn textures | **done**: own header/layout parser (`fnv-dds`); GPU upload as BCn directly; CPU BCn decoders deferred (ADR-0001) |
| 2 | math, viewer | **done**: Bevy 0.19.1 (its `glam`), viewer in its own workspace (ADR-0001) |
| 3 | rendering | `wgpu` (Vulkan/Metal/DX12), `winit` for windows/input |
| 4 | physics / character controller | **chosen**: `rapier3d` 0.36 with `enhanced-determinism` (pure Rust, Apache-2.0, no Bevy coupling; Jolt bindings rejected for the C++ build and FFI, `avian3d` for its Bevy coupling; layer4.md §2.8). Used by `fnv-phys` only, which runs rapier in metres internally and exposes game units; terrain as triangle meshes (rapier's heightfield missed rays along grid lines). Pinned to `=0.36.0` (traces are compared bit for bit; licence Apache-2.0, its new dependencies `glamx`/`glam`/`parry3d` MIT/Apache-2.0, within deny.toml). docs/spec-notes/physics.md |
| 4 | entity model | **decided**: plain ordered maps in `fnv-sim`, no ECS in the core (layer4.md §2.2); `rand_chacha` for the seeded, reproducible RNG |
| 5 | script runtime | in-house bytecode interpreter (no off-the-shelf fit) |
| 7 | audio decode / playback | `symphonia` (WAV/MP3/OGG), `kira` or `cpal` |
| 7 | UI (Bethesda XML menus) | `quick-xml` + own layout engine; `egui` for dev tools |
| video | Bink intro | FFmpeg bindings (optional) or skip |

## Licence check

All dependencies are MIT/Apache-2.0 or similar permissive licences, plus one MPL-2.0 crate
(`option-ext`, via `directories`), compatible with GPL-3.0 and with a permissive project
licence. `cargo deny check` enforces the allow-list in `deny.toml` and RustSec advisories;
CI runs it on every push.

## CI

`.github/workflows/ci.yml`: tests and clippy (`-D warnings`) on Linux, Windows and macOS,
the Python lab/capture self-tests, `cargo deny`, and a Windows `fnvtool.exe` artifact
(`fnvtool-windows-x64`). The parity job that needs game data stays in
`parity-lab/ci/github-actions.yml` (self-hosted runner only).
