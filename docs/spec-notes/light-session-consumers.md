# Session light edits at placed-light and emittance consumers

## Contract

[documented: `jip-traits-4.md` §4, `GetLightTraitNumeric`,
`SetLightTraitNumeric`, `GetLightFlag`, `SetLightFlag`] The numeric light traits
select radius, RGB channels, falloff exponent, field of view and fade. Light flags
select named light behavior. These functions act on a light base form.

[observed: installed Megaton Illuminated winning UserConfig script, local demand
inventory, 2026-11-16] The configured mod changes neon colors and flicker flags
through those setters. Its source stays private. This establishes a consumer
requirement, not that this mod causes reported pause accumulation or overbrightness.

[design] The accepted setters remain the sole writers of canonical session record
fields. A revision marks light changes; renderer consumers read those same fields
when the revision changes or a new placed-light instance is created. There is no
second script command implementation or separate mutable light-value store.

## Render consumers

[design: `world.md` §8, `model-lights.md`, existing placed-light renderer]
Keep the renderer's existing radius offset, color conversion, fade/intensity,
spot-cone conversion, model-light fallback, animation and emittance behavior. Update
an existing instance rather than spawning a replacement each frame. A dormant
carrier permits a currently non-emitting light to start emitting after a session
edit; it remains owned by its cell/reference and is removed with that owner.
Carriers are excluded from model-root consumers, and streaming's emitting-light
statistics remain separate from registration of a non-emitting owner.

[design] Model binding is triggered by newly published rigs or newly emitting
modeled point lights. Foreign transient point lights do not trigger a full rig/light
binding scan. A resumed modeled light can bind to an already-loaded rig.

[design] Light-backed external emittance refreshes from the same canonical fields,
including when the hour/weather does not change. A placed light with an emittance
source keeps its base color separate from the source multiplier, so repeated updates
do not compound the multiplier.

[hypothesis] Native timing, behavior of invalid/nonfinite values, precedence between
script edits and model color controllers, and save/load lifetime require permitted
captures. Preserve existing setter conversion and renderer policy while those remain
open. The renderer's existing falloff shader does not consume per-light exponent;
that field remains an explicit unsupported render feature, even though its value
can be read and written. No full light API or Megaton compatibility claim follows.

## Validation

[design] Independently authored source calls the existing setters, then runs the
actual placed-light update and emittance consumers. Check point/spot values, cold
edits before spawn, repeated same-value updates, non-emitting/re-enabled instances,
cell cleanup/reentry, and stable entity counts across held/resumed frames. Preserve
ordinary lights and unrelated forms. GPU/rendered appearance, original-game parity
and the installed profile's complete feature playthrough are separate acceptance.

## Executed consumer checks

[validation: Windows CPU libtest, integrated source b8da4c3d41e9a950a6e43bdb4d95b0e3c60d0a1e]
The selected consumer controls below passed with zero failures and zero ignored.
This combined source contains the focused MCM/control/light changes and fresh
neighboring inventory work. It is not an isolated standalone-head run. Both
libtest targets compiled; all selected jobs ended and the build source was restored.
No pristine baseline failure, playable client, physical OS/device acquisition,
GPU/rendered, native or original installed-mod acceptance is established by this run.

`lights::tests`:13 passed, including the four authored session/lifecycle/emittance
and foreign-point binding controls plus existing light controls. This is CPU scene
and material state validation, not a rendered brightness/performance measurement.
