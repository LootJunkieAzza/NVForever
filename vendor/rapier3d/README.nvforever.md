# Rapier 0.36.0 collision-query refresh corrections

[documented] The baseline is the published `rapier3d` 0.36.0 crate, SHA256
`c2e2b20538584bb3ba5bfe6a66c2d9a242ee17d15e5398be0fa35279c7d73739`.
Its published VCS metadata names `b716d375efc0201003f0cd9ef7168eee0b62c177`.
All 326 published files are retained. Only the two Rust files listed below differ;
each carries a modification notice. The upstream Apache-2.0 LICENSE is included
from that same VCS revision. No global Cargo registry source is modified.

[design] NVForever runs a zero-duration physics step to register body changes and
refresh collision queries without advancing simulation time. Two bounded changes
repair the lifecycle of this supported step:

- `src/pipeline/physics_pipeline/substep.rs`: join a deferred broad-phase BVH
  optimization before the zero-duration early return. Otherwise the tree can
  remain temporarily empty and scene queries miss colliders. This is the same
  production change proposed by [Rapier PR #1067](https://github.com/dimforge/rapier/pull/1067),
  reviewed while that upstream PR was open; it is not claimed as an upstream release.
- `src/dynamics/island_manager/manager.rs`: after rebuilding persistent sleep
  components during bootstrap, use the existing `commit_sleeping_chunks` route
  for sleeping bodies still present in the awake island. Mixed components were
  already woken by the original bootstrap and remain awake. This keeps the
  persistent graph and active chunks coherent after query registration followed
  by an explicit sleep request. It does not suppress island assertions.

[observed] The isolated candidate passed all 114 unchanged fnv-phys synthetic
fixtures and ten focused query lifecycle controls. The earlier no-time candidate
failed 16 existing fixtures; adding the deferred join cleared thirteen, and the
bootstrap correction cleared the remaining three. The matching NVForever change
also removes the obsolete `set_position(current_position)` registration repair,
which erased pending kinematic targets, and clears the registration queue.

[design] Those CPU results do not qualify Android gameplay, GPU behavior, a full
canonical workspace run, or the existing animated-collision engine fixture. Those
checks are tracked separately by the Android issue. Package versions are retained;
both workspace locks are regenerated normally by Cargo for this local path patch.
