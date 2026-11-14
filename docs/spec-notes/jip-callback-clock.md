# JIP main-loop callback elapsed time

[documented: GECK Wiki, [SetGameMainLoopCallback](https://geckwiki.com/index.php/SetGameMainLoopCallback)]
A registration invokes a zero-argument function on its selected reference, with
the player as the omitted reference. Its delay is in frames; flags select game
or menu execution, and flag 8 removes the registration on loading.

[documented: GECK Wiki, [SetAnimCurrentTime](https://geckwiki.com/index.php/SetAnimCurrentTime)
and [SetGlobalAnimTime](https://geckwiki.com/index.php/SetGlobalAnimTime)]
The examples use GetSecondsPassed inside a game-mode main-loop callback to
advance animation time. They do not specify the first interval, delayed-call
intervals, nested-call inheritance, or session-transfer timing. These citations
were read by the source author on 2026-11-14; no native implementation was used.

[documented: pinned public source at
[576d3963 Game](https://github.com/LootJunkieAzza/NVForever/blob/576d39630287845c598914d17ddffd71eebbef28/crates/fnv-game/src/game.rs),
[JIP dispatch](https://github.com/LootJunkieAzza/NVForever/blob/576d39630287845c598914d17ddffd71eebbef28/crates/fnv-game/src/jip_events.rs),
and [clock functions](https://github.com/LootJunkieAzza/NVForever/blob/576d39630287845c598914d17ddffd71eebbef28/crates/fnv-game/src/functions/misc.rs)]
Compiled function invocations use temporary locals and have no persistent script
instance. GetSecondsPassed returns zero without such an instance, and main-loop
dispatch supplies no callback-specific clock. The existing game clock is
quantized to 1/64 second and pauses during menus/loading. Ordinary instance and
trigger clocks are separate consumers and retain their existing behavior.

[hypothesis: bounded compatibility policy] A registration keeps the quantized
game-clock time of registration or its last actual invocation. A due callback
receives the nonnegative difference, shared by repeated reads and its nested UDF
Calls. Different registrations keep independent origins. Re-registration resets
the origin; callbacks retained across session transfer rebase to the receiving
game's clock. A frozen game clock contributes zero. Native interval parity has
not been measured.

[hypothesis: invocation scope] Carry the optional interval only through the
callback UDF execution and nested UDF Calls. Only GetSecondsPassed without a
persistent instance uses it. Ordinary UDFs, result scripts, persistent instance
clocks, callback filtering and normal function-result bookkeeping remain on
their existing paths. No global override, serialized clock field, opcode change,
actor-value special case or script-name matching is introduced.

[documented: authored control source in
`crates/fnv-game/tests/common/jip_callback_clock.rs`, registered by the existing
`synthetic` integration target] Six controls exercise compiled OnHit callbacks
and Game::tick; independent subjects/delays and nested reads; paused menus;
explicit removal and re-registration; flag-8 cleanup/session rebasing; and
ordinary/result/instance neighbors. The fixture explicitly binds SGMLC's
receiver. It does not infer a receiver from the query's RunCtx alone. All public
576 compilation and execution remain unqualified until separately reported.

[hypothesis: limits] This leaf does not repair previously saved modifiers or
establish native timing, automatic engine event coverage, whole-mod support,
animation, audio, renderer or device behavior.
