# Live Stewie critical-setting queries

## Supported behavior and policy

[documented: Stewie Tweaks v9.87 readme and generated `nvse_stewie_tweaks.ini`, see
[tweaks](../design/tweaks.md#sources-and-clean-room)] The original profile names
`bCritChanceIgnoresFireRate` and `bScaleCriticalDamage` in `Tweaks`, and
`fCriticalDamageMult` in `Critical Hits`. NVForever has a fire-rate choice and a
critical-damage multiplier, not a separate scale-enable switch.
[documented: the existing catalogue and
the StewieGetValue behavior page](https://geckwiki.com/index.php?title=StewieGetValue)
StewieGetValue reads a `key:section` setting; StewVal is its alias. The page
describes a queried boolean as whether its tweak is enabled; it does not specify
a disk-read or reload contract.

[hypothesis] The original uses fCriticalDamageMult as a multiplier like the existing
NVForever critical-damage scale. The plugin's own formula has not been observed.

[design] After application settings are handed to the actual combat
consumer, the recognized bCritChanceIgnoresFireRate and fCriticalDamageMult queries
read those same current Game settings. This intentional mapped-policy change keeps
script stat calculations consistent with effective NVForever choices. Unbound
headless games and unrelated keys retain the existing layered-file query. No
second setting-value cache is added. The native scale-enable switch is not inferred
from the engine multiplier: bScaleCriticalDamage remains a raw-profile query and
its enabled-intent/native equivalence is open. Consequently the complete original
ItemCards/PipBoy critical-stat initialization is not yet qualified for every
conflicting scale-switch/profile combination.

[hypothesis] In the original, a query returns the plugin's current choice rather than
re-reading the file, so answering from the live engine setting matches it. Not
observed in the original game.

[design] The original INI does not supply defaults for any NVForever-owned option.
As in [Tweaks](../design/tweaks.md), NVForever settings keep their ordinary defaults
unless changed through NVForever. The critical options default to respecting fire
rate and using a ×1 damage scale. The existing NVForever settings reader remains
the only source of stored application preferences, including explicit off and
default-valued choices.

## Startup, restoration and lifetime

Runtime initialization binds the application settings before new-game initialization
and Restart files. After restoring a saved game, the current application settings
are rebound before Restart and Load observers, rather than persisted in the save.
Ordinary frame synchronization continues through the existing Tweaks consumer.
Changes to the original INI can affect the existing fallback but never alter
NVForever settings. There is no new watcher, per-frame disk read, save field or
installation write.

The existing two critical-rule assignments are shared with the actual damage
consumer; formulas, ordinary condition penalties and unrelated tuning do not
change. This does not implement menu search, selected-row stat publication or any
other Stewie service.

## Validation and limits

The retained ordinary core control checks both aliases, current-field updates,
unbound/unrepresented keys, save lifetime and rebinding. Two opt-in engine controls
exercise actual Runtime initialization, source files, critical-hit calculations,
application defaults and explicit choices in the presence of a contrary original
INI, and restoration. They retain the provider-version and warning controls.
Run each exact selector in its own process with a dedicated `NVF_STEWIE_PROFILE_HOME`,
`--ignored` and `--test-threads=1`; ordinary CI skips them.

[implementation evidence: head `68a9c978`, 2026-11-17] The
[natural hosted run](https://github.com/LootJunkieAzza/NVForever/actions/runs/44155882635)
used checkout `2a932d9b` with parents `[13011912,68a9c978]`. All seven checks
passed. Six owning non-ignored controls each passed once: one live critical query
control, two controls for the now-removed typed profile reader, two inventory
controls and the ordinary Pip-Boy control. The three ignored controls received
no credit from that run.

[implementation evidence: exact original `68a9c978`, Windows Cargo 1.99 QA]
The two critical-profile opt-ins and the inventory opt-in subsequently each
passed once in three separately admitted runtime processes, under the former
profile-import policy. The critical case checked initialization, source files,
damage rules, current choices and restore. The conflicting-profile case checked
current settings against the actual damage consumer without rewriting either input
file. These are historical results only; identical-fixture baseline comparisons
remain unrun. The inventory case and its
per-phase sentinel checks are described in [ext-small.md](ext-small.md#live-inventory-settings).

The Windows Cargo build returned zero, but its compile controller failed
descendant cleanup and the child identity remains unknown. That controller and
its overall result remain false. Runtime-only r1 then stopped with `KeyError` on
missing `priorResult` metadata before any Job, discovery or test. Corrected r2 reused
the sealed emitted image without rebuilding and passed all three opt-ins; its
runtime qualification is separate from those failed attempts.

Earlier hosted engine and engine-Clippy jobs stopped at E0599 because the shared
damage fixture called nonexistent `Rules::default()`. The fixture was corrected
to the existing `Rules::defaults()` without changing damage inputs or oracles.
Neither failed job executed an engine control or established a separate lint
failure. Later results above do not relabel those attempts as successful.

No compiler, runtime, strict-lint or hosted checks have run on the corrected
integration. The historical results do not qualify this candidate.

There is no provider-version increase, popup suppression, auto-dismissal or mod
disable. The historical941 report is not validated by this bridge. The960/970
startup guards remain blocked until their actual required services and compatible
reporting contract are established. Native reload and scale-enable parity, paired
original-game results, GPU/installed-mod acceptance and complete Stewie
compatibility remain unqualified.
