# Co-op: engine-level replication (design proposal)

Status: **proposal for the owner's decision** (2026-11-07). Nothing here is built yet.
Private co-op fork only.

## 1. Why

Today each player's machine runs its own full copy of New Vegas, and the host sends across
a growing list of special cases: NPC poses, enabled flags, doors, campfires, the clock,
speech lines, quest view and others. Anything not on that list is decided separately on
each machine, so it can differ. These overnight-queue items are all that one fault:

| Item | Symptom |
|---|---|
| 28, 71 | NPCs look different |
| 38 | Freeside thugs missing on the guest |
| 47 | Genaro has two heads |
| 56 | Luigi's waiter |
| 54 | Fires |
| 55, 69 | Radio drift |
| 33 | Subtitles |
| 22 | Bottles |
| 58 | Furniture |

Each one is fixed by adding another special case.

The goal is **Dying Light-style co-op at engine level.** The host's game is the one world.
A guest's game is a view of it, plus the guest's own player. Bandwidth and processing stay
small enough for home connections and the same PCs that run the game today.

## 2. Hard requirements

- **R1. Solo is unchanged.** With no lobby open, no new code path runs, and saves stay
  byte-identical.
- **R2. Scripts act exactly as they do now, on the host.** Every quest, object, effect and
  extender script keeps running in the host's `Game::tick` (game.rs:3394), unchanged. Its
  results become the world every player sees. No script is rewritten for co-op, and mod
  scripts (Qwib, Burning Campfire, JIP/NVSE functions) work as they do solo.
- **R3. A guest's game runs no world logic.** That means no quest, object or OnLoad
  scripts on world refs, no AI, no leveled rolls for world refs and no spawns. Its world
  state only ever changes because the host said so. The guest's own player still runs
  locally: movement, camera, its own inventory and Pip-Boy, menus. So do cosmetic things:
  particles, debris physics, footsteps.
- **R4. Bandwidth:** about 30 KB/s per guest in the busiest cell, near zero in a quiet one.
  A join is under 1 MB.
- **R5. It ships in phases.** Every phase is shippable alone and removes a class of desync
  bugs. Nothing is a big-bang rewrite.

## 3. Where we are (survey of claude/morning-build, 2026-11-07)

**Transport.** One UDP socket on its own thread (nvf-coop net.rs), with optional Steam
Networking Messages. **There is no reliable channel.** One-off events are sent three
times (`HIT_COPIES`) and de-duplicated, and state is re-sent until it lands.

**NPC traffic.** `Npcs` resends every drawn NPC in full at 10 Hz:

- each is 41 bytes, as raw f32 positions with no quantization;
- there is no delta encoding;
- 29 NPCs fit in a packet.

**Guest world logic still running.** `Game::tick` has no co-op gate, so the guest runs
quest GameMode, object scripts, OnLoad, carried-item scripts and OnOpen/OnClose. Then the
host's state overwrites the results afterwards: presence, doors, positions, deaths,
speech. AI is off on the guest (`actors_ai_off`).

Still local on the guest:

- leveled loot;
- PlaceAtMe;
- script sounds and music;
- its own speech.

**Randomness.**

- Actor looks and template inventories are already seeded per reference (ChaCha8 from the
  FormID; fnv-actor resolve.rs:310, templates.rs:88).
- Container and actor loot, leveled AddItem and death items use the world's single
  sequential RNG (fnv-sim world.rs:681, :903, :956), so results depend on call order.
- `GetRandomPercent` and many others use `Game::rng`, which is also sequential.

**Change tracking.**

- Save data is already a delta against the plugins: `fnv_sim::World.refs` holds only the
  references that were materialised.
- There is no dirty flag, though, so touching a ref materialises it.
- `Game::emit(EventKind)` (game.rs:2269) logs most script-visible changes: StageSet,
  RefEnabled/Disabled, ItemAdded/Removed, Moved, Placed, SoundAsked, MusicAsked,
  LineStarted, AnimAsked and others. Much code still mutates `World` directly without
  emitting.

**Already right.**

- Guest hits go to the host (`RemoteHit`), and stand-ins make the host's AI treat guests as
  players (`as_player`).
- Guests' own saves are kept apart (`KeepSolo`/`SoloBase`).
- The leash keeps the guest inside what the host has loaded.

## 4. The model: who owns what

