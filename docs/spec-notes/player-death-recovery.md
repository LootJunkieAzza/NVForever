# Player death recovery

[observed] The original-game campaign `o29_dmg` (2026-11-08, issue #1858)
reports that player death reloads the save, and warns that a pending capture job
can therefore repeat after each reload. The recorded method and limits are in
[observer-checks.md](../handoff/observer-checks.md), item 89. That observation
establishes a reload, not the exact slot ordering, menu treatment or clock below.

[documented] GECK's [Complete List of GameSettings](https://geckwiki.com/index.php?title=Complete_List_of_GameSettings)
lists `fPlayerDeathReloadTime` with default `5.000000`. This leaf reads the live
loaded/runtime GMST through `World::setting_f32`; it does not alter game data.

[hypothesis] Engineering policy: a finite nonnegative setting takes precedence;
a missing, negative or nonfinite value uses five seconds. Count real seconds
only while the world is playing and unpaused, beginning after the first observed
dead frame. The death camera's slow-motion multiplier does not slow this timer.
A script resurrection cancels the attempt even while a menu is open. These clock
and cancellation details require original-game timing confirmation.

[hypothesis] Engineering policy: attempt recovery once per uninterrupted death.
Read only the current NVForever profile's own save directory, ordered by the
existing save-header timestamp, without preferring manual over quick, auto or
system slots. Use the existing full format/plugin/remap/body preflight, skip
unreadable or incompatible files, and skip bodies whose player is already dead.
This avoids automatically reloading an already-dead player. It cannot prove that
a later script will not kill a living saved player again. No original `.fos` save
is imported by this feature.

[hypothesis] Engineering policy: open the existing Load menu before sending the
existing `LoadRequest`. Leave it open when no living usable save exists or a
handoff cannot start; do not continuously dispatch requests. An already-pending
load is retained without adding another. Use the existing child-process/window
handoff, not a second restart mechanism. No save is written by death recovery;
the existing refusal to save a dead player and queued-autosave handling remain.

Process qualification (historical authored CPU controls): Ten authored CPU controls passed in an isolated normal engine context
based on main26e plus six UI ports. They use portable authored records, real
`Game::kill`/`resurrect`, actual NVForever save files, ECS time/menu resources and
`LoadRequest` messages. They cover setting changes/defaults, pause/resurrection,
manual/quick/auto ordering, one dispatch, invalid/dead save fallback, absent or
unreadable profiles, pending loads and preservation of an existing autosave.
These are historical CPU results, not a build or run of this public port.

Public source qualification: The original ten recovery controls cover the live
delay, selection, cancellation and dispatch behaviors above. Earlier public
heads have their own hosted results; checks for a changed head must be reported
against that head. Rendered death/menu recovery and child-window handoff remain
separate from CPU and process-boundary controls. The authored
`death-recovery-handoff` scenario checks the existing window handoff; preparing
its source does not establish a GAME result.

The existing camera-only checks retain every original acceptance condition,
including `death-fade` black at six seconds. Their isolated homes and successful
loaded `fPlayerDeathReloadTime=120` console checks are retained. No parity golden,
duration, acceptance range or existing expectation is relaxed.



Implementation context (NVForever 7201ca6d57d3bb8a435bffc3fc7a5ecedf79dea3,
`crates/fnv-game/src/functions/console.rs` set_game_setting and
`crates/fnv-sim/src/world.rs` setting_f32/change_setting): Existing `setgs` mutates
only an actual loaded setting for the session. The recovery consumer reads that
same live value. The 120-second camera-only fixture input is an engineering test
setup; it is not an original-game timing observation or a production delay change.
The separate `death-recovery-handoff` scenario retains its recovery behavior and
all expectations. Rendered runs remain UNRUN; source preparation and historical
CPU acceptance do not establish child-window handoff or no-save menu usability.
An early Draft may disclose these pending checks without waiting for them.


## Assisted runs and save ownership

Implementation context (public source `7bec60cebfe115ab863cf7d11358e26b61fdca5d`):
`tools/smoke/smoke/runner.py::game_env` supplies `NVF_PLAY_AIDS_FROM_SCRIPTS` to canonical
`--play` launches; `engine/nvf-play/src/test_aid.rs::drives_run` distinguishes driving
and measuring aids. `SaveConfig::default` selects the explicit NVForever home
or the ordinary player profile. The original handoff removed driving aids but
kept that clock marker and the same profile. A smoke death could therefore
select a real player save and leave its observation run. This is a source-backed
counterexample, not an executed native-game observation.

[hypothesis] Engineering test policy: capture the recovery admission once during
`SavePlugin` initialization. Ordinary runs, including measuring-only runs with
no assisted clock marker, keep normal death recovery. The presence of the clock
marker (including `0`) or a driving aid disables automatic death recovery before
its clock, latch, save-directory scan, menu change or load dispatch. Scripted
resurrection and ordinary save/load commands are not disabled.

[hypothesis] An assisted recovery scenario opts in only with
`NVF_PLAY_TEST_DEATH_RECOVERY=1`, explicit `NVFOREVER_HOME`, and an existing
canonical `death-recovery-home/saves` directly under that run's working
directory. The actual `SaveConfig.dir` must equal that saves directory. Missing,
foreign, redirected or mismatched directories visibly refuse the opt-in. This
path convention is an ownership precondition for an explicitly requested test,
not a claim that any similarly named user folder is safe to overwrite. The
recovery consumer reads saves; it does not write them.

The sole registry automatic-death-recovery positive is
`death-recovery-handoff`; it retains its living-save, restored-world and exact
handoff expectations and gains the explicit opt-in. The own-hit, grenade
self-kill and dead-save-refusal scenarios retain their assertions and add a
forbidden recovery diagnostic. All other death/combat observations remain in
their original process by default. Manual load/refusal/menu handoffs, healing
and NPC resurrection keep their existing behavior. The existing handoff still
preserves the profile and explicit handoff-exit/measuring exceptions, but drops
the driving aids and assisted script-clock marker in the successor.

Authored coverage includes actual recovery-update controls over typed NVForever
saves in fresh, owned child processes, ordinary/measuring preservation,
assisted defaults, owned opt-in and refused opt-ins. A separate real-process
handoff control records the OS-delivered argv/environment, then executes a fresh
Rust test child that uses the actual CLI parser, `SaveConfig::default` and
`start_from_save` on the exact staged save. It checks the save identity, loaded
placement, unchanged bytes and absent driving/clock aids. The child-test wrapper
does not start a renderer, restore a complete live world or prove window timing.
Its ignored entry points are invoked explicitly by their ordinary parent tests;
they are not standalone acceptance skips. New-head execution is reported with
the resulting source-bound checks, not inferred from these authored controls.

The previously noted both-buffer pending-load check and menu-under-handoff
rendering policy are unchanged. A handled earlier-frame load may still leave
the usable Load menu instead of issuing another request during the same death;
rendered coverage remains separate. No global environment variables are changed
by the new controls. They stage only authored saves beneath owned temporary
homes and never open the ordinary player's default profile.
