# Player voices: co-op characters that talk like companions

Status: **phases 0-5 built (2026-11-06)**: the voice set format and `fnvtool voice set-draft`/`set-places`/`set-check`; nine voices in the MO2 mod `NVForever Player Voices`, four vanilla (Cass, Boone, Raul, Arcade) and five from Nexus followers (Willow, Delilah, Niner, Cook, Molerat), the Nexus lines carried with their own sound and subtitle; the creator's Voice row with a preview; first-person combat quips at the set's rate; the two Gameplay settings; co-op quips heard from the speaker's character (phase 2); reactions to the other players and the companion 50/50 (phase 3); place and quest comments (phase 4). What phases 2-4 built differs from the plan below in places: §10.

The owner's outline (2026-11-05): "A new option in the character creator for voice. We pull
voice files from nexus mods or other companions in game (and would probably use tools to
alter them to fit). The primary function is to make your friends playing with you feel more
like actual Fallout companions, having reactions to certain events in the story or quests."
Loosely after Dying Light: every player is a different version of Courier 6, and picking
a voice also picks a personality.

Decisions taken with the owner (2026-11-05 planning session):

- Sources: vanilla companions and five Nexus voiced companions the owner chose (§2). Two
  female Nexus voices is enough for now.
- Content: generic quips (combat, reactions to the player) plus place and quest comments
  that something Courier 6 would know could say. No lines about the companion's own life.
