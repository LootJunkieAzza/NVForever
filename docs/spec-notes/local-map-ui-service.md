# Local-Map Images for Registered Mod UI Recipients

Part of #2418. This note records the typed CPU/ECS local-map request, sampling,
budgeting, and recipient-binding contract in `nvf_play::tiles::local_map_service`.
GPU local-mode rendering and projected-UI painting remain separate follow-up work.

## 1. Scope & Activation Policy

[design] The service is **off by default**: an empty registry performs zero allocations,
zero raycasts, zero image mutations, and zero UI writes per frame. No mod name (such as
`"ModernMinimap"`) and no shared placeholder texture (such as `Interface\Shared\solid.dds`)
activates the service.

[design] A user or profile adapter explicitly registers each recipient via
`LocalMapService::register(RecipientConfig)` with:

- A validated `RecipientId` (non-empty, `<= 128` ASCII bytes).
- Explicit UI paths `source_path` and `recipient_path` resolved to full generation-bearing
  `fnv_ui::TileId`s (`tree`, `index`, `gen`).
- Explicit `FieldBinding` trait names for evaluated request inputs (`active_trait`, optional
  `update_trait`, `window_width_trait`, `window_height_trait`, `scale_trait`, optional
  `center_x_trait`/`center_y_trait`, optional orthonormal `basis_cos_trait`/`basis_sin_trait`,
  optional explicit sampling-basis angle check) and plain writable sinks (`filename_sink`,
  `origin_x_sink`, `origin_y_sink`, optional `span_x_sink`/`span_y_sink`).

[design] Each admitted recipient owns one unique runtime image name
(`nvf:local_map_svc_<slot>` from a monotonic allocation counter) and one stable
`Handle<Image>` while active. No two active registrations may bind the same `recipient`
`TileId`. Only live leaf `image` tiles (`tile.kind == "image"`, no live children) are
admitted.

## 2. Session Epoch vs. Transient Loading Gate

