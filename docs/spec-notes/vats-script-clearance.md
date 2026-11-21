# Named ordinary-play lateral clearance queries

[documented] The GECK function index lists `GetVATSLeftAreaFree` and
`GetVATSRightAreaFree` among its VATS/condition functions:
[function index](https://geckwiki.com/index.php/Function_Page#VATS_Functions).
The index supplies no origin, shape, filtering or timing contract. A name listing
is not evidence of a script opcode or native argument encoding.

[observed: original installed mod data] Enhanced Movement 1.43 contains one call
to each name in its lean controller, with an explicit player receiver and no
arguments. Those ordinary GameMode results influence the controller's requested
node transforms. This establishes source demand, not executed native behavior.
The inspected INI disables leaning; effective saved quest values and installed
feature execution are unmeasured. No mod setting is changed by this contribution.

## Mapped engine contract

Every `[design]` item in this section is an engine design choice (origin, reach,
filter, zero policy, catalogue float return), not a claim about the original game.

[hypothesis] The original functions report free space beside the receiver as a
distance in game units, larger when there is more room. Neither the GECK index nor
the mod's calls state the result's meaning or unit; no original-game capture exists.

[design] The two named source queries cast sideways from the current qualified
actor's position/facing through the existing script-tick physics caster, only
when called. They do not reuse a previous kill-camera measurement, publish a new
per-frame cache or walk all actors. Omitted receivers keep the existing caller
context. Explicit invalid source receivers retain the VM refusal before host
execution; valid nonactors and unavailable physical contexts return zero.

[design] The query frame reuses the current mapped actor-eye height of
`GetRayCastPos` (64 game units above the feet), with level right/left axes from the
receiver's heading. Reach is the existing camera-room limit, 1024 game units.
The ray uses the existing kill-camera static-world filtering: terrain and static
collision, excluding actors, dynamic bodies and the same ignored collision layers.
A hit returns its bounded finite distance; unobstructed space with a live caster
returns the reach. Missing/malformed pose or caster data returns zero. These
choices are explicit NVForever mappings, not a claimed native measurement policy.

[design] CameraRoom, area_value and CPTH condition evaluation keep their existing
meaning and values. Named queries neither overwrite them nor mutate World,
VATS state, camera settings, input, physics geometry or game time. They keep no
persistent result across a save, new scene or missing caster.

[hypothesis] Native result meaning/unit, origin height, view selection, ray-versus-volume measurement,
range limit, collision exclusions, nonplayer receiver support and timing relative
to lean/camera animation remain unverified. Query-only original-game captures are
needed before any native-parity claim. No opcode adapter is inferred.

## Acceptance controls

[validation plan] Independently authored compiled-source controls must prove
numeric distances, current heading, opposite sides, repeated/no-caster and
invalid/nonactor handling; retain a distinct existing CameraRoom as a no-clobber
control. A source callback must use clearance to update the existing named-node
transform request, preserving its real consumer and exact expected effect.
Engine controls must use the actual PhysWorld caster against asymmetric authored
static walls, with terrain/dynamic/actor and scene-removal neighbors, rather than
only supplying invented hit values. Unused queries must do no casts. All newly
authored controls were unexecuted at initial authoring. Hosted composite ab093471
(head 36db07e7) subsequently passed both real-physics controls once and the
engine strict checks. Core stopped in the existing console catalogue census:
33/92 versus 33/90, before the three integration cases. The census omitted
these two implemented source handlers, although the console already compiles
unnumbered function calls. A shared name predicate now connects the handler
and census, with an unknown-name neighbor control and the original 33/90 pin
retained. Composite 1c82c77f (head 3e71f3e2) subsequently passed that census
control, both ordinary/no-caster core controls and both physics controls; the
registered transform control failed its bare-Translation predicate. That fixture
selected FirstPerson, whose accepted queue payload is ForModel-tagged. The
corrected assertion uses into_model and checks both FirstPerson and the original
exact reference/node/[3,0,0] payload, retaining later [4,0,0] and one-edit controls.
The old failed vector was not printed, so its runtime contents are not inferred.
The corrected fixture remains unexecuted at authoring; both earlier failures are
retained. Original mod initialization,
configured inputs, animation/rendering, save/load and full installed acceptance
remain separate requirements.
