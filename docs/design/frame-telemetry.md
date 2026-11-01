# Frame telemetry: JSONL schema 1

[hypothesis] This proposed contract for [#1276](https://github.com/LootJunkieAzza/NVForever/issues/1276) describes the design, not runtime results.

## 1. Enablement and output

[hypothesis] `NVF_PLAY_TEST_FRAME_TELEMETRY=<seconds>` requests one frames-only capture per process.
An unset value or zero disables it. Reject negative, nonnumeric and non-finite values; cap positive values
at the hard maximum of 60 real seconds. `NVF_PLAY_TEST_FRAME_TELEMETRY_GPU=1` additionally requests
render CPU and GPU observations; unset or zero leaves them off, and other values are errors.
The GPU aid alone does not enable capture. Neither aid requires tracing or the custom span collector.
They do not change display sync, frame caps or graphics settings. The fixed limits are 100,000 attempted
frame records and 67,108,864 bytes (64 MiB), including the header, trailer and line separators.

[documented] Home selection and logs follow `NVFOREVER_HOME` or platform defaults (`crates/fnv-install/src/lib.rs:162-184,209-210`).
The run-log directory handler guards and creates that directory (`engine/nvforever/src/crash.rs:54-62`).
The write guard refuses the game install and original user folder (`crates/fnv-install/src/lib.rs:352-371`).

[hypothesis] Reuse that directory handling and `fnv_install::guard_write`, guarding the final file against
the explicit game install. Create `<home>/logs/frame-telemetry-<capture_id>.jsonl` exclusively, outside
ordinary run-log rotation. Report home or write failures; never fall back to the current directory,
the game folder or `Documents\My Games\FalloutNV`. Pair the epoch clocks before the first update.
Duration includes startup. Producers check duration and frame limits before each payload and stop after
the Nth frame attempt. The writer enforces the byte budget before writing each line, reserving 16 KiB for
the trailer. If the next line would exceed that budget, it stops with `byte_limit` and cuts the interval
at the last wholly written record. Attempts with higher seq values are outside that interval and are not
exporter loss. A stalled update may delay finalization. Bounds stop capture without restarting it.
The first producer stop wins; simultaneous checks prefer duration, then frames. A writer byte cut may
shorten an already frozen interval. Writer errors override the stop, shutdown timeouts truncate, and the
game continues.

## 2. Timing and provenance

[documented] Here, `bevy_*` source paths refer to the pinned 0.20.1 crates. `TimePlugin` updates
`Time<Real>` in `First/TimeSystems` (`bevy_time/src/lib.rs:82-87`). Automatic time uses a received
renderer instant or falls back to `Instant::now()` (`:152-192`). The renderer sends an instant after its
render schedule attempt, even without rendering (`bevy_render/src/lib.rs:462-465,522-534`).
Real delta is the saturating difference between update instants; its first delta is zero and excludes
pre-first-update startup time (`bevy_time/src/real.rs:23-38,99-108`).
`FrameCount` is u32, starts at zero during Update, increments in Last and wraps; extraction copies it
into the render world (`bevy_diagnostic/src/frame_count.rs:10-39`; `bevy_render/src/globals.rs:16-37`).

[hypothesis] At the end of main `Update`, after settings and phase and marker producers, read
`Time<Real>::delta().as_nanos()`. It measures cadence between First's instants, including waits, rather than
Update execution or CPU or GPU time. First duration is null; later zeros are valid. Require automatic time
outside synthetic tests. Collection time is separate from interval endpoints.

[documented] Render diagnostics import in PreUpdate and timestamp that import, not GPU execution.
Private frame data and completion callbacks provide no source frame number, and mapping can remain
pending (`bevy_render/src/diagnostic/mod.rs:66-74,126-135`; `diagnostic/internal.rs:217-237,572-583,727-746`).
CPU values measure host begin/end instants; GPU values measure timestamp query intervals. Only explicit
instrumented scopes are covered (`diagnostic/internal.rs:603-639`; `diagnostic/mod.rs:43-58`).

[hypothesis] In GPU mode, collect after PreUpdate sync. Keep the last measurement time per path; export
only a newer `measurement()`, never `average()`, advancing cursors even on drops. Exclude pre-capture samples.
State unavailable origin and age once in the header. Import time cannot establish GPU staleness or
associate a result with this CPU frame. Never sum overlapping scopes into an additive breakdown.
Keep spans and process CPU in legacy output. Never wait for readbacks; history omissions and upstream losses are not exporter drops.

[documented] `engine/nvforever/build.rs:1-10` currently embeds the Windows icon and watches its source.
[hypothesis] Extend this build script to read `git rev-parse HEAD` and embed the commit through
`cargo:rustc-env=NVF_BUILD_GIT_SHA=...`, read with `option_env!`. If Git is absent, the checkout is not a
repository or the query fails, the field is null with a reason; provenance must never fail the build.
Keep the icon watch and add `rerun-if-changed` only for the Git-resolved `HEAD`, the branch reference it
names and `packed-refs` (worktree and common directory paths), so the script reruns when the commit
changes and not on every source or document edit, which would recompile the crate. The engine does not
record uncommitted changes: a build-time dirty flag would either go stale or force a rebuild on any edit.
The stress runner records its own checkout's commit and dirty state (#1277), which describe the
engine only when both come from the same checkout. The game process never runs Git.

## 3. Records

[hypothesis] Write compact UTF-8 JSONL, one object and LF per line, without a BOM or non-finite numbers.
Keep required fields and exact u64 integers; unavailable values are null. Put the header first and trailer
last. Ignore unknown fields; report unsupported versions and record types. Drop and count payloads over
8 KiB; lines are at most 16 KiB. Optional header strings over 1,024 UTF-8 bytes become null with a reason.

| Common field | JSON type | Unit; nullable | Meaning |
|---|---|---|---|
| `type` | string | None; no | Identifies `header`, `frame`, `diagnostic` or `trailer`. |
| `seq` | u64 | Records; no | Counts capture-wide record attempts from header 0, assigned before capacity checks and never reused. |
| `t_ns` | u64 | ns; no | Gives collection time since the monotonic epoch; the header uses zero and trailer uses stop time. |

| Header field | JSON type | Unit; nullable | Meaning |
|---|---|---|---|
| `schema_version` | u32 | None; no | Is exactly 1; incompatible semantics require a new version. |
| `capture_id` | string | None; no | Is a unique token matching `[A-Za-z0-9_-]{1,64}` and the filename. |
| `run_id`, `scenario_id` | string | None; yes | Carry opaque harness IDs from `NVF_PLAY_TEST_FRAME_TELEMETRY_RUN_ID` and `NVF_PLAY_TEST_FRAME_TELEMETRY_SCENARIO_ID`; each is at most 64 ASCII bytes. |
| `started_at_utc` | string | UTC; yes | Records the RFC3339 epoch clock pair, never the source for elapsed calculations. |
| `engine_revision` | string | None; yes | Gives the build-embedded commit, never an inference from the runner or executable name. |
| `build_features` | array of strings | None; no | Lists sorted enabled nvforever crate features from compile-time cfg, not transitive features. |
| `adapter`, `backend`, `driver`, `driver_info` | string | None; yes | Copy AdapterInfo's corresponding fields; empty values become null. |
| `width_px`, `height_px` | u32 | Physical pixels; yes | Give the primary window size at the header snapshot. |
| `display_sync` | string | None; yes | Gives applied FramePace sync: `off`, `vsync` or `variable`. |
| `cap_fps` | u32 | Frames/s; yes | Gives the applied effective cap; zero means uncapped. |
| `refresh_millihz` | u32 | mHz; yes | Gives reported monitor refresh, without promising actual display cadence. |
| `window_present_mode` | string | None; yes | Gives the applied Window PresentMode, including Auto modes; it is not the driver's concrete mode. |
| `unavailable` | object<string,string> | None; no | Maps unavailable header metadata fields to reasons, excluding the conditional diagnostics_reason field. |
| `max_duration_ns`, `max_frames`, `max_bytes` | u64 | ns, frames, bytes respectively; no | Give the clamped duration and fixed frame and byte limits. |
| `diagnostics` | string | None; no | Is `enabled`, `disabled` or `unavailable`; it does not promise GPU samples. |
| `diagnostics_reason` | string | None; yes | Is null when enabled, otherwise `not_requested` or `no_renderer`. |
| `gpu_origin` | string | None; no | Is `unavailable_from_bevy_0_20_1`: source render frame and GPU result age are unavailable for all observations. |
| `render_cpu_origin` | string | None; no | Is `source_frame_unavailable_from_bevy_0_20_1`: render CPU observations also lack source frame identity. |

[documented] AdapterInfo exposes these identity fields (`wgpu-types-30.0.1/src/adapter.rs:145-185`).
FramePace computes caps and applies modes (`engine/nvforever/src/frame_pace.rs:134-167`).
[hypothesis] Snapshot after initial settings application. Stop with `settings_changed` before admitting
a later frame with different resolution or presentation settings; the game and legacy output continue.

| Frame field | JSON type | Unit; nullable | Meaning |
|---|---|---|---|
| `main_frame` | u32 | Updates; no | Gives FrameCount observed in this Update, independently of seq. |
| `duration_ns` | u64 | ns; yes | Gives real delta at the §2 boundary; only the first update is null. |
| `duration_reason` | string | None; yes | Is `no_previous_time_update` for the null duration, otherwise null. |
| `phase` | string | None; no | Classifies this observation as `startup`, `loading` or `gameplay`. |
| `markers` | array of objects | None; no | Holds up to eight events observed since prior collection, in producer order; it may be empty. |

[hypothesis] Each marker has non-null `kind`, `id` and `t_ns`. Kind is a string chosen from `phase_enter`,
`scenario_begin`, `scenario_end`, `waypoint` and `event`. ID is an ASCII string of at most 64 bytes holding
the phase name, scenario ID or ordinal. Timestamp is u64 nanoseconds since epoch when emitted.
Markers may precede their frame and imply no cause. They contain no game data or copied log text.

[documented] Startup lasts through the sample seeing Streamer settled or absent, or its startup sum over
60 s; later hidden frames are excluded (`engine/nvforever/src/stress_sample.rs:251-268`;
`engine/nvf-play/src/loading.rs:170-173`). [hypothesis] Use that policy independently of the sample aid.
Startup takes priority; loading follows when Loading::unseen_frames is true. Gameplay includes the
remaining frames, including menus. Mark every phase entry.

| Diagnostic field | JSON type | Unit; nullable | Meaning |
|---|---|---|---|
| `path` | string | None; no | Preserves the Bevy render path, at most 1,024 UTF-8 bytes. |
| `unit` | string | None; no | Is `ms`, checked against the producer suffix. |
| `kind` | string | None; no | Is `render_cpu_elapsed` or `gpu_elapsed`, selected by path suffix. |
| `value` | number | ms; yes | Holds a new finite measurement, not an average; null denotes unavailable or invalid data. |
| `status` | string | None; no | Is `available`, `unavailable` or `invalid`. |
| `reason` | string | None; yes | Is null when available, otherwise `timestamp_queries_unsupported`, `no_sample_observed`, `non_finite_value` or `unit_mismatch`. |
| `observation_t_ns` | u64 | ns; no | Gives DiagnosticMeasurement.time relative to epoch, which is store import time; for absence it gives check time. |
| `observation_main_frame` | u32 | Updates; no | Gives FrameCount when the collector first observes the measurement or absence, without causal attribution. |

[hypothesis] GPU mode exports render paths ending in elapsed_cpu or elapsed_gpu. Without GPU samples,
emit a terminal `render/elapsed_gpu` sentinel, not a measured total. Check capability before reporting
unsupported, otherwise use no_sample_observed. Frames-only mode never exports diagnostics from other aids.

| Trailer field | JSON type | Unit; nullable | Meaning |
|---|---|---|---|
| `status` | string | None; no | Is `complete` when the capture stopped at a limit or a normal exit without exporter loss, otherwise `truncated`. |
| `stop_reason` | string | None; no | Is `duration_limit`, `frame_limit`, `byte_limit`, `normal_exit`, `settings_changed`, `writer_error` or `shutdown_timeout`. |
| `cut_seq` | u64 | Records; yes | Gives the last wholly written record's seq when the writer cuts at the byte limit; it is 0 for a header-only interval and null when there is no byte cut. |
| `frames_attempted`, `frames_written` | u64 | Frames; no | Count attempted and wholly written frame records, including null-duration records; a byte cut includes only attempts with seq at most cut_seq. |
| `dropped_frames`, `dropped_diagnostics`, `dropped_markers` | u64 | Records, records, markers respectively; no | Count unwritten record attempts, and markers beyond eight or carried by an unwritten frame; a byte cut includes only records and their frame-associated markers at or before cut_seq. |
| `gpu_observations_written` | u64 | Records; no | Counts written available GPU observations, excluding the sentinel. |
| `phases` | object | None; no | Contains exactly startup, loading and gameplay summary objects, including empty populations. |

[hypothesis] Each phase summary has non-null u64 fields `frames_written`, `samples`, `above_33_ms`,
`above_50_ms`, `above_100_ms` and `above_250_ms`. Samples count non-null durations; thresholds use strict
greater-than comparisons against integer nanoseconds. Nullable finite numbers `mean_ms`, `worst_ms`,
`p50_ms`, `p95_ms`, `p99_ms` and `slowest_1pct_mean_ms` are milliseconds. Empty populations have zero
counts and null measures. Never combine phases or reconstruct distributions from periodic averages.
Sort durations ascending as x[1..N]; nearest-rank p(q) is x[ceil(q*N)] for q = 0.50, 0.95 and 0.99,
without interpolation. For [10,20,30,40,100] ms, p50 is 30 and p95 and p99 are 100; strict counts above
33, 50 and 100 ms are 2, 1 and 0. The slowest-1% mean averages the largest max(1,floor(N/100)) values,
giving 100 ms in this example.
[documented] That floor rule preserves the existing metric, which is not p99 (`stress_sample.rs:207-218`).

## 4. Collection, volume and completeness

[hypothesis] Preallocate 1,024 frame slots and 3,072 diagnostic slots with reserved header and trailer
capacity. Drop diagnostics before using frame slots; drop frames when their queue fills. A bounded cursor
map drops and counts new-path observations on overflow; an import watermark prevents duplicate counts.
Collection never encodes JSON or performs file I/O. Producers assign seq, check stop conditions and field
bounds, and use a nonblocking queue send. Frame construction bounds markers to eight and their IDs to
64 ASCII bytes; diagnostic paths are at most 1,024 UTF-8 bytes. These bounds keep each encoded payload
under 8 KiB. A writer thread merges by seq, serializes and writes without blocking main or render threads.
It measures actual lines as a backstop, dropping and counting any oversized payload as exporter loss.
It keeps only wholly written durations, at most max_frames, and computes statistics at stop.

[hypothesis] Before a line would exceed `max_bytes - 16 KiB`, the writer freezes producers and cuts at
the last wholly written record, publishing its seq as `cut_seq` and its collection time as the byte stop
time. Already queued attempts and in-flight submissions with higher seq values are outside the interval.
Counters carried with records preserve cumulative attempts through that cut without an unbounded history.
Markers, including overflow, belong to their frame attempt for cut accounting; pending markers without
a frame attempt at or before the cut are outside the interval. The trailer keeps the next capture-wide
seq without reusing numbers assigned after the cut, so its seq may be greater than `cut_seq + 1`.

[documented] Legacy output lists at most 12 top-level GPU paths, not an exhaustive path count
(`engine/nvforever/src/graphics.rs:353-361`). Bevy constructs nested paths and may emit both elapsed_cpu
and elapsed_gpu for each timed path (`bevy_render/src/diagnostic/internal.rs:515-519,603-639`).
[hypothesis] Budget about 200–300 bytes for an ordinary frame and 250–350 bytes for a diagnostic record,
including LF; markers and longer paths increase these estimates. At 240 fps, 60 s gives 14,400 frames:
about 2.7–4.1 MiB without frequent markers, leaving ample margin in 64 MiB. Using 12 distinct top-level
paths with both CPU and GPU values gives about 24 observations plus one frame per update; nested paths
increase that. At 300 bytes per observation and frame, this is about 103 MiB over 60 s, or a byte stop
near 37 s; 48 observations give about 202 MiB and a stop near 19 s. GPU mode cannot fit 60 s at those
rates; it stops early at the byte limit with a shorter, still complete capture. These estimates require measured delivery and byte rates.

[hypothesis] Record drops leave seq gaps; any exporter loss within the interval, including markers alone, truncates capture.
Statistics describe written samples and may be biased. Attempted frames equal written plus dropped
frames; written frames equal the sum of phase frame counts; seq gaps equal dropped frame and diagnostic
records. With a byte cut, counts and seq-gap checks cover only records with seq at most `cut_seq`, excluding
the later trailer seq and all attempts after the cut. Loss before or at the cut still truncates; attempts
after the cut are not exporter loss. Duration, frame and byte limits end a lossless bounded interval and
stay `complete` when nothing in that interval was dropped; the trailer's counts give its actual length.
Settings changes, writer errors and shutdown timeouts truncate.
Freeze producers, drain the retained interval, write the trailer and flush, allowing 2 s at shutdown.
The terminal sentinel is exempt from duration checks, but not byte capacity. On I/O failure, stop, log one error and attempt a
trailer only if writable. Mark partial lines or absent trailers incomplete; never invent exact final drops.

## 5. Coexistence, consumption and validation

[documented] Existing frame windows use Time<Real> in Update and omit cycle warmup; profile windows
use it in Last, while profile hitches use consecutive Instant calls (`graphics.rs:329-350`;
`profile.rs:165,267-316`). Branch `codex/1275-render-diagnostics` provides
`graphics::ensure_render_diagnostics` (`engine/nvforever/src/graphics.rs:320-324` on that branch).
[hypothesis] Keep all existing text lines unchanged. Telemetry retains warmup and all phases. GPU mode
must call that helper; frames-only mode does not request the plugin. Add only export-specific log lines.

[documented] Stress gives each launch its own home and captures stdout (`tools/stress/stress/lib.py:251-280`).
[observed] Sibling #1277 records provenance in `tools/stress/stress/metadata.py:158-198`, attaches it in
`cli.py:285-286` and creates serial repetition folders in `cli.py:406-419`; those changes are not on this branch.
[hypothesis] Stress will pass unique run and scenario IDs and discover new matching files under each
launch's home/logs. Require one matching capture; save its path and validation per repetition. Validate
field types, finite values, order, strictly increasing seq, nondecreasing t_ns, bounds, IDs, null reasons,
count invariants and recomputed statistics, restricting counts and seq gaps to seq at most `cut_seq` when
it is non-null. Allow FrameCount wrap and import times preceding collection.
Keep engine identity separate from runner provenance. Older runs without files retain coarse results
and unavailable percentiles. Reject missing, invalid, partial or lossy requested exports for comparisons.
Preserve run-status and load gates; averaged repetition quantiles must never be called pooled percentiles.

[hypothesis] Synthetic tests should cover known distributions, N=99,100,199,200, threshold equality, first
null and later zero deltas, phases, seq gaps, FrameCount wrap, queues, cursor overflow, markers, all bounds,
write failures, partial shutdown, import deduplication, non-finite diagnostics and unavailable origin metadata.
Cover lossless byte cuts, losses before the cut, excluded later attempts and maximum-size escaped payloads.
Cover delayed or missing GPU data and unsupported timestamps. Test provenance
after commits and branch changes, with missing Git or outside a repository, legacy runs and repetition discovery.
Supervisor game checks should parse a capture, match markers and phases, inspect adapter and backend
availability, and exercise limits. After builds and checks finish, compare disabled release, frames-only
and GPU captures serially with equivalent settings, cache and load. Report overhead, CPU, available GPU
evidence, byte rates, size and drops; check pacing and console and display state. No overhead target is assumed.

[hypothesis] GPU mode exports all timed paths; on fast machines the byte limit shortens the capture
instead of a path filter. Cursor capacity is an implementation choice. Overhead, rates, device
availability, phase and marker ordering, and provenance refresh behaviour await implementation checks.