| Thing | Owner | Guest's game |
|---|---|---|
| World refs: placed, created, enabled, position, open/locked, health/dead, inventory, scale, owner | Host | Applies replicated changes only |
| Quests, stages, globals, script variables | Host | Receives what it needs to show: the quest view, globals that the HUD or menus read |
| AI, packages, combat decisions, detection | Host | Off (as today) |
| Leveled resolution, spawns, loot | Host | Off; seeded rolls cross-check it (§5) |
| Script side effects: sounds, music, speech, fades, messages, imagespace, camera shakes | Host runs the script | Plays the replicated event |
| The guest's own player: movement, look, weapon, its own inventory, perks, AVs, Pip-Boy, menus | Guest | Local; the host gets the state (today's `State`) |
| The guest's actions on the world: activate, take, open, pick lock, hit, talk, sit | Requested by the guest, decided by the host | Shows the result when the host confirms |
| Cosmetics: particles, debris, ragdoll settle, footsteps, ambient loops from replicated state | Each game | Local, never replicated |

Scripts that name `player` still mean the host's player, as now. A guest's action that runs
a script reaches it as its stand-in actor (`coop_actors`, `as_player`), which the AI and
conditions already treat as a player. That is the one place co-op touches script meaning,
and it already exists.

## 5. Layer 1: determinism (zero bytes for what both can compute)

Both games load the same plugins, so every placed reference already exists with the same ID
and base data. Rolls that decide **what everyone must see the same** become a pure function
of a seed both sides share:

```
roll_seed = hash(world_seed, ref_formid, roll_kind, roll_counter_for_that_ref)
```

- `world_seed`: sent once at join. It's part of the host's save.
- `roll_kind`: actor pick, outfit and worn items, visible death items (a dropped weapon).
- `roll_counter`: bumped per ref and kind on each re-roll (respawn, re-arm). It's kept in
  the save, as `rolls` and `roll_seed` already are for resurrection.

The level a leveled list reads becomes host data. `ref_levels` and `zone_levels` are
already in `Snapshot`, so replicate those values and don't let the guest compute them.
Actor looks are already seeded per reference (fnv-actor resolve.rs:310), so this mostly
adds the host's level and roll count, which df9afc2e9 already sends.

**Loot is not part of this (owner decision, 2026-11-07).** It stays as it is today:
per-player and rolled the way it is now. See §6.4. The sequential world RNG for container
and corpse loot (world.rs:681/903/956) is left alone. Solo is unchanged (R1), and existing
saves roll exactly as before.

`Game::rng` for `GetRandomPercent` etc. does **not** need to change. World scripts only run
on the host (R3), so only one sequence exists.

**What this buys:** with Layer 2 off, the guest would already draw the same NPCs and
outfits. Layer 2 then only has to correct the rare case where the host's world differs
from the plugins.

## 6. Layer 2: change replication (the core)

### 6.1 A change log in the world

Add a dirty set to `fnv_sim::World`: each mutation of a ref's replicated fields marks
`(ref, component)` dirty, with a per-ref version number. The components are:

