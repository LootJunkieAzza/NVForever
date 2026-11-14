# Collision query refresh and registered bodies

- [observed: owned public CPU fixture, phys-arrival-active-island-repro-20261114-r1]
  A world with one previously stepped dynamic body, a fixed support, and a newly added
  touching dynamic body reaches actual contact detection with two awake dynamic bodies
  but only one registered active body. The cached release-profile regression fails its
  population assertion; the unchanged-body neighbor passes. No game/device run occurred.
- [documented: pinned rapier3d0.36.0 PhysicsPipeline::step docs and substep.rs]
  A zero-dt pipeline step applies user changes with the real IslandManager and joints,
  performs contact detection, then returns before solver/integration/CCD. CollisionPipeline
  instead handles body changes without island registration. That distinction matters
  once contact notifications use an already populated manager.
- [hypothesis: bounded repair] fnv-phys query refresh uses the documented zero-dt path
  with a copied IntegrationParameters, actual world fields and the existing ragdoll
  hooks. The persistent dt and fnv-phys step counter are not changed. No positive step,
  scratch manager, invariant suppression or dependency patch is introduced.
- [hypothesis: qualification] Actual pose/velocity/queued-force, sleeping and struck-body,
  jointed and insertion/removal neighbor controls must qualify this change. A cached
  dependency with debug assertions disabled can establish the registration regression
  without claiming the exact debug-only Android panic was executed. Full arrival/load,
  device/game/native behavior, all physics suites and performance remain separate.

- [observed: owned authored CPU controls, 2026-11-14] The candidate registers both awake
  dynamic bodies (2/2) where the unchanged baseline registers only one (1/2). Five
  public-API controls and two internal-state controls passed, covering new contacts,
  unchanged neighbors, sleeping clutter, pushed-body motion, joints, queued force,
  configured step size and disable/re-enable. The cached Rapier dependency has debug
  assertions disabled, so this establishes the registration invariant repair without
  claiming the exact Android assertion was executed. The Android integration retains
  the reviewed runtime and all seven authored tests unchanged.
