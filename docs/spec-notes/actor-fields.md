# Spec notes: the actors' minor fields (ESM audit low-impact list, "Actors")

From the low-impact list of docs/handoff/esm-gap-audit-2026-10-21.md, "Actors". Field
names [documented: xEdit wbDefinitionsFNV.pas NPC_, CREA, RACE, HAIR, FACT, BPTD, LVLI;
`schemas/fnv/gameplay.json`]. Counts are FalloutNV.esm's [observed 2026-10-23, private
survey, not committed].

## 1. ACBS flags (built)

Read from the actor's Base Data source (template flag "Base Data") [documented: GECK
wiki Category:NPC, Template Data: Base Data supplies the configuration flags except No
Knockdowns, No VATS Melee and a few others] (`fnv_game::data::GameData::minor_flags`).

- **Not Pushable** (NPC_ bit 27, CREA bit 27 "Not Pushable / Allow Pickpocket"): 25
  creatures, 26 placed; no NPC. The player and walking actors do not push it aside
  (`nvf_play::hitvol`, `blocking`) [hypothesis: the CREA bit means Not Pushable for
  creatures; the label names both].
- **No Rotating To Head-track** (NPC_ bit 30, CREA bit 29): 1 NPC (1 placed), 6
  creatures (2 placed). The head does not turn to look at things, as with a package's
  HeadTracking Off; a forced look (a conversation, a script's Look) still turns it
  [hypothesis] (nvforever `head_look.rs`).
- **No Head, No Right Arm, No Left Arm** (CREA bits 15–17): 48 creatures each, 38 placed.
  Such a part is never cut off or blown off (`nvf_play::dismember`) [hypothesis: the
  creature has no such limb to lose].

## 2. CREA CNAM Impact Dataset (built)

793 creatures name one: `BiteDataSet` 172, `FeralGhoulImpactDataSet` 97,
`SuperMutantHandImpactDataSet` 84, `GeckoClawAttack` 64, `NPCHandtoHandDataSet` 58,
`ClawDataSet` 57 … [observed]: the impacts of the creature's own attacks, a weapon's INAM
for a creature that has none [hypothesis]. A creature's own blow (no weapon) now shows the
impact of that set for the struck actor's material; before, it showed the player's fists'
(`nvf_play::combat`, `fnv_combat::fx::FxForms::creature_impact_set`).

## 3. Nothing to build: no master record uses the field

- RACE DATA skill boosts: every race's seven boosts are 0 or unused.
- HAIR "Fixed": no hair sets it (67 hairs).
- LVLI COED owner: 1,926 entries, none with an owner.
- CREA "No Shadow": 3 creatures, none placed.
- AIDT mood and ACBS "Autocalc Service": built before (GetAV Mood; services.rs).

## 4. Not built, and why

- **CREA Swims / Walks / No Combat in Water** (190, 1,526, 3): keeping creatures out of
  water needs pathing that knows water; every character may swim (swimming.rs).
- **CREA Tilt Front/Back, Left/Right**: nothing aligns creatures to the slope yet.
- **CREA WNAM foot weight, LNAM melee list; NPC_ NAM7 weight; RACE weights and FaceGen
  clamps**: no system uses them (footstep loudness is the sound's, there is no body
  morphing, the character creator keeps its own slider limits).
- **NPC_ KFFZ** (extra animation files): the NPC clip sets are found by folder; adding a
  record's own list is animation work (animation.md).
- **RACE XNAM relations**: race relations are not modelled (faction relations are).
- **FACT ranks and titles, Hidden, Allow Sell**: no Pip-Boy faction page; no stolen-goods
  barter rule.
- **BPTD To Hit Chance and IK flags**: VATS mode has no hit chance by design
  (vats-mode.md); no IK system but head tracking.