| Component | Fields |
|---|---|
| `Exists` | created / deleted, base, `next_dynamic` FormID (created refs keep the host's FormID, as `create_as` does now) |
| `Enabled` | enabled flag |
| `Place` | cell/worldspace, position, rotation, scale |
| `Open` / `Lock` | door/container open, lock level, locked |
| `Life` | dead, health fraction, knocked out, unconscious, essential |
| `Look` | leveled pick result: base, sex, race, face seed, worn items (only when it differs from the seeded roll) |
| `Inventory` | **not replicated** for containers and corpses (per-player loot, §6.4); only the host-script and quest-item exceptions |
| `Owner` | ownership and crime flags |
| `Anim` | the furniture the actor sits in, the idle playing, the current package's visible state (sitting, sleeping, leaning) |
| `Pose` | a dead actor's body as it lies at rest on the host: per ragdoll body, its bone's place and turn (protocol 130). Sent with the actors the host draws or simulates, so a guest that comes by later lays the body down there at rest instead of dropping it from a death pose of its own (playtest 119 items 145, 161) |

The mutation points are the setters in fnv-sim and the existing `emit` sites. A test walks
every `World` setter and fails if one doesn't mark dirty. That turns today's gap
("mutated directly without emitting") into a build error, not a desync.

Solo: the dirty set is a no-op unless a lobby is hosting (R1).

### 6.2 Reliable channel

Add a small reliable, ordered stream over the existing socket (sequence + ack bitfield +
resend, about 150 lines; or the Steam reliable flag). Changes and events go on it. Poses
stay unreliable (newest wins). This removes the `HIT_COPIES` triple-send.

### 6.3 What goes over the wire

- **Changes:** each tick (20 Hz), the host sends each guest the dirty components the guest
  hasn't acknowledged, for refs in the guest's interest set (§6.5). Each is `ref (u32) +
  component (u8) + version (u16) + payload`, typically 4-16 bytes. A ref that changed five
  times in one tick sends once.
- **Moving actors** (the hot path) get a compact stream, replacing `Npcs`:
  - position quantised to 1 cm relative to the cell origin: 3 × i24, 9 bytes;
  - yaw as u8 or u16 and pitch as u8;
  - a flags byte;
  - the animation state as clip id + start tick.

  That's about 16 bytes per actor update, against 41 today, and it's only sent when the
  actor moved or its state changed. Standing NPCs cost nothing.
- **Rates by priority:** in the guest's view and within about 20 m, 20 Hz; in view farther
  off, 10 Hz; out of view or beyond about 60 m, 2-5 Hz. The guest interpolates as now
  (0.2 s delay).

### 6.4 Items: "every item is an item", and loot is per-player

**Loot stays per-player, as it is today (owner, 2026-11-07: "players shouldn't have to
fight over loot").**

- Container and corpse contents are rolled in each player's own game, the way they are
  now. Each player loots their own copy.
- A guest emptying a footlocker doesn't empty it for the host, or the other way round.
- Loose items lying in the world (a bottle of Nuka on a shelf) are per-player too: what
  one player picks up is still there for the others to take their own.
- None of this is replicated, so it costs no bandwidth.

**Exceptions, shared because the story depends on them:**

- **Quest items** (flagged quest item, or the target of a quest objective). There is one
  copy, on the host. Taking it is a request to the host (§8), so the host's scripts see it
  taken. It then disappears for everyone, and the item goes to the player who took it.
- **Items a host script adds or removes** in a container or the world after load (AddItem
  or RemoveItem on a ref, PlaceAtMe of an item). These replicate as `Exists`/`Inventory`
  changes, because a script put them there for a reason. Each player then takes their own
  copy, unless it's a quest item.

- **Items a player drops** (owner, 2026-11-09, queue item 135: "any items players drop
  should be visible for all players, and if a player picks up a player dropped item it
  goes away for all players"). Hand drops, scripted drops and a weapon lost from the
  player's hand are marked (`RefState::player_drop`). There is one copy, in the host's
  world, and the host's save keeps it. Every player sees it (`Value::Item`, then its
  `Place`), and its `deleted` flag goes to everyone.
  - A guest's drop goes to the host once it lies still (`Loose::Dropped`). The host makes
    it and sends back its number (`Loose::Made`), and the guest's own copy takes that
    number.
  - A guest's pickup happens in its own game at once and is asked of the host
    (`Loose::Took`). The host's world decides who was first. A guest who was not first
    gives the item back (`Loose::Refused`), so nothing is doubled.
  - Not carried: the dropped unit's condition and mods (the host's copy has the base and
    count only). NPCs' drops stay per-player loot.

**Never sent:**

- **Clutter at rest.** A can that has never moved is still the plugin's can, and both
  games have it.

**Sent once:**

- **A clutter object knocked by physics** sends its resting `Place` once, when it settles.
  The flight itself is simulated locally on each game as a cosmetic (it starts from the
  same impulse event). Only the settle is authoritative. Knocking things over is world
  state; taking them is loot.
- **NPCs' worn items** go in `Look` (only what's visible). What they carry is loot, so it's
  per-player once they're dead.

### 6.5 Interest set

A ref is in a guest's interest set if it's in a cell the guest has loaded. The leash means
that's the host's loaded area anyway. Persistent refs that the HUD, Pip-Boy or quest
markers need are also in it, such as quest targets and companions. Refs leaving the set are
dropped by the guest; re-entering sends their current state once.

### 6.6 Join

