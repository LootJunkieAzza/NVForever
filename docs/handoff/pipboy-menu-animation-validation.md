# Pip-Boy first-person script animation validation

The device-menu clock and selected script-layer blends are implemented from
[the mapped behavior note](../spec-notes/pipboy.md#first-person-script-animations-while-the-device-is-open).
The owner's holstered/no-weapon invisible opening is the target. A log reporting
`raised` or menus on the model does not prove visible output.

At the original `fac7e359` source freeze these controls were authored and
unexecuted. Later [hosted CI](https://github.com/LootJunkieAzza/NVForever/actions/runs/44027976201)
and the [game-data guard](https://github.com/LootJunkieAzza/NVForever/actions/runs/44027976036)
passed all seven checks on their first attempt with Rust 1.100.0. These results
qualify `fac7e35965aa2e1f83be1b4678a416a9fa6742d7`, not a later documentation
successor. Actual checkout `e07aac03a8413a8dd50f8e4a24099a18921dd894` has
parents main `a0ebe1c516ce9472ae9c051cb53bc85ce7135c0f` and that repair head;
its complete tree matches the repair. Core reported 3,446 passed, zero failed,
five ignored; engine reported 3,412 passed, zero failed, three ignored. Root and
engine strict Clippy, dependency checks and tools checks passed. Ignored tests
receive no execution credit.

Eight individually listed controls passed exactly once, along with the aggregate
registration guard and all 23 blend tests. The blend group includes the selected
smoothing control already listed: these are ten checklist scopes, not ten distinct
test cases. The table records those hosted results at `fac7e359`.

| Boundary | Authored selector | Execution |
|---|---|---|
| Actual aggregate registration, including the new source fixture | `cargo test -p fnv-game --test integration every_test_file_is_built -- --exact` | Passed once at `fac7e359` |
| Source compiler, inventory freeze, first-person sample/getters/setters, NPC/third-person controls, unrelated menus/loading, deferred seek, deactivation/resume, automatic completion and fresh runtime | `cargo test -p fnv-game --test integration first_person_menu_clock::pipboy_menu_advances_only_player_first_person_samples_and_lifecycle -- --exact --nocapture` | Passed once at `fac7e359` |
| Cold third-view SetGlobalAnimTime before first-person activation/getter, held-menu samples and NPC/third-person explicit-clock controls | `cargo test -p fnv-game --test integration first_person_menu_clock::cold_third_person_global_does_not_freeze_first_person_menu_sequence -- --exact --nocapture` | Passed once at `fac7e359`; no first-person warm-up |
| Real layered CPU pose, priority-1 script above ordinary Pip-Boy hold, world delta zero, smoothed and unsmoothed holstered/drawn hands, menu fade-in/out and third-person control | `cargo test -p nvforever player_body::sequence_consumer_tests::device_menu_sequence_moves_and_blends_with_holstered_or_drawn_hands_and_a_held_world -- --exact --nocapture` from `engine` | Passed once at `fac7e359` |
| Production prepare-frame admission consumer with actual core menu ticks, skipped runtime ticks, Mods page frames and loading | `cargo test -p nvforever script_sequences::tests::prepare_frame_uses_admitted_menu_time_once_when_runtime_ticks_are_skipped -- --exact --nocapture` from `engine` | Passed once at `fac7e359`; candidate-only metadata seam |
| Existing source clock/transition controls | `cargo test -p fnv-game --test small_libs explicit_player_sequences_keep_view_clocks_targets_and_release_state -- --exact` and `transition_helpers_preserve_explicit_weight_and_offset_readbacks` | Passed once at `fac7e359` |
| Existing prepared-sample CPU pose control | `cargo test -p nvforever player_body::sequence_consumer_tests::prepared_public_sequence_reaches_real_pose_and_releases_to_the_underlying_clip -- --exact` from `engine` | Passed once at `fac7e359` |
| Selected pose time reaches smoothing while ordinary layer time/ease and locomotion phase stay held | `cargo test -p nvf-assets --lib anim_blend::tests::admitted_pose_time_moves_a_smoothed_overlay_without_ticking_ordinary_layers -- --exact --nocapture` from `engine` | Passed once at `fac7e359`; candidate-only consumer |
| Existing blend/fade controls after sharing the fade step | `cargo test -p nvf-assets --lib anim_blend::tests` from `engine` | All 23 passed at `fac7e359`; includes the smoothing control above |

The CPU pose fixture supplies an authored decoded clip at the existing prepared
sample seam. It does not execute asynchronous KF loading, the complete renderer
schedule, a weapon model or the installed mod scripts. No game assets belong in
these fixtures.

Baseline comparisons remain unrun. The cold core fixture can overlay `37cc579`,
and the smoothed CPU pose fixture can overlay `a8b6`, without changing their inputs
or oracles. The core clock fixture uses only interfaces present in the accepted
baseline. Overlay its identical source and aggregate module registration there;
the exact child process owns its process-wide file-availability seam. The new CPU pose fixture uses the candidate's explicit
menu blend-time seam; an original-source run must declare its adapter for that
field rather than count a baseline compilation error as the expected failure.

The metadata fixture calls the production prepare-frame consumer in an ECS system
and advances its real core menu clock with Game::tick. Repeated preparations
without a runtime tick must admit zero blend time despite retained positive
frame configuration. It excludes asynchronous loading and the full run_scripts
or Escape-input pause schedule; ordinary Escape reachability over an active
engine menu has not been established. This fixture is candidate-only.

Earlier static verification at `fac7e359` covered 1,639 smoke definitions and
selected 346 scenarios: 127 Game launches and 219 tool runs. None was executed
for this repair; a plan is not a smoke pass.

The real launcher boundary still needs baseline and candidate checks with the
same installed profile: Pip-Boy opening with no weapon, a holstered weapon and a
drawn weapon; close/cancel/reopen; combat/reload control; menu pause modes; and
save/load. The separately observed zero-submission readiness cap remains open.
Native pause parity and complete installed-mod compatibility are unqualified.
