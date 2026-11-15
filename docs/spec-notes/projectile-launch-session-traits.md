# Session projectile traits at initial shot creation

[documented: public main da13f4f8a65cb56d9f4f38823d79a0126ff9467b, crates/fnv-game/src/nvse_jip7.rs num_field/set_field; crates/fnv-combat/src/data.rs CombatForms/projectiles; engine/nvf-play/src/combat.rs fire/actor_attacks] Existing Game setters retain session field overrides, while the original physical launch reads imported typed CombatForms records. Getter readback alone does not establish new-shot velocity or carried impact force. The existing trait mapping identifies speed as2 and impact force as9; portable schemas/catalogue define ammo/weapon projectile selection.

[hypothesis] Resolve the selected ammo/weapon projectile for each new shot. Clearing the ammo projectile falls back to the weapon's. Resolve a temporary PROJ form through the existing original-form relationship, keep the requested identity and its own overrides before inherited original overrides, and copy only supported speed/impact-force values into a per-shot record. Preserve finite signed values including zero; refuse nonfinite or nonrepresentable f32 overrides. Leave typed caches, unrelated forms and existing flights unchanged.

[hypothesis] Share the existing random spread/spawn operation between ordinary and edited shots. Preserve no-runtime/no-override typed behavior, vanilla weapon speed multipliers, normal player ammo/magazine/counters/animation, and the distinction between a selected missing typed record (effects/counters but no physical flight) versus no selected projectile (the existing silent path). This leaf adds no setter and broadens no generic World placement admission.

Process qualification (historical authored CPU controls): An owned coherent d9 source pair ran the same eight source-compatible controls: three baseline passes and five physical-consumer assertion failures, then all nine candidate controls passed. The ninth control uses the new factored spawn API and was candidate-only. An earlier E0463 host dependency-search setup failure ran zero tests and remains separately recorded. Later coherent c28 CPU contexts also passed the original nine bodies. These results belong to those exact source contexts, not any later public revision.

[documented: authored engine/nvf-play/src/combat/projectile_launch_tests.rs] Nine portable-record controls exercise actual Game setters and next-frame actor shots, physical velocity/carried force, restoration without editing existing flights, temporary-form trait inheritance, unrelated typed neighbors, missing/no selected projectile and normal equip_sync -> combat::fire -> fly with actual ammo, magazine, counters, animation and shot messages. The player input boundary is CPU PlayerControl.trigger, not OS input or complete animation scheduling. A fixed-random three-projectile control checks six draws and independent analytic direction sentinels.