At join, the host sends a snapshot of the guest's interest set: the `Snapshot` deltas for
those refs, plus globals, quest stages and the clock. It's built from the same code as a
save, filtered to the loaded area and compressed, and sent on the reliable channel during
the guest's loading screen. A full NV save is a few hundred KB, and this is a part of one.

## 7. Layer 3: script effects as events

Scripts run once, on the host (R2). Their visible side effects become events on the
reliable channel, stamped with the host's game time so each guest plays them in sync. All
of them are already `EventKind` or `ScriptEffect` values:

| Event | Payload | Today on the guest |
|---|---|---|
| Sound / music / radio | sound form, ref or position, start time; music type; radio station + stream position | Runs its own copy, which drifts (items 55, 69) |
| Speech | speaker, INFO + response, start time | `Lines` at 4 Hz, partial |
| Anim | actor, idle/clip, start time | Partial (`Anim` for items only) |
| Fade, imagespace, camera shake, cinematic camera | effect, duration, target players | Not sent (item 66) |
| Message box, notification, quest message | text id, buttons, **which players** | Host's quest view only |
| Effect visuals (shaders, explosions, projectiles) | effect, ref or position | Partly |

**Which players see a player-facing effect.** Scripts address `player` (the host). For
co-op, the rule is a table per effect kind:

- World effects (sound at a ref, an explosion) go to everyone in range.
- Story presentation (fades, cinematic cameras, DLC intros, quest messages) goes to
  everyone, as Dying Light does for the host's story.
- Menu-opening effects (barter, container, lockpick, message box with buttons) go only to
  the player whose action caused them. If a script opened them on its own, they go to the
  host.

The same table settles item 66: cutscene cameras and hiding the players apply to everyone.

### 7.1 Story moments wait for every player (item 133)

Owner, 2026-11-09: "testing host not being able to talk to important NPCs when guests
are too far"; "It shouldn't be a choice the host should have to wait"; "It should be a
prompt like the duel prompt on the hosts end it'll say waiting for players, and then
give guests only the option to teleport while they're not in a menu already and if they
are it'll come up once that menu closes".

- **Story NPC**: the speaker of a line of a story quest (FalloutNV.esm's main quest by
  its Editor ID prefixes, or an add-on's NVDLC0nMQ… quest), or of a line whose result
  scripts name one in their compiled reference list, while that quest is not completed
  or failed (`fnv_game::Game::story_npc`). Jed at Zion counts through HonestHearts.esm
  INFO 0x0100F195, which starts NVDLC02MQ01.
- **Within sight**: 2000 units in the same space (`fnv_game::coop::STORY_NEAR`), for both
  the gate and a script's move.
- **The gate** (`Game::story_gate`, host only): the host's talk with a story NPC, and a
  story NPC's Dialogue package (force greet or Say To), wait while any other player is
  not within sight of the NPC. A script's StartConversation is not held.
- **A script's move of the host** (a DLC start, MoveTo) takes the guests within sight
  along, 72 units to the host's right (not on its feet: that pushed the host out of
  Jed's 250-unit greeting). Guests not within sight are left where they are, and the
  host waits for them until each is near (`Repl::wait_for`). The move itself is not held
  (a MoveTo runs mid-script).
- **The prompts** are the co-op invitation's banner (`nvf_play::coop_social::StoryPrompt`):
  the host's "WAITING FOR PLAYERS..." with no keys; a far guest's "<HOST> IS WAITING FOR
  YOU" with only "Y) Teleport to the host" (refused in combat), which neither lapses nor
  can be declined. A prompt shows only while that player is in `PlayState::Playing` and
  no script menu or loading screen is up: a menu holds it until it closes; an
  invitation goes first. The console is not a menu here: it is a debug overlay, and the
  prompt stays up (and Y works) under it [decision, 2026-11-09].
