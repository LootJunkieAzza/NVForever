# Dependency plan: using the Rust ecosystem fully

Owner decision (2026-10-06): use Rust "to the full extent, external libraries and
everything". The current code is std-only **only** because the build sandbox's network
policy blocks crates.io; that restriction is being lifted, not adopted as a project rule.

Migration happens crate by crate, each step behind the existing tests and the parity lab
(84/84 Layer 1 scenarios must still pass, byte-identical round-trips included).

## Replace in-tree helpers (Layer 0/1)

| Today | Replace with | Notes |
|---|---|---|
| `fnv-json` | `serde` + `serde_json` (`preserve_order`) | derive structs for every CLI document; schemas become typed |
| `fnv-inflate` decoder | `flate2` (miniz_oxide or zlib-rs backend) | keep our trailer diagnostics: inflate the raw deflate body, then check Adler-32 ourselves (`adler2`) |
| `fnv-util::sha256` | `sha2` | |
| `fnv-util::cp1252` | `encoding_rs` (WINDOWS_1252) | |
| hand-rolled CLI parsing | `clap` (derive) | |
| ad-hoc error enums | `thiserror` (libs) + `anyhow` (bins) | |
| `std::fs::read` of 245 MB masters | `memmap2` | zero-copy records |
| sequential coverage/loadorder scans | `rayon` | per-record decode is embarrassingly parallel |
| manual home-dir logic in `fnv-install` | `directories` | same policy, platform-correct paths |
| — | `tracing` + `tracing-subscriber` | structured logging of unhandled data (spec section 7) |
| — | `proptest` / `cargo-fuzz` | fuzz parsers (the review's malformed-input cases) |

## Engine layers (candidates, to evaluate when each layer starts)

| Layer | Need | Candidates |
|---|---|---|
| 2 | NIF parsing driven by niftools `nif.xml` | `quick-xml` + build-script codegen |
| 2 | DDS / BCn textures | `ddsfile`, `texture2ddecoder` or `bcdec_rs`; GPU upload as BCn directly |
| 2 | math | `glam` |
| 3 | rendering | `wgpu` (Vulkan/Metal/DX12), `winit` for windows/input |
| 4 | physics / character controller | `rapier3d` or Jolt bindings |
| 4 | entity model | `hecs` or `bevy_ecs` |
| 5 | script runtime | in-house bytecode interpreter (no off-the-shelf fit) |
| 7 | audio decode / playback | `symphonia` (WAV/MP3/OGG), `kira` or `cpal` |
| 7 | UI (Bethesda XML menus) | `quick-xml` + own layout engine; `egui` for dev tools |
| video | Bink intro | FFmpeg bindings (optional) or skip |

## Licence check

All candidates above are MIT/Apache-2.0 (or Zlib/BSD), compatible with GPL-3.0 and with a
permissive project licence. `cargo-deny` will enforce licences and advisories in CI.

## Status

Blocked on network access to `crates.io`, `index.crates.io` and `static.crates.io` from
the build sandbox. Until then, CI on GitHub (which has registry access) is where
dependency-based builds can be verified.