Process qualification (pinned public hosted CI): [run 43666264301](https://github.com/LootJunkieAzza/NVForever/actions/runs/43666264301) and its [guard](https://github.com/LootJunkieAzza/NVForever/actions/runs/43666264187) completed all seven checks successfully for source head `cc789749b9b69c27dac9a1a7609bb2b2ee9c76df`. The actual checkout was merge `897883d59090670466e86a87458e3ee18d375e1d`, combining that head with main `78408bcba623baaabbab4bb77a0e19d3a5e4fd72`. All nine portable launch controls passed once in the engine job. Complete summaries reported 3019 core passes with four existing ignored cases, and 2917 engine passes with zero ignored cases; both had zero failures, measured or filtered cases. Both strict Clippy checks, both dependency audits, the six Python suites and the data guard passed. These are repository/process results in that tested merge context; they are not pristine-head, native, rendered or full B42 acceptance. Later source revisions require their own exact-head CI and review.

[hypothesis] The authored opt-in physical-consumer source and scenario below require current-source typechecking and review. Actual GAME execution remains optional and UNRUN. Getter-only JIP smoke is not edited-velocity proof; the aid observes actual physical insertions. No guessed asset identity or rewritten acceptance oracle is introduced.

[hypothesis] Initial launch support does not complete B42 startup, quickthrow input/animation, item/reference spawning or native parity. Temporary form trait resolution does not establish temporary-base World reference admission, visual model lookup, GetBaseObject/type51 scans, TTL deletion or save/session ownership. Existing clone/high-ID/callback/save boundaries remain untouched. No generic high-ID exemption, warning suppression, DLL-version spoofing, native implementation, installed script, asset or save is part of these authored controls.


## Authored physical-consumer scenario

[hypothesis: bounded diagnostic policy] The opt-in `NVF_PLAY_TEST_PROJECTILE_LAUNCH`
contains one bounded weapon EditorID, resolved uniquely from the loaded typed
records. The authored scenario uses `Weap10mmPistol`; missing, ambiguous, modified,
beam, multi-projectile or unsupported records refuse setup. No numeric FormID is
guessed. The aid requires the canonical `b42-launch-probe-home` directly under
the smoke output directory, the matching actual SaveConfig, and no preexisting
saves. A fresh output directory is required for each run.

[hypothesis: physical acceptance] Compile real script getters/setters and
`player.FireWeapon` with exact loaded forms passed through `StringToRef`. Observe
only the resulting actual `Flying` insertion, after the ordinary actor shot
consumer pushes it. The first shot must carry the staged finite speed and force;
a second shot must carry independently resolved original physical values after
restoration. Keep weapon speed multipliers and the ordinary random shot path.
Scripted FireWeapon does not debit ammo here; the normal player-attack debit is
covered separately by the unchanged portable CPU control. Getter values or a
queued request alone never satisfy the physical acceptance oracle.

[hypothesis: cleanup policy] Preserve exact presence and XVal contents of the two
session override entries, not just their numeric getter values. An opaque lease
binds restoration to its original Game incarnation and can be consumed once; a
wrong-owner attempt retains it for its actual owner. For restored physical values,
a nonnumeric prior override follows the existing typed launch fallback rather
than the numeric getter's zero. Unsupported original representations refuse the
fixture. Capture and restore weapon/ammo item counts on success, script error,
physical mismatch, timeout or observed teardown. Cleanup does not claim a general
rollback of inventory events, world state, arbitrary concurrent edits or a
retired/replaced Game. Those boundaries fail visibly without touching a replacement.
The run owns its isolated QA session and never loads or writes an original save.

[hypothesis: bounded scheduling] The aid is absent in normal runs. Its finite
waiting and active-stage deadlines, at most two matching requests, distinct actual
insertions, ordered stage markers and final exact-field/count cleanup are required
by `projectile-launch-physical-session-traits`. Raw failures remain visible. The
launcher tests use a real owned child only to verify the delivered EditorID, HOME
and inherited-aid filtering; synthetic judge rows test acceptance rules and are not
rendered/game results.

Process qualification: the pinned current2c source's seven lease controls,
nine initial-launch controls and seven physical/context controls passed all23
selected CPU cases in a coherent owned dependency graph. This result is specific
to that source; it is not whole current-public canonical CI or GAME acceptance.
The two Python launcher/judge boundary checks also passed against that source.
Original failed source-argument assumptions and setup errors remain retained.

[documented: authored engine/nvf-play/src/combat/projectile_launch_probe.rs::drive]
The frame guards use strict greater-than comparisons against7200 waiting frames
and300 active frames, checked before staging or restored-shot dispatch. Independent
real-time guards remain120 waiting seconds and15 active seconds.

[hypothesis: bounded diagnostic policy] Two additional direct boundary controls set
the actual Probe counters near each cap, with real elapsed time below both separate
timeouts. The wait control admits7200 without staging, makes the7201 update eligible at20 real seconds (below the waiting timeout), and requires refusal before preparation can acquire a capsule or fire.
The active control admits the first real physical insertion at300, aborts301 before
a restored second request, and requires exact absent-field restoration, original
inventory counts and no repeated dispatch or cleanup. They use no7200/300-frame
loops. All original23 control bodies are unchanged. Authored coverage specifies
the required oracles; compilation and execution results must be bound to the exact
source revision and reported separately. Actual GAME execution remains optional
under CONTRIBUTING; the meaningful consumer scenario must be source-reviewed and
typechecked before Ready.
Nothing here completes B42 startup, Quickthrow OS input/animation, temporary World
reference placement/model lookup, TTL deletion, save ownership, native or rendered
acceptance.
