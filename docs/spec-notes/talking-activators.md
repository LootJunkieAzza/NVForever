# Spec notes: talking activators (intercoms, radios with lines, loudspeakers)

Clean-room description in plain language, tags as in the other notes. Sources: the
function catalogue (SetTalkingActivatorActor, IsTalkingActivatorActor,
IsActorTalkingThroughActivator), xEdit's TACT definition (`schemas/fnv/placed.json`), the
masters' compiled scripts and dialogue lines (read privately with `fnvtool script
decompile` and `fnvtool dialogue lines`; nothing quoted beyond names, short labels and
FormIDs). Radios as stations: [radio.md](radio.md).

Code: `fnv_game::talking_activators` (who speaks, IsActorTalkingThroughActivator),
`fnv_game::dialogue` (`Conversation::through`), `Game::say` (`Speech::through`), `fnv_sim::World::activate` (a TACT's
default action is Talk); the engine's `dialogue_look.rs` (the view turns to the
activator; the actor does not turn). Tests: `crates/fnv-game/tests/perk_talk.rs` (Say and SayTo through one:
`a_talking_activator_says_its_talking_actors_lines_by_script`; an actor's Dialogue
package aimed at one: `a_dialogue_package_aimed_at_a_talking_activator_talks_to_its_talking_actor`). Smoke:
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
- **A script's `<activator>.Activate player`** is the player activating it: the REPCONN
  intercom's trigger (SCPT 0x000CD04E, OnTriggerEnter player) sets Chris as the talking
  actor of intercom 0x000CD67C or 0x000CD685 and then runs `<intercom>.Activate player`
  [observed: `fnvtool script decompile` FalloutNV.esm], so the conversation opens as for
  the player's own activation (nvf-play `talk_after`; the headless runner's
  `scripted_talk`, issue #906; smoke `intercom-trigger-activate`).
- **A bare `Activate`** (no action reference) uses the reference's own action reference,
  and outside OnActivate that is whoever activated it last [documented: GECK wiki
  Activate, "the calling reference's ActionRef will be used; if the calling reference
  doesn't have an ActionRef, the object won't be activated"; hypothesis: the reference
  keeps it after its OnActivate; not saved]. Every Sink appliance in OldWorldBlues.esm
  (SCPT 0x0100F05A, 0x0100F05D, 0x0100F071, the Jukebox talker's 0x0100D36B, ...) shows
  a message in OnActivate, sets a flag and calls `Activate` (bare, or `Activate player`)
  in GameMode once the message box is closed [observed: `fnvtool script decompile`], so
  it talks to the player after the module is installed (issue #1111; smoke
  `mq-old-world-blues-sink`). A reference never activated still does nothing.
- **Who speaks**: the activator's talking actor while it lives, else the activator itself
  [documented: SetTalkingActivatorActor "makes a talking activator use the dialogue of a
  given actor; with no actor it reverts to normal behaviour"; hypothesis: a dead actor
  counts as none — the scripts test GetDead before setting it]. Its lines are chosen with
  the actor as the speaker and said in its voice; the activator must be there, the actor
  need not be (Ramos is in the bunker, Benny in his suite).
- **No talking actor set yet**: a talking activator no SetTalkingActivatorActor has
  reached speaks for the one actor the load order's scripts give it, when they name
  exactly one: the calls in its base's own script (on itself) and the calls of any
  script on that reference by name; several actors named, or none: itself
  (`Game::scripted_talking_actor`) [hypothesis, issue #2454; observer check 185]. The
  Hidden Valley intercom (HVIntercomREF) needs it: after McNamara sends the player out
  for the equipment (INFO 0016329B, controls off) the inner door's script (SCPT
  000E3665) has the intercom start a conversation, and only Ramos's line 000E3664
  gives the controls back. Its OnActivate (SCPT 000A418A) and Veronica's trigger (SCPT
  0015B6D5) are the only calls that set Ramos, so a player who came in without using
  the intercom (sneaking in) met an intercom with no line and kept the controls off
  [observed: `fnvtool script run`, smoke `hv-intercom-push`].
- **IsActorTalkingThroughActivator** is 1 on that actor while its lines are chosen,
  while that conversation runs, and while it says a line through the activator outside
  one (Say or SayTo on the activator, a conversation between actors through it), the
  line's begin and end result scripts included; else 0 [documented: "returns true if the
  actor is speaking via an activator; the actor must first be registered through the
  talking-activator setter"; GECK wiki IsActorTalkingThroughActivator: it tests whether
  the actor "is currently talking through an Activator" and "only returns true when the
  actor is actually in dialogue through the activator"; hypothesis for the line outside a
  conversation, #2768]. The wiki's further note that it "will return false if the talking
  activator actor was set via script" is not followed: the masters' intercom lines test
  it == 1 for actors only scripts set (§1). Outside a conversation and a line it is 0
  even with the actor set [observed: capture ac6, 2026-10-18]. The catalogue's note "false if the talking actor
  was assigned by script" does not hold: IsTalkingActivatorActor read 0 before, 1 after
  SetTalkingActivatorActor by script and 0 again once the actor was cleared [observed:
  ac6, the same as NVForever's]. That IsActorTalkingThroughActivator is 1 during a
  conversation through the activator is still [hypothesis] (the 92 conditions rely on it;
  smoke `talking-activator-intercom`).
- **The picture**: the view turns to the activator itself (an intercom on the wall, not
  a point above it as for an undrawn actor); the actor does not turn to the player (it is
  not there). The dialogue menu names the activator ("Intercom" at the top right, the
  line in the band below) [observed: owner screenshot 2026-10-22 of the REPCONN
  intercom]; the logged subtitle names the actor.
- **A Goodbye greeting** the player gets through one opens a conversation, as a force
  greet's does (dialogue.md §5): the dialogue menu holds the player while the line is
  said and the Goodbye flag then ends it [owner statement 2026-10-22: "even if the line
  has a goodbye it still draws into dialogue"]. Chris Haversam's "I'm letting you in"
  (INFO 000CD87E) from the REPCONN intercom by the locked door
  (REPCONMidToTopDoorIntercom, RocketLabMid) is one: Chris is upstairs in RocketLabTop,
  and the line's end script unlocks the door [observed: FalloutNV.esm]. Said at the
  player without a conversation, as face to face, it was said where Chris stood, out of
  hearing; the player tried again, got "Stop messing around with the intercom", and that
  conversation cut the silent line before its end script ran (owner playtest 2026-10-22;
  smoke `intercom-goodbye-line`).
- **Say and SayTo on a talking activator** with a talking actor (the one set by
  SetTalkingActivatorActor, else the one its scripts name, as above) say that actor's
  lines: the line is chosen with the actor as the speaker (GetIsID, faction, voice type
  and every other condition on the speaker, the fixed-speaker check, Say Once), with
  IsActorTalkingThroughActivator 1 on it while choosing, and said by the actor through
  the activator, in the actor's voice, as a conversation through one is [hypothesis,
  issue #2768; observer check 219; documented: SetTalkingActivatorActor "makes a talking
  activator use the dialogue of a given actor"]. Tale of Two Wastelands needs it:
  Vault 101's intercom is given Jonas and says CG02IntercomConv, whose lines test GetIsID
  Jonas; the Pitt's Ashur intercom (DLC01AshurIntercomREF) says DLC01AshurCalledOutside1
  and the square's loudspeaker Ashur's speech [observed headless by the maintainer, TTW
  3.4: "no line" before]. When the actor has no line of the topic the activator's own
  lines are tried, as before [design, #2768]; without a talking actor nothing changes.
  While the line is said the activator stands for the actor in the world, as for a
  conversation's line through it (#2451; `Speech::through`): the line goes on while the
  activator is there (the actor's death cuts it), the activator reads IsTalking 1, a
  repeated Say of the topic on it finds the line playing, its own line and a line through
  it cut each other (whichever starts second wins) [design, #2768], the listener looks at
  the activator (the engine turns heads to the point where it is placed) and the far
  actor does not turn, the line is heard from the activator (the engine's voice and
  subtitles) panned to its side but with no distance attenuation [documented: GECK wiki
  SetTalkingActivatorActor, Notes: "Talking Activators set via script will have no
  distance attenuation in the New Vegas engine"; hypothesis: one in another cell is
  still not heard], and its SayToDone blocks run on the activator
  [hypothesis: the script that said it waits there]. The line's result scripts run on the
  actor, as in a conversation through the activator [hypothesis].
- **A talking activator as the listener** with a talking actor (as above) is that actor
  for the line's conditions run on the target: an actor's Say To Dialogue package or
  conversation aimed at the activator talks to the actor on the other end, so GetIsID,
  faction and every other condition run on the target test the actor, and the
  activator's replies are said as the actor through it (the "Who speaks" rule above)
  [hypothesis, issue #3020; documented: SetTalkingActivatorActor "makes a talking
  activator use the dialogue of a given actor"]. Tale of Two Wastelands needs it: in
  Growing Up Fast (CG02), stage 35 makes Vault 101's diner intercom
  (CG02DinerIntercomREF) speak for Jonas, and Dad's Dialogue package
  CG02DadTalkToJonasOnIntercom, aimed at the intercom, says CG02IntercomConv, whose
  first line (INFO 0x0A031D3C) tests GetIsID CG02Dad on the subject and GetIsID
  CG02Jonas on the target, Jonas's reply (0x0A031D3D) GetIsID CG02Jonas on the subject
  and GetIsID CG02Dad on the target [observed: `fnvtool dialogue lines --topic
  CG02IntercomConv`, TTW load order]. With the activator itself as the target Dad had
  no line, and stages 36 and 38, set by the exchange's lines, never came [observed
  headless, the quest pilot's chain ttw-vault101-cg01-cg04]. Conditions on the target
  that read where it is (GetDistance and the like) read the actor too, who is
  elsewhere [design: one rule for every condition; none of the masters' intercom lines
  is known to need otherwise]. Without a talking actor the activator is the target, as
  before. Code: `Game::conditions_target`, applied in `Game::line_verdict`.
- Distance attenuation is not an issue in a conversation (heard whole).

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