- Hearing: everyone in hearing range, positional from the speaker's body, like an NPC line.
- Dialogue-choice reactions: not planned now ("want to see how the simpler quippy kinda
  stuff goes first").
- Chattiness: Normal by default, a setting per player.
- Several players may pick the same voice.
- Vanilla voices play slightly shifted in pitch and speed, by a fixed amount per voice that
  NVForever sets (no slider; owner 2026-11-06). When that companion is with the
  group, each quip for that voice goes 50/50 to the player or the companion (§6).
- Two Gameplay settings: **First Person Quips** (your own character's quips, generic only,
  nothing story-related) and **Party Quips** (the other players' quips) (§7).
- Editing recordings means trimming, pitch, speed and effects on the existing files.
  Generating new speech in a real actor's voice (voice cloning) is out of scope.

## 1. What already exists [observed in the code, 2026-11-05]

- **Voice playback** `engine/nvf-play/src/voice.rs`: plays a response's file from the
  archives where its speaker stands, with hearing range (`DIALOG_MIN_DISTANCE` 600 /
  `DIALOG_MAX_DISTANCE` 2000), pan, voice filter and reverb. It hides the subtitle when a
  line is not heard. Lip sync comes from the `.lip` beside the file
  (`engine/nvforever/src/lips.rs`). The voice type comes from the speaker's base form
  (`voice_base`, `fnv_voice::voice_path`).
- **Combat barks** `crates/fnv-game/src/combat_lines.rs`: Hit, HealthHalf/Quarter,
  Crippled, Death and DeathResponse, Attack, NoAmmo, OnFire, Poisoned, the detection
  changes, with the game's own chances and timers.
- **Co-op lines** `engine/nvf-coop/src/net.rs`: `Msg::Lines` sends `LineView { speaker,
  target, topic, info, remaining, flags }` from the host to guests. Players already have
  speaker ids (`LineView::HOST`, `LineView::guest(id)`). Looks travel as `Msg::Look`
  (`encode_look` in `engine/nvforever/src/coop.rs`). The protocol is at `VERSION` 15.
- **Character creator** `engine/nvf-play/src/creator.rs`, rows and tabs in
  `creator/model.rs`. The look it edits is `fnv_game::chargen::Look` (serde, saved with
  the game).
- **Settings** `crate::settings::add_setting` with `group: crate::pause::GROUP_MENUS`
  ("Gameplay"), for example in `dialogue_look.rs`.
- Not wired anywhere yet: the PLAYER* reaction topics (PLAYERFIREWEAPON,
  PLAYERTHROWGRENADE, PLAYERLAYMINE, PLAYERINIRONSITES, PLAYERLOCKEDOBJECT).
- Guest-side NPC voice range and repeat bugs from the 2026-11-05 playtest are being fixed
  on the coop-progression branch. Player voices build on that fixed path. They start
  after it merges.

## 2. The voices

Counted with `fnvtool dialogue lines` over FalloutNV.esm and the four story DLCs, plus each
mod's plugin [observed 2026-11-05; the counts come from a rough classifier, so read them as
sizes, not exact numbers].

**Vanilla** (in everyone's archives already):

| Voice | Combat and detection | Reacting to the player | Place, quest and idle | Note |
|---|---|---|---|---|
| Cass (F) | 125 | 17 | ~270 (186 `VCassLocation*`) | Richest vanilla set |
| Veronica (F) | 66 | 4 | ~110 | Many lines name her own people: hand-pick |
| Lily (F) | 95 | 10 | ~40 | Novelty voice ("Leo", "Grandma") |
| Boone (M) | 84 | 8 | ~50 | Terse; skip his past |
| Raul (M) | 92 | 10 | ~45 | "Boss" humour |
| Arcade (M) | 57 | 6 | ~60 | Skip the Followers/Enclave story |
| Joshua Graham (M, HH) | 64 | 6 | few | Combat voice |
| Follows-Chalk (M, HH) | 74 | 0 | few | Combat voice |
| Waking Cloud (F, HH) | 73 | 0 | few | Combat voice |
| Dean Domino (M, DM) | 60 | 3 | Sierra Madre only | Combat voice |

Not usable: Christine (mute), ED-E and Rex (beeps, barks), Dog/God (a novelty; maybe later).

**Nexus** (the owner's downloads; every player installs them through MO2):

| Mod | Files | Lines | Combat | Place and quest | Personality |
|---|---|---|---|---|---|
| NVWillow 1.10 (F), 41779 | 1,693 ogg + lip, BSA | 1,452 | 59 | 130 place, `AWDTimed*` per location | Warm, flirty, chatty |
| Delilah 1.7 (F), 50641 | 340 ogg + lip | 434 | own `00VDCombatBarks` | `00VDDelilahAt*`, timed chatter | Upbeat medic |
| Molerat 1.4.1 (M), 75128 | 424 ogg + lip | 321 | 40, 14 kinds | place plus story reactions (BoS, Omertas, Ulysses) | Folksy cowpoke |
| Niner 1.4 (M), 48002 | 510 ogg + lip | 333 | 16 | few | Crude, cheerful junkie, British |
| Cook 1.2 (M), 86274 | 140 WAV + lip | 67 | none | none; 4 reactions | Gruff marksman; thin |

**Permissions** [owner, 2026-11-06: "Nexus page says it's fine with credit"]: the files a
voice set uses may be taken from the mod and kept in our own mod folder, with credit. So
the voice tool extracts only the files a map uses into `NVForever Player Voices`, next
to a `mods-used.txt` naming each mod, its author, its Nexus page and the version (§3).
They are never committed to the repository.

## 3. A voice set and its voice map

A **voice set** is a voice (an id such as `vanilla.cass` or `nexus.willow`) and its
**voice map**: a JSON file (`nvforever.voice_set/1`, `fnv_game::player_voice::VoiceSet`)
saying which lines answer which event. FormIDs are written `<plugin>|<id in the plugin>`,
so a set does not depend on the load order.

```json
{
  "format": "nvforever.voice_set/1",
  "id": "vanilla.cass",
  "name": "Cass",
  "sex": "female",
  "blurb": "Whiskey-rough caravaneer: blunt, profane, never short of a comeback.",
  "speaker": "FalloutNV.esm|133FDD",
  "rate": 0.95,
  "companion": "FalloutNV.esm|133FDD",
  "lines": [
    { "info": "FalloutNV.esm|15C37D", "event": "NoAmmo", "first_person": true },
    { "info": "FalloutNV.esm|15CA6A", "event": "react.inironsites", "first_person": false },
    { "info": "FalloutNV.esm|1234AB", "event": "place", "use": false, "note": "conditions: GetInCell" }
  ]
}
```

- `speaker`: the actor whose voice type holds the files (its folder is looked up through
  the game's own rule, `fnv_voice::VoiceBank::file`). `event`: a Combat-tab topic's
  Editor ID for combat lines, `react.<what>` for the PLAYER* topics, `place`, `quest`,
  `idle` for the rest. `first_person`: the line suits the speaker's own ears (First
  Person Quips). `use: false`: kept for review, not said. `note`: why the draft tagged it
  so, never the line's text.
- **Vanilla maps are drafted by a tool** (`fnvtool voice set-draft`, phase 0). It takes
  every line the actor says (fixed speaker, or `GetIsID` / `GetIsVoiceType` of it on the
  subject), leaves out conversation lines (choices, prompts) and lines without a file in
  the voice, and tags the rest. Combat lines with nothing but identity in their conditions
  are in use. Place, quest and idle lines are drafted from their conditions (GetInCell,
  GetStage...) and wait for review. `--text` prints the texts for the review (never
  saved). A person then drops lines about the companion's own life (`use: false`) and
  lines that ask a leader for something (`first_person: false`, the party still hears
  them). `fnvtool voice set-check --set <file>...` resolves each set and checks every
  response has its file.
- **Mod maps are hand-made from a draft.** The mods use their own topics
  (`AWDTimedTheStrip`, `00VDCombatBarks`), so the tool only lists the lines with their
  conditions, and the map tags them.
- **Maps hold FormIDs and tags only**: no line text, no audio. They live in the MO2 mod
  folder `NVForever Player Voices` (`nvforever/voices/<id>.json`), not in the repository. For a
  Nexus voice the folder also holds the extracted files the map uses
  (`sound/voice/<plugin>/<voice type>/...`, with their `.lip`) and `mods-used.txt`
  (credits). The base-game sets (`vanilla.*`, FormID and tag lists, no text, no audio) are
  committed in `coop-assets/nvforever/voices/` and ship beside `nvforever-coop.exe` in
  `coop-assets
vforeveroices`, read like the co-op icons (owner decision 2026-11-23: base-game
  voice sets ship; no Nexus set or file ships). The Nexus sets stay in the private mod folder.
- The menu name and personality blurb are NVForever's own words (LEGAL.md).
- **A Nexus voice's lines carry their sound and subtitle** (phase 5, decided 2026-11-06).
  Loading the follower's plugin to reach its INFOs would put the follower in the world
  (and every player would need the mod and its masters), so a line of a mod set names
  its sound and subtitle itself, and the set has no `speaker`:

  ```json
  { "info": "Molerat.esm|00E61D", "event": "NormalToCombat", "first_person": true,
    "file": "sound\\voice\\molerat.esm\\aaamoleratvoicetype\\<file>.ogg", "text": "<subtitle>" }
  ```

  `info` then only says where the line came from. When the script data is built, each
  such line becomes an INFO of its own, numbered from `0xFE000000` (a load-order index no
  plugin has) in set id order, then line order, so two games with the same sets number
  them alike (co-op). Its one response has the subtitle; `GameData::voice_files` names
  its sound, which plays from the data (the MO2 folder), lasts its file's length and has
  its `.lip` beside it (`fnv_game::player_voice::add_packed_lines`). The subtitle is the
  mod author's text, credited in `mods-used.txt` with the mod, its page and the files
  taken; acting notes in braces are dropped.
- **Drafting a mod voice**: `fnvtool voice set-draft <dir> <masters...> <mod plugin>
  --speaker <npc> --pack` on a scratch folder holding the masters, the mod's plugin and
  its `sound/voice` folder. With `--pack` every line with a file in the speaker's voice
  type is the speaker's (the mods put the speaker's condition on the quest), only
  one-response lines are kept, and each line gets its `file` and `text`. A review then
  tags the mod's own topics (`00VDCombatBarks` as NormalToCombat, and so on), and only the
  lines in use and their files go into the mod folder. `set-check --mod-dir <folder>`
  checks a carried line's file in that folder.
- `NVF_MOD_DIR=<folder>` (for tests): the folder's loose files stand over the
  data, as MO2 puts a mod folder (headless runs read the plain game Data).

## 4. Events

Tags, with who decides them in co-op:

| Tag | When | Decided by |
|---|---|---|
| `combat.start`, `combat.end`, `combat.lost` | the speaker's detection state changes | the speaker's game |
| `hit`, `health.half`, `health.quarter`, `crippled` | the speaker is hurt | the speaker's game |
| `kill`, `ally.down` | the speaker kills; another player goes down | the speaker's game / host |
| `attack`, `explosive`, `no.ammo`, `poisoned`, `on.fire`, `stimpak` | as the combat topics | the speaker's game |
| `react.grenade`, `react.mine`, `react.aimed_at`, `react.shot_near`, `react.lockpick` | **another player** does it near the speaker (the companion PLAYER* lines, turned around) | the speaker's game, from that player's state (attacks counter, weapon, worn) |
| `place.enter` | the speaker enters a mapped place (cell, worldspace or map marker) | the speaker's game |
| `quest.stage` | a mapped quest stage is set | the host (it owns quest state) |
| `idle` | nothing has happened for a while, out of combat | the speaker's game (Chatty only) |

Barks follow the game's own chances and timers (`combat_lines.rs`). On top of that:

- a **group cooldown**: no new quip starts while another player's quip is playing in
  hearing range, plus 2 s after it;
- **Normal**: at most one `place.*` or `quest.*` comment per player every 150 s, and each
  INFO at most once per 20 minutes for that player (no repeats);
- **Quiet**: combat start, health.quarter, `place.enter` for named places, `quest.stage`;
- **Chatty**: Normal plus `idle` about every 60 s.

A `quest.stage` comment is given to one player: a random player in hearing range whose voice
has a line for it.

## 5. Co-op

- **Same data on every machine.** Co-op already needs one load order. The Nexus voice mods
  and `NVForever Player Voices` join the shared MO2 list (`C:\NVForever\mods`). The lobby
  check refuses a voice set a peer doesn't have: that player is heard silently, with
  subtitles only.
- **The choice** goes with the look: `Look` gets `voice: Option<String>` (the set's id)
  (serde default, so old saves load). `encode_look` adds trailing bytes; old decoders stop
  before them.
- **A quip** is a new message `Msg::Quip { id, seq, info }`: the speaker's game sends
  it to the host, and the host relays it to the others (the same route as `Msg::Anim`).
  Every game then plays the INFO's files from its own archives. No audio goes over the
  network. Protocol `VERSION` +1 (take the highest on merge, as for `Progress`).
- **Playback** reuses `voice.rs` with the speaker set to the player's puppet (or the local
  player): position from `PlayerState.feet`, mouth at `MOUTH_HEIGHT`, hearing range,
  pan, helmet voice filter (power armour helmets), reverb. Speaker names come from the
  voice set, not from the player's base form.
- **Subtitles** show the player's name, and follow the hearing rule and the game's subtitle
  setting.
- **Lip sync** on the speaker's puppet face from the `.lip`, its timeline scaled by `rate`.
  On your own character: only in third person.

## 6. Pitch, speed and the real companion

- **Rate.** A voice plays at `rate` (Bevy playback speed): pitch and speed move together, so
  the line stays natural and the `.lip` timeline is just scaled. Vanilla voices default to
  0.94 or 1.06 (whichever suits the voice), never 1.0, so a player's Cass is not mistaken
  for Cass. The rate is the voice map's; there is no slider [owner 2026-11-06]. Whether
  Bevy's speed goes through the pan/filter/reverb chain unchanged is checked in phase 1
  [hypothesis].
- **Line lengths.** The line's length is the file length ÷ rate (`VoiceBank` hook).
- **The companion is here.** When the NPC a vanilla voice belongs to is a follower of the
  group, within hearing range, and the speaker's event has a line in the companion's own
  topics: a 50/50 coin flip. Either the player says it (at their rate), or the companion
  says its own line at 1.0. The player's quip and the companion's own bark for that event
  share one cooldown, so the event is never heard twice.
- Trims and effects for mod voices, if ever needed, are a per-line `trim` and per-voice
  `filter` in the map, applied at playback. NVForever never rewrites the mod's files.

## 7. Character creator and settings

- **Creator**: a **Voice** tab (or a row on the first tab, the owner's pick in phase 1):
  - **Voice**: the sets found (Left/Right), grouped Female / Male / Other, with the
    personality blurb; "None" (the default) keeps the character silent;
  
  - **Preview** (button, P key, controller X): plays a random `first_person` line from
    the set at its rate, flat, with its subtitle; again for another line.
  - Saved with the look (Done, the save file), shown to others in the lobby.
- **Gameplay settings** (`add_setting`, group "Gameplay"):
  - **First Person Quips**: on/off. Your own character's quips play for you. Generic
    lines only (`first_person = true`): combat and reactions, nothing story-related. This
    works in single player too.
  - **Party Quips**: on/off. The other players' quips. Off: silent and no subtitles.
  - **Quip chattiness**: Quiet / Normal / Chatty (§4).
  - Turning off First Person Quips doesn't stop your friends from hearing you. What each
    player hears is up to that player's own settings.

## 8. Phases (each needs the owner's go-ahead)

0. **Voice map tool.** `fnvtool voice set draft <plugin> --voice-type <vt> | --speaker
   <npc>`: lists a voice's lines with draft event tags (§3). `fnvtool voice set check`:
   reads a map, checks every INFO exists and has files in the load order. Draft maps for
   Cass, Boone, Raul and Arcade, reviewed. *Done when:* the four maps check clean against the
   owner's install.
1. **Combat quips, vanilla voices, solo first.** `Look.voice`, the creator's Voice row and Preview, the events `combat.*`, `hit`, `health.*`, `kill`, `no.ammo` for your own
   character, the rate and lip scaling, the three settings. *Done when:* in single
   player, First Person Quips on, a fight gives Cass-at-1.06 barks with subtitles; a smoke
   scenario checks the log lines.
2. **Co-op.** `Msg::Quip`, relay, puppet playback with range, pan, subtitles and lip sync,
   the lobby's check for missing sets, the group cooldown. *Done when:* in a two-game
   session the guest hears the host's quips from the host's body and loses them out of
   range.
3. **Reactions and the real companion.** `react.*` from the other players' state, the 50/50
   companion rule. *Done when:* a grenade thrown by one player gets a "Grenade!" from a
   friend's voice; with Cass following, a Cass-voiced player and Cass share the event.
4. **Place and quest comments.** `place.enter`, `quest.stage` (host-assigned), chattiness,
   no repeats. Hand-picked lines for the vanilla voices. *Done when:* entering the Strip
   with a Cass voice gives one Strip comment, not again for 20 minutes.
5. **Nexus voices** (built 2026-11-06, ahead of 2-4 at the owner's word). Sets for Willow
   (29 lines), Delilah (10), Niner (16), Cook (6) and Molerat (30), combat only; their
   place, quest and idle lines wait for phase 4. Cook has no combat lines of his own: his
   set is six lines of his that fit a fight's start, a grenade and its end. *Done when:*
   each checks clean and previews in the creator; a fight with a Nexus voice plays its
   lines (smoke `player-voice-nexus-combat`, `player-voice-nexus-creator`).
   Phases 2-4 were built the same day (§10).
6. Later, not planned: dialogue-choice reactions; more voices; a third female Nexus voice.

## 9. Open questions for the owner

- ~~May the vanilla maps (FormID and tag lists, no text) be committed?~~ Settled 2026-11-23:
  the base-game sets are committed and ship (§3); nothing from Nexus ships.
- Voice as its own creator tab, or a row on the first tab?
- Cook: kept as a thin set of six lines (phase 5); enough, or folded into another voice?
- The authors of Niner, Cook and Molerat for `mods-used.txt` (their plugins name none and the Nexus pages could not be read here).

## 10. Phases 2-4 as built (2026-11-06)

- **Co-op transport** [design]: a quip is a table action of the co-op tables
  (`nvf-play/src/coop_tables.rs`, game `VOICE` = 10, table 0): the set's id and the INFO,
  sent to everyone through the host, each copy taken once. No new `Msg` and no protocol
  bump. The receiver resolves the set itself (same sets and load order on every machine),
  refuses a line that is not one of the set's, and plays it at the speaker's feet plus
  `MOUTH_HEIGHT`, by distance (`voice_gain`, nothing past `DIALOG_MAX_DISTANCE` or in
  another place) and side (`pan_gains`), at the set's rate, with a subtitle "Name: text"
  (`ScriptHud::party_lines`). Party Quips off: not heard, no subtitle. Not built: the lobby
  check for a missing set (the receiver logs it and stays silent), lip sync on the puppet,
  the voice filter and reverb for another player's quip (`engine/nvf-play/src/party_quips.rs`).
- **Who picks what** (`fnv_game::player_voice`): alone, a quip is picked among the
  first-person lines only. In co-op (`QuipSettings::coop`, set while others are in the
  session) it is picked among all the voice's lines and sent; this player hears it only
  when it is a first-person line and First Person Quips is on, else it says its usual
  line. The group cooldown: while another player's quip plays, plus `GROUP_COOLDOWN` (2 s),
  no quip of this player starts (`QuipState::hold_until`).
- **Reactions** (phase 3): each game looks at the other players near it (`SocialMates`,
  now with each player's weapon, attack counter and sights flag): an attack with a grenade
  (WEAP animation type 10), a mine (11, 12) or a gun out of a fight (3-9), or sights held
  on this player for `AIMED_AT_SECONDS` within about 10 degrees and `REACT_RANGE`. The
  answer is a `react.*` line of this player's voice, at most every `REACT_EVERY` (20 s),
  sent to the others. The vanilla sets' PLAYER* lines are in use for this.
- **The companion 50/50** (phase 3): when the voice's companion follows the player within
  `PLACE_RADIUS`, a coin flip gives each of the player's combat lines to the companion,
  who says its own line for the topic at its own speed, and the player says nothing.
- **Place comments** (phase 4): a `place` line names references it is said near (`near`,
  the trigger boxes whose scripts start it) or the cells, worldspaces and map regions it
  is said in (`inside`). `fnvtool voice set-places` fills them: references from the
  scripts that name the topic, and `--place TOPIC=EDID,...` for a topic a script starts
  after testing the player's region. Cass's 29 location topics are started that way by her
  quest scripts (`IsPlayerInRegion` on the vMap regions, `GetInWorldspace` for the Strip
  and Freeside) [observed: FalloutNV.esm, VCassTimerGeneralBarkScript and
  VCassTimerRegionBarkScript source]; Raul's by trigger boxes and a package. Willow's
  location comments (AWDTimed*) are mapped by hand to the same regions. A comment is said
  on coming into the place, out of a fight, if its conditions pass but the ones that only
  say who speaks (`IDENTITY`), at most one every `COMMENT_EVERY` (150 s), each line at
  most every `COMMENT_AGAIN` (20 min). Lines in use: Cass 152 (her own story, her sex and
  earlier visits left out), Raul 2, Willow 23, Delilah 1.
- **Quest comments** (phase 4): a `quest` line is a candidate when a stage of a quest its
  conditions name was just set (`Game::note_stage_for_voice`), tested as a place comment.
  Built, but no set has one in use: the vanilla `quest` lines are conversation replies and
  the Nexus ones depend on the follower's own quest variables. Each player's own game
  decides its comments (not the host, as planned): quest stages are shared in co-op.
- **Chattiness**: Normal only; there is no setting, and no `idle` comments.


## 11. Ghouls and voices by race and sex (2026-11-06)

The owner: "when you select a ghoul you have only ghoul voices to choose from, ... when
you're a human you only have access to human lines. Make sure lines are only available to
the gender of the character" [owner 2026-11-06].

- **Ghoul as a race.** The Playable Ghoul mod (Nexus 35531) turns on the playable flag of
  the vanilla Ghoul race (RACE 0x003B3E, DATA flags) and brings its hairs, a few first
  person hand textures and its traits [observed: PlayableGhoul.esp read record by record,
  its archive listed with `fnvtool bsa list`]. The creator lists every playable race of the
  load order, so with the plugin loaded Ghoul is an Origin like the others; nothing in
  NVForever names it.
- **A set's race.** A voice set has `race`, `human` or `ghoul` (empty is human):
  `fnv_game::player_voice::race_kind` makes a race `ghoul` when its editor id has "ghoul"
  in it (Ghoul, a follower mod's WillowsGhoulRace), otherwise `human` [design].
  `fnvtool voice set-draft` writes the speaker's race and prints it with the speaker's sex.
- **The creator** offers only the sets that fit the look (`VoiceSet::fits`: the same race
  kind, and `female` for a woman, `male` for a man; `other` fits nobody). Changing Sex or
  Origin drops a chosen voice that no longer fits (back to None), and a saved voice that
  does not fit is not shown as chosen. A set whose speaker is a plugin's actor (the
  vanilla ones) is offered only with that plugin loaded.
- **In the game** a voice that does not fit the character (an old save, a SexChange) is
  not used: `player voice: <id> is a <race> <sex> voice and the character a ...; the
  player stays silent`. The choice stays in the save. The test aid's voice is not checked.
- **New sets.**
  - `vanilla.beatrix` (female ghoul): Beatrix Russell speaks with FemaleGenericGhoul, so
    her fight lines are that voice type's shared barks; `set-draft --voice-lines` takes
    every line with a file in the speaker's voice. 34 in use: Attack 6, PowerAttack 4,
    Hit 7, CombatToNormal 4, CombatToLost 4, AvoidThreat 1, reactions 8. Two Followers of
    the Apocalypse lines are off. Rate 1.04.
  - `nexus.mikey` (male ghoul, Big Mike, Nexus 54027, carried): 13 lines.
  - `nexus.deathblow` (male ghoul, Deathblow, Nexus 93930, carried like the other Nexus
    voices: no companion mod is installed, only the voice files the sets use [owner
    2026-11-06]): 4 fight lines (its own CombatStart and CombatEnd topics, mapped by hand)
    and 44 place comments (its advice topic; `set-places` now also places carried lines,
    keeping only the cells of other plugins). Its page asks for permission before its
    assets are used: the files are in the owner's private install only, and
    `mods-used.txt` says to ask the author before the folder is shared.
- **Not used.** Odessa, Crash, Rodney and Greasy Doc John (Deathblow's other ghouls) have
  0 to 2 lines outside conversations. Scratch (Nexus 83135) is a human child (child head,
  FemaleChildScratch), not a ghoul.

## 12. How often the voice speaks (overnight item 64, 2026-11-07)

The owner heard almost no chatter in co-op and none solo (2026-11-06 late playtest).
Why, from the code and the installed sets [observed]:
- Solo, only first-person lines are heard. The sets' biggest pools are not: the `idle`
  lines (109-273 per vanilla set) wait for review (`use_line` false) and nothing triggers
  them; `react.*` lines need another player; most `place` and `quest` lines are not
  first-person (Cass and Willow are the exceptions).
- The player never said the Detection lines on starting or ending a fight
  (NormalToCombat, CombatToNormal ...), though nearly every set has them as first-person
  lines (all six of Cook's lines are of that kind, so Cook was never heard).
- Comments waited 150 s between them, reactions 20 s.

What changed [design]:
- `Game::player_fight_lines`: a line of NormalToCombat (else AlertToCombat, LostToCombat,
  StartCombatResponse) as a fight with the player begins, of CombatToNormal (else
  LostToNormal, AlertToNormal) `FIGHT_OVER_SECONDS` (4) after nobody fights it any more.
- `COMMENT_EVERY` 150 -> 60 s, `REACT_EVERY` 20 -> 10 s.
- `QUIP_GAP` (4 s): no line of the player's voice begins sooner after the last one (not
  for Death), so the extra lines do not pile up. A held combat line falls back to the
  player's own grunt, as without a voice.
- Every trigger that said nothing is logged with its reason.

Still open (owner): the `idle` lines are the one large pool that could make the voice
talk out of a fight; they need a review pass (which are fine said by the player, alone or
to the others) before an idle timer can use them.

## 13. Co-op: every player's voice from its own state, heard by all (item 106, 2026-11-09)

The owner: some lines played only on the speaker's machine, and some fired from another
player's flags. What each trigger did [observed, both the normal and the replication
build, which change none of this]:
- Combat lines (Hit, HealthHalf/Quarter, Crippled, Death, Attack, PowerAttack ...) are
  decided on each player's own game from its own player (a guest's hits come from the
  host as `RemoteHit` and are applied to the guest's own player). A voice-set line went
  to the others; the player's own grunt (GenericPlayer, no set line for the event) was
  never sent: heard only on the speaker's machine.
- Reactions answered another player's deeds. A gunshot was to be answered out of a fight
  only, but the speaker cannot know the shooter's fight (on a guest nobody is ever seen
  fighting it), so every shot of the host's fight made the guest's voice answer.
- The host sent its own player's lines in its line feed too, and a guest's stand-in's
  as that guest's: a player's line could be heard twice, or from the wrong body.

Now [design]:
- A combat line the player says that is not a set line (its grunt, in the voice type of
  `Game::player_own_voice_type`) is sent with that voice type (`party_quips::OWN_LINE`,
  "@own" in the voice table's blob, then the INFO and the voice type) and heard from the
  speaker's character, positional, like a quip; the Party Quips switch does not hold it,
  it shows no subtitle and holds no quips back. One line per event: when a set line went
  to the others for the event, that is what they hear. An older game reads "@own" as a set
  it lacks, warns once and plays nothing.
- Reactions: gunfire is not answered; the deeds left are about the speaker itself (a
  grenade or mine near it, sights held on it), and none in its own fight.
- The host's line feed carries no player speaker: a player's voice goes only by the
  voice table, from its own game, once.

## 14. Follows-Chalk (item 119, 2026-11-09)

`vanilla.followschalk`: Honest Hearts' NVDLC02FollowsChalk (voice type
NVDLC02MaleUniqueFollowsChalk), drafted by `set-draft` like the other vanilla sets: 54
combat lines over 23 events in use, all first-person; his 78 idle and 13 quest lines wait
for review like the other sets'. Companion: himself (the 50/50 split when he follows).
Rate 0.90: the rate moves pitch and speed together (playback speed), so the owner's
"about -2 semitones and 0.92x" is met as about -1.8 semitones at 0.90x; lip timing stays
in step. A set whose speaker's plugin is not loaded is not offered, and the game says so
once: `player voices: <name> (<id>) is hidden: its speaker <ref> is not in the load order`.
