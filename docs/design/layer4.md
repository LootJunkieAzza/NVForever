# Layer 4 design: core simulation

Spec section 5 Layer 4 ("entity model (forms and references), save-agnostic game state,
collision and character controller, basic physics, activation, containers, inventory")
and milestone M4 ("player controller, collision, activation, inventory"), in the section
9 format. Status 2026-10-07: **started**. The headless simulation core (forms, references,
inventories, leveled lists, activation, delta state, scripted traces) is implemented and
runs on the owner's install. Collision and the character controller are designed (§2.8)
and come next.

## 1. Summary and assumptions

Task: the part of the engine that makes the static world (Layer 3) interactive, kept
**headless and deterministic** (spec section 7) so every behaviour can be tested without a
renderer and compared with captures from the original game.

Assumptions and decisions (2026-10-07):
- **Developed in parallel with Layer 3** (another session works on world rendering). To
  stay out of its way, Layer 4 lives in its own crate (`fnv-sim`), touches no rendering,
  viewer or cell-streaming code, and changes shared files only by small additions
  (workspace member, `fnvtool sim` subcommand, one `pub(crate)`). After Layer 3 merged,
  the placed-reference table was rebuilt on Layer 3's `fnv_world::WorldIndex` (§2.3).
- The core does **not** use an ECS (§2.2).
- Behaviour that the data cannot show (what activation does, how leveled lists roll) is
  implemented as clearly tagged **[hypothesis]** and isolated behind small functions, so
  the in-game captures of docs/spec-notes/simulation.md §6 can correct it.

## 2. Findings and design

Facts and their tags: `docs/spec-notes/simulation.md`. Summary of what the data showed
[observed, ten masters]: 427,022 placed references; the persistent flag 0x400 matches the
persistent-children groups exactly; the player base is NPC_ 0x7 and no plugin places the
player reference 0x14; locks occur only on doors and containers; 1,376 of 1,378 teleport
doors are reciprocal pairs; 105 carried-item entries have count 0; 1,492 references use
engine-reserved bases 0x17/0x20 (occlusion planes and portals: Layer 3 culling).

### 2.1 Crates

| Crate | Role | State |
|---|---|---|
| `fnv-sim` | forms, references, inventories, leveled lists, activation, delta state, action traces | **done (first cut)** |
| `fnv-collide` | Havok collision blocks from NIFs → engine-neutral shapes in game units | next (§2.8) |
| `fnv-phys` | rigid bodies, queries, character controller over a physics engine | after `fnv-collide` |

`fnv-sim` depends on `fnv-esm` and `fnv-world` (plus serde, rand_chacha, rayon). No
Bevy, no renderer: the Bevy engine will drive it, never the reverse.

### 2.2 Entity model: plain ordered maps, not an ECS (decision)

- **Forms** (`FormStore`): the winning record of each FormID, interpreted for the record
  types Layer 4 needs (items with value/weight, containers, doors, activators, furniture,
  actors' carried items, leveled item lists, globals); every other winner kept as its
  signature so any reference can be classified.
- **Placed references** (`RefTable`): read-only authored placement (base, cell, position,
  rotation, scale, count, owner, lock, teleport, enable parent, linked reference).
- **Runtime state** (`World`): `BTreeMap<FormId, RefState>` of *materialised* references
  only (§2.4), plus created references (FormIDs from 0xFF000000).

Rejected for the core: `bevy_ecs` (ties the simulation to Bevy's release cadence, which
ADR-0001/0002 deliberately confine to the `engine/` workspace; query iteration order is not a stable
contract, which determinism needs) and `hecs` (no benefit at this stage: there are no
per-frame systems iterating thousands of entities yet). Revisit when AI and physics need
per-frame iteration over many actors (Layer 6); the Bevy app mirrors `World` into ECS
entities through an adapter rather than owning the state.

### 2.3 Loaders read through the Layer 1 schema

Fields are looked up by the names `schemas/fnv` gives them (`extract.rs`), so layout
corrections need no code change, and anything skipped is counted in an issues list
("nothing silently dropped", spec section 7). Which references exist and which cell each
is in come from Layer 3's `fnv_world::WorldIndex` (`RefTable::from_index` lets the engine
share one index); `fnv-sim` adds count, owner, lock, teleport and linked reference.
Switching to the shared index left every `sim forms` statistic and the Goodsprings trace
unchanged [observed]. Full load of the ten masters (forms, index and all 427,022
references) takes about 0.8 s [observed, release build].

### 2.4 Save-agnostic state

A reference nothing has touched has no runtime entry; the first access derives it from
the placement data (initial inventory: carried items with leveled lists resolved; lock;
enabled flag). A `Snapshot` (`fnv.sim.state/1`) holds exactly the materialised
references, the RNG position and the next created FormID, so it is the set of changes
against the plugins and independent of the original savegame format (savegame
compatibility is Layer 7). Restoring a snapshot and continuing gives the same result as
an uninterrupted run (tested).

### 2.5 Inventory

One stack per base item with a count (`Inventory`, ordered). Per-instance extra data
(condition, ownership, weapon mods, equipped state) becomes a list of extra-data entries
per stack when Layer 6 needs it. Weight/value totals report items whose layout is not yet
in the schema (NOTE, IMOD, CHIP, CCRD, CMNY) instead of guessing.

### 2.6 Leveled lists

Rules in docs/spec-notes/simulation.md §4, all [hypothesis]. One seeded ChaCha8 stream
(`rand_chacha`, reproducible across platforms and versions by that crate's contract)
makes runs repeatable; matching the original game's outcomes is a *distribution*
question (spec section 8.1), so these get distribution scenarios once that comparator
exists.

### 2.7 Activation

`World::activate(actor, target)` dispatches on the target's base type (item → pick up,
container → open, door → teleport or toggle, actor → talk, activator/furniture →
activated), after checking enabled/deleted state and locks (a carried key unlocks first).
The base form's script is reported for Layer 5's OnActivate. Every rule is [hypothesis]
until captured; the outcome is a value, not a side channel, so the trace shows it.

### 2.8 Collision, physics and the character controller (next)

1. **`fnv-collide`**: convert the Havok blocks that Layer 2 already decodes at 100%
   (`bhkCollisionObject` → `bhkRigidBody(T)` → shapes: MOPP tree over packed triangle
   strips, convex vertices, box, sphere, capsule, list, (convex) transform, NiTriStrips
   shapes) into engine-neutral shapes in game units, with body transform, collision
   layer, motion type and mass. No physics engine dependency, so it can be checked
   against an independent oracle like every other format.
2. **Havok scale**: the factor between Havok units and game units is commonly given as 7
   for this engine generation [hypothesis]. It will be *measured*: for every mesh with
   both, compare collision-shape bounds with render-geometry bounds; the ratio
   distribution must peak sharply at one value.
3. **`fnv-phys`**: `rapier3d` with `enhanced-determinism` (pure Rust, cross-platform
   determinism) — preferred to Jolt bindings (C++ build and FFI) and to `avian3d`
   (Bevy-coupled). Static world: placed references' collision plus terrain from LAND
   heights (shared with Layer 3). Character controller: a kinematic capsule with step
   height, slope limit and gravity; the original values are unknowns to capture (§5).
4. **Scenarios**: shape statistics per archive against an oracle (exact with tolerance);
   later, player position after scripted movement against in-game captures.

## 3. Code, schema and tests

- `crates/fnv-sim`: `form`, `extract`, `forms`, `refs`, `inventory`, `leveled`, `world`,
  `actions`. Tests: unit tests (FormID text, inventory incl. a conservation property
  test), `tests/synthetic.rs` (a plugin built in memory and decoded with the repository
  schemas: loaders, locks and keys, pick-up/drop, teleport/toggle doors, errors,
  leveled lists, uniformity of the RNG helper, action trace, snapshot resume).
- `fnvtool sim forms | containers | run` (`crates/fnvtool/src/sim_cmd.rs`); contract in
  docs/spec-notes/simulation.md §7.
- Independent oracle: `tools/capture/oracle_sim.py` (containers), written from the spec
  notes only.
- Parity lab: `parity-lab/scenarios/layer4_sim.json` — `L4.sim.containers` (keyed vs the
  oracle) and `L4.sim.trace.goodsprings_doc_mitchell` (observations vs an in-game
  capture; `needs-local-capture`); action scripts in `parity-lab/inputs/layer4/`.

## 4. Validation plan and exit criterion (M4)

1. Loaders: zero unexplained issues on the ten masters; every anomaly documented in the
   spec notes — done for this cut (remaining issues are schema gaps, listed in §5).
2. Static contents: `L4.sim.containers` against the independent oracle.
3. Behaviour: action traces against in-game captures (Goodsprings first, then a locked
   container with its key, a lock without key, a teleport pair, dropping and re-taking).
4. Leveled lists: distribution scenarios (needs the distribution comparator).
5. Collision: shape conversion vs oracle; Havok scale measured; then movement traces.
6. M4 done when the player can walk the Mojave (Layer 3 world) with collision, open
   doors and containers, and take items, with the scenarios above passing.

## 5. Open questions and verification steps for the owner

1. **In-game capture** for `L4.sim.trace.goodsprings_doc_mitchell` (procedure in
   docs/spec-notes/simulation.md §6). Can you run console commands on a pinned save in
   Doc Mitchell's house, or do you prefer that we build the xNVSE logging plugin first?
2. What happens in-game when activating a locked container *with* its key in the
   inventory: does it open directly (our hypothesis) or prompt?
3. Count-0 carried items on NPCs (e.g. the Vault 21 gamblers): do those NPCs carry
   anything for those entries?
4. Schema gaps that block weight/value: NOTE (no field), IMOD, CHIP, CCRD, CMNY (no
   layouts yet) — Layer 1 work queue.
5. Placed grenades (PGRE, 297 in OldWorldBlues.esm) are not simulated yet: are they
   live explosives in-game, or decoration?

## 6. Updated spec notes

`docs/spec-notes/simulation.md` (new): forms and references, the player, items and
carried items, leveled lists, locks/doors/activation, capture procedure, CLI contract.
