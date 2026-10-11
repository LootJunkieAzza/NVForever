# Spec notes: radios

Clean-room description in plain language. Tags: **[documented]** cites a public source,
**[observed]** was measured on the owner's install (FNV 1.4.0.525), **[hypothesis]**
needs verification, **[design]** is our choice where the game's rule is not known.
Counts come from `fnvtool world radio` (fnv.world.radio/1, §7) over FalloutNV.esm alone
unless "ten masters" is said. Sources: the owner's GECK wiki export (pages Radio Stations,
Creating Radio Stations, Radio Tab, Reference, Activator, SetBroadcastState,
GetBroadcastState, StartRadioConversation, SetNPCRadio; 2026-10-09), xEdit's record
definitions (schemas `objects.json`, `placed.json`, `world.json`).

Code: `fnv_world::radio` (records, placed stations and receivers), `fnv_game::radio`
(receivers switched on, broadcast state, the programme as dialogue), `nvf-play`
`radio.rs` (playback). Related: audio.md (sounds, music), dialogue.md (lines, topics),
voice.md (voice files).

## 1. The pieces

- **A station** is a talking activator (TACT) marked as a radio station; each station is
  its own base object [documented: Radio Stations]. Its placed reference is the
  station's voice: the radio quests' lines are said by it (§3). TACT fields: `VNAM`
  voice type, `INAM` radio template (a SOUN), `SNAM` looping sound [documented: xEdit].
- **A receiver** ("radio") is an activator (ACTI) whose `RNAM` names a station; it plays
  that station "when turned on" [documented: Radio Stations, Activator]. Its own `INAM`
  radio template "appears to not work properly when applied to activators, must be
  applied to Talking Activator" [documented: Activator].
- **A placed station reference** carries `XRDO` "Radio Data": range radius, broadcast
  range type (0 Radius, 1 Everywhere, 2 Worldspace and Linked Interiors, 3 Linked
  Interiors, 4 Current Cell Only), static percentage, exterior position reference
  [documented: xEdit REFR; GECK wiki Reference "Broadcast Range Type" and
  GetRadioBroadcastType].

## 2. Records [observed]

- 87 TACT records, 39 with record-header bit `0x00020000` set; 28 of those also have
  `0x40000000`. Ten masters: 132 TACT, 49 and 38.
  - Bit `0x00020000` is set on every station named by a receiver (Radio New Vegas, Mojave Music Radio, Black Mountain Radio, the
    casino radios, the Tops performers, …) and on FO3 leftovers such as Galaxy News
    Radio; the 48 TACTs without it are terminals, intercoms, speakers and a few named
    "Radio" that no receiver names (`RadioTenpennyTower`, `MoriartysSaloonRadio`,
    `BMRadioReceiver`). It stands for the editor's "Radio Station" box [hypothesis].
  - Bit `0x40000000` is the "Continuous Broadcast" box [hypothesis]: among the 11 radio
    stations without it are those scripts start themselves — `SetBroadcastState 1` on
    Repcon's station, `StartRadioConversation RadioHello` on the NCR emergency radio,
    Hanlon's confession, Lily's grandkids' recording and the three Tops performers
    [observed: the masters' compiled scripts]; the others are the EDE frequency and FO3
    leftovers. Radio New Vegas, Mojave Music Radio, Black Mountain Radio and the casino
    stations have it.
  - Bit `0x10000000` is set on 14 stations, meaning unknown.
- Radio templates: 10 TACTs name one (`AMLRadio` min 150, max 1800 units,
  13 dB; `AMLRadioLarge` 20 / 2500, 10.13 dB; `AMLRadioTheTops` 780 / 10000, 13.66 dB;
  `AMLRadioTopsPerformer` 20 / 2500, 2.64 dB). All four name the file
  `fx\xxx_placeholder_silence.wav` and have only the flag Mute When Submerged: they are
  attenuation settings, not sounds. Radio New Vegas and Black Mountain Radio name none.
- 24 receiver bases (ten masters: 39); every `RNAM` points at a TACT base, never at a
  placed reference.
- 220 placed receivers (ten masters: 392), 6 initially disabled (ten masters: 61). By the
  base's script: 134 `VintageRadioStartOnSCRIPT`, 24 `BMVintageRadioBMRScript`, 7
  `VCampGoldLoudspeakerScript`, 1 `GoodspringsRadioScript`, 54 without a script (ten
  masters adds 81 Dead Money speakers with `NVDLC01RadioSpeakerSCRIPT`).
- Placed stations: 43 TACT references (all TACTs). Radio New Vegas and Mojave Music Radio
  sit in the interior `Vault11a` with range Everywhere; Black Mountain Radio has range
  Radius 50,000 with 5 % static; Camp Guardian Radio Radius 5,000 with 50 % static and an
  exterior position reference.

## 3. What a station says [observed]

A station's lines are the Radio-tab lines of its quests [documented: Radio Tab, Radio
Stations: radios "use dialogue from the Radio tab" and start "radio conversations" from
RadioHello as conversations start from HELLO]. Every radio quest of the masters ties its
lines to its station with a quest condition `GetIsID <station>` (for example
`vGSRadioQuest` → `VGSStoreRadio`, `vCountryRadioQuest` → `vCountryRadio`,
`RadioNewVegas` → `RadioNewVegasTA`): the station's placed reference is the speaker the
conditions are run on.

