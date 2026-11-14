# Explicit script renames of skeleton roots

- [documented] RenameNifBlock changes a named block of the calling reference's
  loaded model; its optional JIP player selector addresses separate player rigs.
  Source: <https://geckwiki.com/index.php?title=RenameNifBlock> and the parameter
  catalogue in `schemas/script/functions.json`. Root-specific native behaviour
  has not been measured by this candidate.
- [documented] That page says the rename resets on parent-cell reload or Update3D.
  The entity-owned marker does not by itself qualify every engine rebuild/reset
  route; it establishes only that a replacement entity does not inherit the marker.
- [documented: repository `engine/nvforever/src/player_body.rs` at 3ccc75a3] PlayerBody labels the root entity with a factory
  Name such as its body/view label (`player_body.rs`, build_rig). The actual NIF
  root name is kept in the rig's skeleton map. `nif_mirror::name_of` deliberately
  prefers the skeleton name for this root, preventing the factory label from
  becoming a script-visible NIF node name.
- [documented: repository `engine/nvforever/src/nif_mirror.rs` at 3ccc75a3] The Rename edit only changes the entity's Name.
  On a skeleton root, the next publication still prefers the old skeleton name.
  Its explicitly renamed Name therefore cannot be found or retained by the
  mirror, unlike the Name of a non-root node. The initial authored CPU comparison
  below also reproduces this route; native root-specific parity remains unmeasured.
- [design] Keep factory labels and skeleton names distinct. A successful explicit
  script rename marks that entity as script-renamed. Only a marked entity's Name
  takes precedence over the skeleton-root name. Unmarked roots keep the original
  skeleton-name rule; ordinary non-root Name lookups keep their existing rule.
- [design] The marker belongs to the actual entity. A rebuilt or despawned model
  does not inherit it by FormId or by a matching bone name. This change does not
  otherwise decide held-transform or material-cache lifetime across rebuilds.

Authored CPU controls use real PlayerBody rigs with distinct factory labels
and skeleton root names; rename first and third roots separately, flush queued
commands, and check subsequent publications and lookup through the new name.
An ordinary actor rig and non-root rename are controls. The source VM's dynamic
root-name argument is separately exercised. Asset loading, GPU rendering, native
timing/parity, same-frame structural edit ordering and Loop255 remain unqualified.

The optional query-job smoke discovers both actual root names through the existing
query API, uses authored replacement names, and waits an intervening query frame
before checking later publication. It has an earlier watch query before the job.
Its printed positive checks and zero-error job summary are authored, not yet run.

A separate authored CPU control loads that exact owned JSON with include_str and
serde_json, uses the real query_steps/run_step consumer for all six source strings,
and advances the real mirror producer between steps. It requires the return sequence
[1, 0, 1, 1, 0, 1] and no ScriptError. This qualifies only the source strings and
producer-step interaction when executed; it does not run QueryJob's scheduling,
logging/output writer, the installed game or a GPU. It executed and passed in hosted b24124e2 CI (run 43557253676); it is not a run of this corrected source tree.

[observed: initial authored CPU controls] Keeping the same initial three producer
fixtures and replacing only the baseline production file at 3ccc75a3 gives one pass
and two intended assertion failures. The explicit-rename marker gives three passes;
the four existing mirror helpers also pass. This predates the additional exact-JSON
consumer fixture. Hosted b24124e2 CI subsequently executes that consumer among its
13 passing producer controls. The actual optional query-job smoke remains pending;
neither historical result qualifies the corrected cold-write source tree.

This is a provisional follow-up to the per-view implementation under #2387.
Complete armor-HUD behaviour, native parity and the excluded lifetime/order routes
remain separate.
