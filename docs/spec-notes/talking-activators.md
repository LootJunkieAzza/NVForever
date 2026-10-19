# Spec notes: talking activators (intercoms, radios with lines, loudspeakers)

Clean-room description in plain language, tags as in the other notes. Sources: the
function catalogue (SetTalkingActivatorActor, IsTalkingActivatorActor,
IsActorTalkingThroughActivator), xEdit's TACT definition (`schemas/fnv/placed.json`), the
masters' compiled scripts and dialogue lines (read privately with `fnvtool script
decompile` and `fnvtool dialogue lines`; nothing quoted beyond names, short labels and
FormIDs). Radios as stations: [radio.md](radio.md).

Code: `fnv_game::talking_activators` (who speaks, IsActorTalkingThroughActivator),
`fnv_game::dialogue` (`Conversation::through`), `fnv_sim::World::activate` (a TACT's
default action is Talk); the engine's `dialogue_look.rs` (the view turns to the
activator; the actor does not turn). Tests: `crates/fnv-game/tests/perk_talk.rs`. Smoke:
`talking-activator-intercom`. Capture job: `tools/capture/probes/actor_changes.toml` ac6
(run 2026-10-18 on nvftest: every answer the same as ours).

## 1. What the data does [observed]

- 87 TACT records in FalloutNV.esm: stations, intercoms ("Intercom", "Security
  Intercom", "PA System"), recordings, "Lucky 38 Mainframe". TACT has a voice type
  (VNAM), a looping sound and a radio template [documented: xEdit].
- 31 SetTalkingActivatorActor calls in 28 scripts. The intercoms' OnActivate blocks set an
  actor and end with `Activate` (Hidden Valley's bunker intercoms set Ramos; the Tops'
  presidential intercom Benny; Repcon's, VMS18's waiter's, Black Mountain's radio); a few
  set it OnLoad (VEFR0xNCRTalkingActivator, the Lucky 38 alarm, the assassination radio).
- 92 dialogue conditions use IsActorTalkingThroughActivator, on the line's speaker: lines
  for the intercom (`== 1`, e.g. Ramos's "[You hear nothing.]" with only GetIsID Ramos
  beside it) and lines face to face (`== 0`, e.g. Benny's and Chris Haversam's).

## 2. NVForever's rules

- **The default action** of a talking activator is a conversation with it [hypothesis:
  the intercom scripts' closing `Activate` is what starts the talk]: activating one (E, or
  a script's Activate) answers Talk, as an actor does.
- **Who speaks**: the activator's talking actor while it lives, else the activator itself
  [documented: SetTalkingActivatorActor "makes a talking activator use the dialogue of a
  given actor; with no actor it reverts to normal behaviour"; hypothesis: a dead actor
  counts as none — the scripts test GetDead before setting it]. Its lines are chosen with
  the actor as the speaker and said in its voice; the activator must be there, the actor
  need not be (Ramos is in the bunker, Benny in his suite).
- **IsActorTalkingThroughActivator** is 1 on that actor while its lines are chosen and
  while that conversation runs, else 0 [documented: "returns true if the actor is
  speaking via an activator; the actor must first be registered through the
  talking-activator setter"]. Outside a conversation it is 0 even with the actor set
  [observed: capture ac6, 2026-10-18]. The catalogue's note "false if the talking actor
  was assigned by script" does not hold: IsTalkingActivatorActor read 0 before, 1 after
  SetTalkingActivatorActor by script and 0 again once the actor was cleared [observed:
  ac6, the same as NVForever's]. That IsActorTalkingThroughActivator is 1 during a
  conversation through the activator is still [hypothesis] (the 92 conditions rely on it;
  smoke `talking-activator-intercom`).
- **The picture**: the view turns to the activator; the actor does not turn to the
  player (it is not there). The subtitle and the dialogue menu name the actor
  [hypothesis].
- Not built: a talking activator's lines outside a conversation (Say, SayTo from an
  intercom are said by the activator as by any speaker); distance attenuation is not an
  issue in a conversation (heard whole) [documented: "talking activators configured by
  script have no distance attenuation"].

## 3. The bunker intercom in the original (2026-10-19) [observed]

AI observer campaign `intercom` (tools/capture/observe/intercom.campaign.json, plugin
version 9), from the saves `nvftest` (early game) and `nvfstrip` (on the Strip): the
player stands 110 units in front of HVBunkerIntercom2 (REFR 000ACD73) and a script runs
`HVBunkerIntercom2.Activate player`; samples every 2 game loops, a screenshot every
sample while a menu is open (the game in a 1,280 × 720 window).

- xNVSE's OnActivate event names the intercom and the player as the activating reference.
- A menu opens in the same sample and MenuMode 1009 (dialogue) reads 1 for one or two
  samples; the clock stops. The pictures show the dialogue camera close on the intercom
  (the view turns to the activator, as §2 has it), with the mouse cursor and **no line
  and no topic list** on screen.
- After about 10 game loops every menu is closed by itself (no hook ran), the clock goes
  on, and RamosREF (tracked) never read IsTalking 1. In both saves.
- So in these saves the original opens the conversation through the intercom and ends it
  at once with nothing said. Ramos's GREETING "[You hear nothing.]" (INFO 000A3D0D, quest
  VDialogueHiddenValley, conditions GetIsID Ramos and IsActorTalkingThroughActivator 1)
  links to the four password topics, so it is not a goodbye line; why no line was chosen
  is not readable (the observer cannot see which INFO the game tried) [hypothesis: one of
  the two conditions is false in the original here, or the bunker quests must have
  reached a stage]. NVForever says the line and keeps the conversation open (smoke
  `talking-activator-intercom`): a difference left open for the owner (owner-review).