- **A far guest is locked too.** The host's scene locks (DisablePlayerControls) reach
  every guest, far ones included (owner, 2026-11-09: "that shouldn't matter because they
  should always be with the host during those scenes"); the teleport prompt's Y is read
  while the controls are locked (laptop run story-far-3b). The locks are the host's
  state now, sent every 0.5 s (`SceneView::locks`), and a host not heard for 3 s holds
  nothing, so EnablePlayerControls (or a lost host) lets every guest go.
- **Only story locks reach the guests** (owner, 2026-11-09: "only story scenes"). A lock
  is a story lock when the script that set it runs for a quest (a quest's script, a
  stage's result, a dialogue result) or is a trigger volume's (a placed reference whose
  script has OnTriggerEnter, OnTrigger or OnTriggerLeave: the level designers' story
  triggers, the DLC intros, the slideshows) (`Game::is_story_caller`, kept per control
  group in `Game::story_locked`; a plugin's Alt lock counts too). A lock set by the script
  of something the host uses (ED-E's repair message boxes, an activator, a terminal) and
  the host's own menus hold the host alone (laptop run q-ede-repair-2p held the guest for
  7 s before). EnablePlayerControls still lets every group go, and the 3 s rule stands.
- **Not held**: add-on starts that are no conversation (the DM signal, the Lonesome Road
  tunnel): the far guests are left behind and waited for, but the start itself goes on.
- Log: `co-op: story wait: …`; checked by the laptop runs story-near-3, story-far-3 and
  story-far-3b (2026-11-09); the menu hold by the unit test
  `a_menu_holds_the_teleport_offer_until_it_closes`.

## 8. Layer 4: guest actions go through the host

A guest's input that changes the world is a request. The host runs it as the guest's
stand-in actor, through exactly the same code (and the same script blocks: OnActivate,
OnAdd, OnEquip, OnHit and so on) as the host's own player:

- **Activate** (doors, switches, furniture, items) runs `Activate` on the host with the
  stand-in as the action ref, then OnActivate. The result replicates back.
- **Loot (containers, corpses, loose items):** local to the guest's own game, as today
  (§6.4). No request is needed, and nothing changes for the other players.
- **Quest items:** taking one is a request. The host moves it to the stand-in and the
  guest's inventory gets it. The host keeps a mirror of quest items on the stand-in, for
  scripts that check the player's items.
- **Lockpick and hack:** the minigame runs locally on the guest, and only the result is a
  request (as `Unlock` does now).
- **Hits:** as `RemoteHit` today.
- **Dialogue: restricted, as today (owner, 2026-11-07).** "keep guests to restricted
  dialogue. anything that starts/progresses a quest should be a no go on dialogue."
  - A guest can talk to anyone, and gets only choices that don't touch the story: trade,
    chip exchange, games, repair, and plain talk lines.
  - Any choice the guest would see is hidden if its INFO, or a line it leads to in the same
    conversation, could start or move a quest. That covers:
    - a result script calling SetStage, StartQuest, StopQuest, CompleteQuest,
      SetObjectiveDisplayed, SetObjectiveCompleted, AddTopic, SetQuestObject, or a quest
      item's AddItem or RemoveItem;
    - a choice that the quest's stage conditions gate as story.

    The engine works this out once per INFO from the compiled result script, a static
    scan, and caches it.
  - The conversation runs on the host's Game with the stand-in as the speaker's target. Its
    allowed result scripts (open barter, swap chips) run there as R2 requires, and the
    guest's menu shows the lines the host sends.
  - Forced greetings and scripted conversations never start with a guest, as now
    (`coop_guest` in dialogue.rs).
  - **The host's own conversation holds the quests its lines name (2026-11-15).** In co-op
    the world does not pause for the host's conversation (`Game::no_pause`), so quest
    GameMode blocks kept running in it, which they never do in the original: a
    conversation is a menu, and GameMode does not run while one is open
    (docs/spec-notes/scripts.md, "While a menu is open") [observed: layer5-captures.md
    §10.7]. The Fort's gear return broke on it (owner bug, 2026-11-15, Caesar's first
    task): the weather station guard's lines set `bGimme` in their begin script and
    `StartQuest FortEquipmentConfiscationQuest` in their end script (INFO 0x0015BCA1,
    0x00165712), so the quest's GameMode handed the gear back and stopped the quest
    while the line played, and the end script then started it again and it confiscated
    once more [observed: FalloutNV.esm, SCPT 0x00164763; owner log
    nvforever-20261115-235453-22188]. Now every quest that a spoken line's result script
    names (an SCRO of the quest) skips its GameMode until that conversation ends
    (`Game::talk_held_quests`, filled in `run_line_result`, emptied when `Game::dialogue`
    is none). The rest of the world goes on. [hypothesis: holding only the named quests
    is enough for the lines that rely on the pause; a quest a line moves only through
    another script is not held.]
- **Latency:** the guest's own UI updates optimistically for low-risk actions (a door
  starts opening, a picked-up item vanishes) and is corrected if the host refuses. On a
  LAN or a nearby connection it's under 100 ms.

## 9. What stays local on the guest

Its own player: movement, camera, weapon handling, first person, its inventory, AVs, perks,
XP, Pip-Boy, saves (`KeepSolo`). Menus. Cosmetic physics, particles, decals, footsteps and
ambient loops, all derived from replicated state. The guest's own solo game, which stays
paused in `SoloBase` as now.

## 10. Cost estimate

| Situation | Host → each guest |
|---|---|
| Quiet cell, nobody moving | about 0.2 KB/s (poses of players, heartbeats) |
| Goodsprings, 10 NPCs moving | 10 × 16 B × 10-20 Hz ≈ 2-3 KB/s |
| The Strip or a casino, 40 NPCs moving, music, speech | 40 × 16 B × 15 Hz ≈ 10 KB/s, plus events ≈ 12-15 KB/s |
| Big firefight, projectiles, debris | 20-30 KB/s in bursts (debris is cosmetic; only settles go) |
| Join | 100-500 KB once, in the loading screen |

**Host processing:** dirty-tracking is a bit set per mutation, and encoding is a few
microseconds per changed ref. That's well under 1% of a frame.

**Guest processing:** it drops. The guest no longer runs scripts or leveled rolls for the
world, and its AI is already off.

## 11. Phases (each shippable, R5)

1. **Determinism.** Per-ref seeded rolls for loot, death items and leveled AddItem.
   Replicate `ref_levels`/`zone_levels`. Closes 28/71 without new traffic. A test checks
   that two `World`s with the same seed and the same history produce the same rolls.
2. **Reliable channel and change log.** The dirty set, components `Exists`, `Enabled`,
   `Place`, `Open`/`Lock`, `Life`, `Look`. These replace `Presence`, `Door`, `Campfire`
   and the death and teleport parts of `Npcs`, which become thin wrappers or are deleted.
   Closes 38, 47 (heads), 54 and 22.
3. **Compact actor stream and interest priorities**, replacing `Npcs`. Includes `Anim`
   (furniture, idles), which closes 56 and 58.
4. **Guest world logic off.** Gate `Game::tick`'s world parts (quest/object/OnLoad/carried
   scripts on world refs, leveled rolls, PlaceAtMe) on "not a guest". Its own player's
   scripts and menus stay on. Script effects become events (§7), which closes 33, 55, 69
   and the presentation part of 66.