[design] The service tracks a committed `SessionEpoch { load_epoch, tree }` combining
`script::Runtime::session_epoch` (incremented only after `Game::restore` commits; failed
loads leave it unchanged) and `ui.screen()` (the UI tree's unique root `TileId`).

[design] Transient loading (`PlayState::Loading` or `Loading::render_busy || Loading::actors_busy`
or `!phys.ready_at(player.feet)`) is gated separately from `SessionEpoch`: it transitions
recipients to `ProviderLoading` and pauses sampling/publication without creating a new
logical session. A committed `SessionEpoch` change cancels all in-flight jobs, removes
owned runtime images, and retires previous bindings so no tile in a new session or reused
numeric slot can inherit an old binding.

## 3. WorldNorth Sampling & Shared Global Budgets

[observed: repository source `engine/nvf-play/src/pipboy/data/outline/grid.rs`]
`OutlineGrid::checked` validates finite positive window/scale/center/feet, integer anchor
and dimension bounds, `Basis::WorldNorth`, pixel count, and prospective payload bytes
before allocating any buffer.

[design] Sampling reuses `pipboy::data::outline::Outline` (`Outline::from_checked`,
`Outline::advance`, `Outline::write_picture_into`) under one global `ServiceBudget`
shared across all registered recipients:

- `max_recipients`, `max_pending_jobs`, `max_resident_images`, `max_resident_pixels`
- `max_edge`, `max_pixels_per_image`, `max_payload_bytes_per_image`
- `max_rays_per_frame` (distributed round-robin in `max_rays_per_batch` chunks) and
  `max_time_per_frame` (elapsed wall-clock check between batches)
- `max_canceled_jobs` (bounded canceled-job accounting with full-cap backpressure)
- Bounded exterior terrain surface cache (`MAX_LAND_SURFACES`) cleared on world/cell change

[design] `EngineProvider` computes a static-geometry revision from
`(location.world, location.interior, phys.refs_built, phys.cells.len())` so actor movement
never thrashes local-map regeneration. Generated pixels always remain `Basis::WorldNorth`;
optional orthonormal `(cos, sin)` marker axes are applied via
`RuntimeImages::set_recipient_axes` without mutating XML `rotateangle` or texture revision.

## 4. Coherent Publication & Ownership-Conditional Restoration

[design] `local_map_service::system` runs `.after(PlaySet::Scripts).before(tiles::update)`.
When a job completes:

1. Texels are written in place into the recipient's stable `Handle<Image>` via a scoped
   `Assets::get_mut` guard, then `RuntimeImages` publishes a new revision (`set` on first
   bind or dimension change, `touch` on same-size texel refresh).
2. `Ui::set_existing_batch_if_unchanged` atomically writes `(filename, origin_x, origin_y`
   and optional `span_x, span_y)` using the recorded `TraitWriteSerial`s.
3. Because this runs immediately before `tiles::update`, dependent XML expressions evaluate
   against the exact matching origin/span on the same frame before normal painting
   (`tiles::draw`).

[design] If an external script overwrites any owned sink (advancing its `TraitWriteSerial`),
the conditional batch rejects without clobbering the external write, the recipient enters
`CompletedUnbound { reason: BindFailure::SinkChanged }`, and the owned runtime image entry
is removed. On cancellation, removal, deactivation, invalid request, tile replacement,
vanished provider, or session change, each sink is restored to its pre-binding value only
when its current `TraitWriteSerial` still matches this service's last owned write. The
`Handle<Image>` is removed from `Assets<Image>` only when `!runtime_images.uses_image(&handle)`,
preserving any independent foreign runtime alias.

## 5. Distinct Diagnostics & Remaining Limits

[design] `RecipientDiagnostic` keeps `Unregistered`, `Inactive`, `ProviderUnavailable`,
`ProviderLoading`, `InvalidRequest(RequestRejection)`, `Pending { sampled, total, key }`,
`CompletedUnbound { revision, key, reason }`, and `Bound { recipient, image_name, revision,
key, origin, span, size }` strictly distinct. No empty success stub substitutes for a
rendered, bound image.

[hypothesis] Budgets, outline presentation, and static-geometry revision heuristics are
NVForever engine policies, not claims about native DLL camera output, native plugin
versions, or GPU upload atomicity. GPU local-mode rendering tests remain separate
follow-up work.

## 6. Projected-UI Painter & 3D Model Surface Binding

[design] `nvf_play::tiles::projected_ui` (`ProjectedUiTargets`) consumes active
`fnv_game::ui_projection::Requests` (`ProjectUITile`) through the existing
`tiles::draw::paint` pipeline rather than adding a second generic painter:

- **Zero-work default:** when no `ProjectUITile` requests are active and no targets exist,
  `projected_ui::draw` returns immediately with zero allocations, zero cameras, and zero
  image allocations. Ordinary HUD and menu painting remain unchanged.
- **Pre-draw reconciliation and session isolation:** `projected_ui::reconcile` runs after
  `tiles::update` and before `PlaySet::Ui` to retire stale or shadowed sources permanently;
  `projected_ui::draw` tracks `SessionEpoch { load_epoch, tree }` and tears down all
  cameras, UI roots, and off-screen `Handle<Image>` assets on committed session rollover.
- **Isolated subtree layout & coordinate/clip validation:** `layout::layout_isolated` lays
  out only the subtree rooted at `request.source` without inheriting ancestor offsets,
  clips, alpha, or hidden flags outside `source`. `draw::projected_pieces` shifts rects and
  clips by `(-viewport.x, -viewport.y)`, rejects non-finite geometry, and intersects active
  clips with `[0..width, 0..height]`.
- **Shared runtime-image path:** `draw::paint` executes `runtime_images::mark` and
  `runtime_images::present` on the projected `Target::image(camera, ...)` target so
  `LocalMapService`'s in-place `Assets<Image>` mutations, `RuntimeImages` revision bumps,
  and `set_recipient_axes` marker rotations reach projected recipients identically to normal
  menu painting.
- **Bounded off-screen resources:** the painter independently enforces `MAX_TARGETS` (16),
  `MAX_EDGE` (2048), and `MAX_TOTAL_PIXELS` (8 Mi), reporting `ProjectionDiagnostic::RejectedBudget`
  distinctly on overflow, and frees old images on resize or exact `1x1` withdrawal.

[design] `nvforever::ui_project` (`UiProjectPlugin`) runs in `PostUpdate` after
`nif_mirror::MirrorSystems` to bind each active `ProjectedOutput.image` onto matching
`(reference, block)` mesh entities (`ShapeName`, `Name`, or skeleton node name,
case-insensitive, across both first- and third-person player rigs as well as world/actor
references) via an owned unlit `NifMaterial` copy. On withdrawal or target removal, the
original `Handle<NifMaterial>` is restored only when the entity still carries this module's
owned copy, and the owned copy is removed from `Assets<NifMaterial>`.

[design] In `LocalMapService::publish_completed`, if an external sink write races with a
completing rebind (`BindFailure::SinkChanged`), every unmodified sink whose
`TraitWriteSerial` still matches this service's last owned write is conditionally restored
before removing the runtime image so `filename` never remains pointed at a removed `nvf:`
entry.
