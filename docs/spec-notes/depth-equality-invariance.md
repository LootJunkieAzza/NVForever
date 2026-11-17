# Equal-depth position invariance

## Contract

[documented] WGSL's [`invariant` attribute](https://www.w3.org/TR/WGSL/#invariant)
requires vertex position results with matching data and control flow to agree across
programs and invocations. It applies only to the position built-in and has no effect
when the same declaration is used as a fragment input. Vulkan likewise permits
Invariant on Input or Output storage and ignores it on Input
([VUID-StandaloneSpirv-Invariant-04677](https://docs.vulkan.org/spec/latest/appendices/spirvenv.html)).

[documented] The SPIR-V representation uses BuiltIn Position and Invariant decorations
([SPIR-V specification, decorations](https://registry.khronos.org/SPIR-V/specs/unified1/SPIRV.html#Decoration)).

## Engine policy

[hypothesis, engineering requirement] Main passes selected by `nvf_assets::depth_equal`
retain Equal comparison and disabled depth writes. Both the depth writer and main
pass declare invariant vertex positions through the vendored Bevy prepass and forward
`VertexOutput` structs. Their position calculations are unchanged. NIF/terrain use
Bevy's mesh vertices; grass and tree use their own paired vertex paths with the same
shared output structs. The attribute cannot make different transforms, deformation,
inputs, depth bias or fragment-written depths agree.

The application warning identifies entry point `vertex` and Equal depth comparison,
but has no material or pipeline identifier. This change addresses the missing
invariance contract of the traced mesh families. It does not establish the cause of
any particular visual symptom or cover Bevy's separate deferred/meshlet Equal paths.

## Validation

`engine/nvf-shader-tests/tests/depth_invariance.rs` composes actual checked-out WESL through
Bevy's `ShaderCache` with a CPU callback, validates with the renderer's Naga version,
and checks vertex-output invariance plus the SPIR-V Position decoration. Controls
cover standard, skinned, morphed and compressed mesh paths; grass/tree main and
prepass paths; an unannotated baseline; independent removal of each producer's
attribute; and the shared structs as fragment inputs. No render device is created.
The focused test package depends directly on `bevy_shader`, `bevy_asset` and the
same Naga version; it does not depend on the Bevy umbrella or `nvf-assets`. This
keeps selection of the compiler tests independent of Bevy's render/PBR/runtime
dependency chain. It still reads the production shader sources in place.

An integrated CPU run of the earlier setup passed the baseline-rejection,
independent missing-producer/retained-producer, and shared-fragment-input controls.
Both positive tests failed before the invariant assertions: the depth-only morph
prepass had normal/tangent flags for fields its IO did not expose, and grass loaded
the raw Bevy PBR functions without the production display-lighting preparation.
These three passes and two failures do not establish native or visual acceptance.

The corrected fixture follows the actual prepass specialization: normal/tangent
attributes occur in normal/deferred prepasses, while depth-only prepasses omit them.
Morph, skin and compressed families remain, with explicit normal-prepass coverage
added. The shader registry also uses the same pure display-source helper as
`LightsPlugin::patch_display_sum`, extracted into `lights/display_shader.rs`; the
runtime retains the same ordered edits and its `lights::unix_lines` API. The test
does not supply a copied lighting shader or depend on the renderer. All five
selectors and the strict invariance/SPIR-V/negative/fragment oracles remain intact.

From `engine/`: `cargo test --release --locked -p nvf-shader-tests --test depth_invariance`.
No extra feature flags or environment variables are required. A subsequent Windows
CPU run on an integrated source cohort compiled and executed each of the five strict
controls once: five passed, none failed or were ignored. The test, display-source
helper, package manifest and both position-output shader blobs matched this change.
The positive mesh/prepass and grass/tree cases completed the actual ShaderCache,
WESL, Naga validation and SPIR-V checks; the baseline rejection, independent missing
producer and shared-fragment controls also passed. This qualifies that source cohort
and CPU test configuration, rather than a standalone build of this public head.
The earlier three-pass/two-failure setup result remains part of the history.

State: focused Windows CPU compiler controls passed on matching integrated sources.
Runtime-helper unit checks, runtime scheduling, Clippy/Cargo deny, normal-client and
whole-graph builds, shader-driver validation, performance measurement and rendered
acceptance remain unrun by this focused target. No timing improvement, warning
disappearance, native parity or visual repair is established. The ordinary engine
test check discovers this Cargo integration test.