5. **Guest actions as requests** (§8), with dialogue on the host. Lazy inventories.
6. **Join from the host's snapshot.** Delete the special-case messages that are left.

A desync detector runs from phase 2 on. Every few seconds the guest hashes its replicated
state for the interest set, and the host compares. A mismatch logs the ref and component.
That makes any new desync class show up in the first playtest, not after a bug report.

## 12. Risks and open questions

- **Scripts that read the player while a guest acts** (`GetActionRef`, `player.GetItemCount`
  in a result script). The stand-in covers the actor side. It needs its inventory mirror to
  be right for checks like `player.GetItemCount Caps001`. Phase 5 has to list such
  functions and test them.
- **MenuMode blocks** run on the host while only a guest is in a menu: they shouldn't.
  MenuMode follows the host's own menus only, as now.
- **Mods that do per-frame work** (Qwib's `LListAddForm` every tick) run once on the host
  and replicate their effect. That's cheaper than now, where both games run them.
- **The guest's own quests while visiting** are untouched (`SoloBase`), as now.
- **Host performance:** the host simulates everything, as it already does.
- **Saves:** unchanged. `coop-save.md` still applies.
- **What to cut:** once phases 2-4 land, `follow_presence`, `undo_own_presence`,
  `not_in_hosts_world` and most of `place_like_host` go away. That's code deleted, not
  added.

## 13. Decisions for the owner

1. Approve the model (§4) and the phase order (§11)?
2. ~~Solo loot seeded per object~~ **Decided 2026-11-07:** loot stays as it is today and
   per-player (§5, §6.4). Only visible looks are seeded.
3. ~~Guests advancing quests~~ **Decided 2026-11-07:** no. Guests keep restricted dialogue,
   and any choice that starts or moves a quest is hidden from them (§8).