| Station | Quest | Radio lines | With a sound (song) | Voice files |
|---|---|---|---|---|
| RadioNewVegasTA | RadioNewVegas | 216 | 20 | 181 (MaleUniqueMrNewVegas) |
| RadioBlackMountainTA | RadioBlackMountain | 186 | 30 | — |
| vCountryRadio (Mojave Music Radio) | vCountryRadioQuest | 15 | 15 | — |
| vCasinoTopsRadio | TopsRadioQuest | 8 | 8 | 8 |
| vCasinoUltraLuxeRadio | vUltraLuxeRadioQuest | 7 | 7 | 7 |
| vCasinoGomorrahRadio | vGomorrahRadioQuest | 6 | 6 | 6 |
| vNCREmergencyRadioTA | vNCREmergencyRadio | 33 | 0 | 33 |
| RadioNVCampGuardianRadio | vCampGuardianRadio | 8 | 0 | 8 |
| RepconRadioStation | RadioRepconQuest | 3 | 1 | — |

- **Songs** are lines whose response names a sound form (TRDT "Sound"); the sound "will
  play instead of the info's voice file" [documented: Radio Stations]. The 89 such
  responses of the masters' radio quests name SOUN records whose file is an `.mp3` under
  `songs\`; 87 are flagged 2D and Dialogue Sound. 45 distinct song files are named; 43
  exist. The two missing are temporary songs (`MUSNVTempBillyTheKid` in
  `songs\radio\_nv_temp\`, `MUSNVTempBlueMoonStrip` `songs\radionv\temp_bluemoon_strip.mp3`)
  of the one-line quests `vGSRadioQuest` and `StripRadioQuest`, whose stations no placed
  receiver names.
- **Mono files.** For in-world radios the game plays a `_mono` copy of the song beside
  the Pip-Boy's file [documented: Creating Radio Stations: "This mono file will be what is
  played out of speakers and radios in-game"]. All 48 loose MP3s in
  `Data\Sound\songs\radionv\` have a `<name>_mono.ogg` in `Fallout - Sound.bsa`
  [observed].
- **DJ lines** have no sound and are voice files in the station's voice type (Mr. New
  Vegas: `sound\voice\falloutnv.esm\maleuniquemrnewvegas\radionewvegas_…`).
- "Radio Infos can only have a single response" [documented: Creating Radio Stations];
  the masters' radio lines have one response each [observed].

## 4. Receivers on and off

- `VintageRadioStartOnSCRIPT` (134 receivers): OnLoad, once (a variable `init`), the
  reference activates itself; OnActivate passes the activation on (`Activate`)
  [observed: decompiled]. `BMVintageRadioBMRScript` and `VCampGoldLoudspeakerScript` do
  the same with `doonce`. So a receiver is off until activated, a default activation
  switches it [hypothesis: from these scripts' names and shape; the default action of a
  receiver is not described in the wiki], and these scripts make their radios "start on".
  Receivers without a script (the jukeboxes `JukeBoxDirty01`, `NewVegasRadio`, …) stay
  silent until the player switches them on [hypothesis].
- `GoodspringsRadioScript` (the Prospector Saloon's radio `00109087`): OnLoad calls
  `SetDestroyed 1` unless the player knows about the radio; OnActivate by the player
  shows a message unless fixed; fixing it sets `bfixed` and activates it [observed:
  decompiled; layer5-captures.md §8.3 d]. A destroyed receiver is silent [hypothesis].
- In the Prospector Saloon both receivers play Mojave Music Radio: the jukebox
  `00107D73` (`JukeBoxDirtyNVCountry`, StartOn script) and the broken radio.

## 5. The programme

- **Broadcast state**: on or off per station; the default is the base's Continuous
  Broadcast box; `SetBroadcastState 0` stops continuous broadcast ("the station will play
  static except when playing scripted conversations"), `1` starts it ("the station will
  generate and play radio conversations one after another") [documented:
  SetBroadcastState, Radio Stations]. `StartRadioConversation [topic]` interrupts a
  broadcasting station with one conversation and returns to the loop, or plays one
  conversation on a silent station [documented].
- **Order** [observed: the masters' lines]:
  - Mojave Music Radio: 15 RadioHello lines, all Random and Run Immediately, each a song;
    three conditions `GetQuestVariable vCountryRadioQuest <slot> != <song number>` over
    three slots, and a begin script that shifts the slots and stores the song: a song
    does not come back while it is one of the last three played.
  - Radio New Vegas: RadioHello (15 lines) → RNVNewsIntro → RNVNewsStory / Story2 /
    Story3 / DamStory / Transition → RNVNewsPost → RNVMusicIntroGeneric or
    RNVMusicIntroSpecific → RNVMusic (20 songs, each linking to RNVMusic again and
    conditioned on a counter below 3). Lines link on by their Link To topics (TCLT);
    RNVNewsPost links to both music intros.
- **Rules we use** (`fnv_game::radio`):
  - The station's placed reference says lines like an actor, with the player as the
    listener: within a topic the dialogue rule of dialogue.md §4 picks the line
    (quest priority, stored order, Random pools, Say Once per speaker), and the line's
    begin and end result scripts run with the station as their reference [hypothesis:
    the documented "like normal dialogue conversations"].
  - The programme starts with RadioHello [documented]. After a line, of the topics it
    links to, those with a line the station can say now; one of them at random
    [hypothesis: how the game chooses among several links is not documented; taking the
    first would never play RNVMusicIntroSpecific]. With none (RNVMusic after three songs,
    a line without links), back to RadioHello [hypothesis].
  - Half a second of silence between two lines [design].
  - Checked headless (`crates/fnv-game/tests/radio_data.rs`, with game data): Mojave
    Music Radio plays 30 songs with no repeat within three; Radio New Vegas comes round
    news → intro → one to three songs → RadioHello again.

## 6. Playback (`nvforever --play`, `nvf-play` `radio.rs`)

- A receiver plays while it is on (§4), enabled, not destroyed, its station has a
  placed reference and broadcasts, and the player is within the template's maximum
  distance of it.
- One programme per station, shared by its receivers; it starts when one of its
  receivers is first heard and runs on from then, heard or not [design]. A receiver that
  starts while a line is under way joins at the point reached (the file is sought to it)
  [design].
- File: a song's `_mono.ogg` when the archives have it, else the file the SOUN names; a
  DJ line's voice file in the station's voice type; a line with neither lasts its
  fitted text length in silence. Length from the Ogg headers (voice.md §5).
- Loudness [design]: the song SOUN's static attenuation (12.33 dB for the RNV songs
  looked at); DJ voice files at full level. By distance: the station's radio template,
  else the receiver's, else 150 / 1800 units (AMLRadio's), as `sfx::distance_gain`
  (full within the minimum, falling as minimum ÷ distance, faded out at the maximum).
  The template's own static attenuation is not applied. Not panned.
- Pause: the Esc menu and loading screens hold sounds and programmes, as for placed
  loops (audio.md §7).
- Log: `radio: <station> says <topic> <info>: <file> <seconds> s`, `radio: <receiver>
  tuned to <station> playing <file> [from <s> s]`, `radio: <receiver> (<station>)
  switched on|off`, `radio: <receiver> (<station>) is destroyed: silent`, and at the end
  `radio summary: N receivers heard, N songs started, N DJ lines, …`.
- Smoke test (CLAUDE.md): the Prospector Saloon with the jukebox switched off at 20 s
  and on again at 28 s. Observed: `00109087 (vCountryRadio) is destroyed: silent`; the
  jukebox switched on by its OnLoad, `vCountryRadio says RadioHello 0x0016B80E:
  sound\songs\radionv\mus_roundhouse_rock_mono.ogg 167.9 s`, off, then on again "from
  20.2 s"; `radio summary: 1 receivers heard, 1 songs started, 0 DJ lines, …; receivers
  switched 3 times, 1 on at exit`. In Boone's room in Novac (`0016CC35`, 90 s) Radio New
  Vegas played nine DJ lines from RadioHello through RNVMusicIntroGeneric and then a
  song.

## 7. Tools

`fnvtool world radio <Data> <plugins> --schema schemas/fnv` (fnv.world.radio/1):
`counts` (talking activators and their header flags, placed station references by
range type, receiver bases and placements by script, the stations' lines and song
files), `stations` (flags, voice type, template, placed references with `XRDO`, the
quests that `GetIsID` them, their lines by topic and file type, receivers),
`receiver_bases` (station, template, script, placements) and `goodsprings_receivers`.
No texts are printed.

## 8. Open and not built

- Not built: the Pip-Boy radio (tuning, the station list, a station's broadcast range
  and static, which decide what a Pip-Boy receives); `StartRadioConversation` (so the
  Tops performers, Hanlon's confession, Lily's recording and the NCR emergency radio,
  which only speak through it, stay silent), `ForceRadioStationUpdate`, `SetNPCRadio`
  (an actor as a radio), `SetRadioToDialogueOnlyMode`; static between stations; panning
  and reverb; subtitles of radio lines.
- `GetBroadcastState` still reads 0 for a station no script has set, while playback
  takes the base's Continuous Broadcast flag as the default (§5): which one the game
  does is a capture question.
- Capture questions for the owner: whether a scriptless jukebox starts off and one
  activation switches it on; how loud a radio is at a distance (the templates' curve and
  whether their static attenuation applies); whether DJ lines and songs are as loud as
  each other; whether a station runs on while no receiver is heard and where a receiver
  joins; which linked topic a station takes when several have lines; the pause between
  lines.
