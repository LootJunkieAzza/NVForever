# Scoped mod diagnostics

These commands inspect repository execution boundaries. They do not certify a native
plugin, version, installed mod profile or rendered behavior. Existing `ext report`,
`ext scan`, `NVF_PLAY_TEST_MOD_LOG` and `--mo2-profile` behavior is retained.

## Compiled call coverage

```
fnvtool ext connections <Data> <plugins>... --schema schemas/fnv [--plugin name]...
```

The game library builds the units using its ordinary source compiler. The connection
walk reads that compiled IR, including calls in branches that have not run. It does not
create a Game, run startup scripts, or invoke placeholder arguments. Numbered calls
report `registered`, `no_op`, `stub` or `unhandled` from the actual opcode registry;
only console-assigned numbers also use the console name registry, matching Game dispatch.
Named calls report `unknown` when source compilation identified an unknown name;
otherwise they remain `unchecked`, including VM intrinsics and catalogue-only names.
A registered function can still reject real arguments or lack an engine consumer.

Units that kept their bytes after a source-compile error are counted separately and
their calls are omitted. Unread bytecode statements also have a separate count. A
plugin filter absent from the selected load order is an error. The report refuses
more than 200,000 selected units or 1,000,000 call sites instead of returning incomplete
coverage as if it were complete. Loading and compiling the selected load order still
has the ordinary costs of `ext report`.

## Played-log digest and comparison

```
fnvtool ext mod-log run.log [--previous previous.log] [--detail] [--summary new.json]
```

Only the explicit files are read. No Data, mod directory, profile or parent directory
is scanned. Default JSON exposes diagnostic categories and counts, not the messages,
plugin names or paths embedded in them. `--detail` includes private messages; inspect
that output locally before sharing it. This does not change how the game logs data.
Invalid UTF-8 is decoded with replacement and the number of affected lines is reported.
Complete ANSI CSI sequences are removed; truncated escape sequences are retained.

The parser recognizes the current `mods:` failure, compile, dependency, HUD, MCM and
exit rows, Script Runner skips, UI warnings, movement, message boxes, errors and panics.
Unrecognized lines are not proof of success. The aid prelude and its load-order row belong to one session; another launch
prelude or load-order row starts a separate session. Comparisons select the last session from each input and identify their indexes.
Default comparisons show changes in category record counts. Detailed comparisons also
show newly present and absent message entries, without claiming that an absent message
means a fixed defect. Repeated plugin exit totals replace that plugin's earlier total;
they are not summed twice. Totals reported at exit are separate from sampled log-record
counts and can be missing or incomplete. Missing totals are null; integer overflow is
explicit and also makes the total null.

Each input is limited to 64 MiB, each line to 64 KiB, and all sessions together to 4,096
retained entries and 8 MiB of retained words. At most 128 sessions are accepted. Exceeding
a limit returns an error without a partial digest. An explicit missing previous log is
also an error. A summary passes the existing install write guard, checks only its selected output
ancestors for an otherwise unremembered game install, and requires a new file, so it cannot overwrite a log or an existing summary.

## Actual Script Runner trace accounting

```
fnvtool ext script-runner <Data> <plugins>... --schema schemas/fnv --root <authored-root> --no-data-files --run [--detail]
```

The existing compilation and restart/new-game result rows stay unchanged. With `--run`,
`diagnostics` summarizes actual `CallFailed`, `ScriptError`, `Skipped`, `Unhandled`,
`ArgUnread` and `ScriptPrint` trace records. A successful result row can coexist with a
call failure: some calls continue with zero. Counts refer to trace events, not total
runtime calls; the game already deduplicates some diagnostics before tracing them.
Unit names and printed/error words in the new summary require `--detail`. The existing
compile/result error fields retain their previous behavior and may themselves be private.
The summary admits borrowed payload lengths before formatting or copying words. More
than 4,096 distinct entries or 8 MiB of unit/text bytes makes diagnostics unavailable
(`available: false`, `reason: retention_limit`), without partial counts. The original
command still returns its compile/run rows and keeps its previous exit behavior. It adds
no runtime tracing hook, background scan or per-frame work.

## Portable validation and provenance

The Rust controls exercise actual source compilation and VM events, redaction and
retention boundaries. `tools/smoke/tests/test_diagnostic_cli_batch.py` requires an
explicit `FNVT_DIAGNOSTIC_BIN` and invokes that actual CLI over generated authored
plugins, Script Runner files, logs and a synthetic MO2 profile. Its opt-in skip is no
execution credit. The same file can qualify baseline and candidate binaries; record
actual first failures rather than forecasting counts. `mod-diagnostic-log-tool` is
the registered smoke scenario over the two authored text logs.

This implementation adapts public GPL-3.0-or-later work by h9x000, with Claude Fable 5.2
credited in the original history: connection walk
`e41bee41eec6c01e80be4ee420e93412d04fce33`, runner accounting
`ffc6b6fb62484b1089f08dddf328681b61aab165`, and log digest
`7c1bfa19f9efa3a6f600b5337026dadceb425ce2`. Their old validation statements do not
qualify this adapted source. Terminal-aware engine stderr from
`3fcbfbfc47ed0264fe6c6345e251e16d36927a39` is a separately coordinated transport
change; it is not included merely by referencing that commit. No extender/native
implementation source or original game assets are used here.
