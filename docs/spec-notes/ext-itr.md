# The ITR NVSE plugin's functions (2026-11-01, #1011)

ITR's (itr-nvse's) script functions as NVForever builds them: the pure ones in the
script VM (`crates/fnv-script/src/vm/itr.rs`), the ones on the game in
`crates/fnv-game/src/nvse_itr1.rs` (batch 1) and `nvse_itr2.rs` (batch 2). Sources: the
GECK wiki export [documented: the page named]. ITR is not installed in the probe game, so
nothing here is `[observed]`; what a page leaves open is marked `[hypothesis]` and is the
first thing to check if ITR is ever installed for a probe run
(`tools/capture/jip_probes.py`). Four names have no page (Duplicate, Sv_Join, Sv_Reverse,
Sv_TrimStr): their behaviour is chosen from the name, the catalogue's parameters and the
neighbouring pages, all `[hypothesis]`.

ITR keeps most of its state in the plugin, not the save: a load clears it [documented:
the pages that say so; hypothesis for the others]. NVForever keeps it in
`Game::other.itr` (a load builds a new game state, so it goes). Three settings are kept
"in static memory" until the game is restarted [documented: SetFallDamageMult,
SetNoWeaponSearch, SetPreventWeaponSwitch]: they ride across a load with the event
handlers (`ItrKept` in `Game::take_session`).

## 1. Strings (the VM)

| Function | NVForever |
|---|---|
| Sv_Join `array [delimiter]` | the array's values in its order, each as ToString writes it, the delimiter between them; no delimiter joins them as they are [hypothesis: no page; the catalogue's parameters, Python's `join`] |
| Sv_Reverse `text` | the characters in reverse order [hypothesis: no page] |
| Sv_TrimStr `text` | white space taken off both ends [hypothesis: no page] |

## 2. Actors (batch 1)

| Function | NVForever |
|---|---|
| GetFallDamageMult `[actor]`, SetFallDamageMult `mult [actor]`, ClearFallDamageMult `[actor]` | the actor's override, else the global multiplier, 1 when unset; below 0 kept at 0; an actor's 1 clears its override; ClearFallDamageMult with no actor at all clears every override and the global one [documented]. The engine's falls (`nvf-play` `falls.rs`) multiply the damage by it. Kept across a load [documented] |
| DisableCrouching `disable`, ForceCrouch `crouch` | `Game::sneaks` (read by IsSneaking and the engine's NPC stance) answers ForceCrouch's word, else DisableCrouching's "no", before every other rule [documented: "regardless of what the combat AI, follower behaviour, or movement packages request"; ForceCrouch "bypasses DisableCrouching"]; a new DisableCrouching call drops a forced stance [hypothesis]. 0 on a non-actor [documented] |
| Set/GetNoWeaponSearch, Set/GetPreventWeaponSwitch `flag` | the switch, kept across a load [documented]. NVForever's combat AI has no mid-combat item search or weapon re-choice to switch off yet: follow-up for the combat AI to read `ItrKept` |
| Set/GetOnContactWatch `flag` | the switch [documented]; the ITR:OnContactBegin/End events need the engine's physics contacts: follow-up |
| SetAlertNS `toggle [seconds]`, GetAlertNS | alert for the seconds (at most 86400) or until cleared; 0 clears it and its timer; a timer that runs out while the actor fights holds until combat ends [documented]. GetIsAlerted reads it (`Game::is_alerted`). GetAlertNS: seconds left, -1 with no timer, 0 untracked [documented] |
| GetHairColorAlt `[channel] [npc]`, SetHairColorAlt `r g b [npc]` | the NPC base's HCLR: channel 1-3 red, green, blue, else red + green·256 + blue·65536 (GetHairColor's packing) [documented; the packing observed for GetHairColor]; the setter keeps each channel's low 8 bits [documented]; 0 for no NPC |
| SetCreatureCombatSkill `value [creature]` | the creature base's DATA combat skill, at most 255 (below 0: 0 [hypothesis]); GetCreatureCombatSkill reads it [documented] |

## 3. Camera, keys, eyes (batch 1)

| Function | NVForever |
|---|---|
| Set/GetAimZoomFirstPersonOnly | the switch; SetAimZoom… returns the new setting [documented]; the INI's start value is not read (no ITR INI in NVForever), so off [decision] |
| SetCameraAngle `enable x y z` | the override in degrees, 0 releases it [documented]; returns 0 (the page's type is None) [hypothesis] |
| SetDialogueCameraEnabled, …Mode `0-3`, …FixedAngle `0-11`, …Angle `-1/0-11`, …AngleEnabled `angle on`, …Dolly `speed max [once]`, …Shake `amplitude` | the settings (`DialogueCamera`): out-of-range values return 0, FixedAngle returns the angle, Angle the angle chosen (-1 picks one of the allowed angles at random), Shake -1 the default 3 [documented]; AngleEnabled returns 1 done / 0 out of range [hypothesis] |
| DisableKeyEx `key [mask]`, EnableKeyEx `key [mask]` | DisableKey/EnableKey with mask groups: a key stays disabled while a group holds it; EnableKeyEx without a mask releases it outright [hypothesis: the page says only that the mask "releases keys that were disabled under the matching group mask"]; each fires ITR:OnKeyDisabled / ITR:OnKeyEnabled with the key and mask [documented: the events; hypothesis: their parameters, no page]; 0 for a code that is no key |
| AddRaceEye `race eyes`, RemoveRaceEye, ClearRaceEyes `race` | the race's eye list (GetRaceEyes and the race menu read it, the generic list 1): only playable eyes join [documented: AddRaceEye]; 1 when the list changed [hypothesis: no return documented] |
| SetEyeMesh `eyes left [right]`, ClearEyeMesh `eyes` | the meshes by eyes form, right = left when not given [documented]; for the engine's head model on its next Update3D: follow-up |

**Engine follow-ups**: the aim zoom switch, the camera angle override and the whole
dialogue camera (NVForever's dialogue view has no ITR camera yet), eye meshes, contact
events.

## 4. Records (batch 1)

| Function | NVForever |
|---|---|
| GetCasinoBan `casino`, SetCasinoBan `casino banned` | banned when the player's earnings there reached the casino's max winnings; SetCasinoBan 1 sets the earnings to the max (making the entry) and the winnings level 4, 0 drops them to max − 1 where there is an entry, level 3 [documented]; 0 for a non-casino |
| ModChallenge `challenge amount` | forwards in steps of what the threshold takes, so one call crosses a recurring challenge's threshold more than once [documented: "Handles overflow"]; backwards lowers the progress, not below 0 and not undoing a completion [hypothesis] |
| GetWorldspaceOffsetScale, …X, …Y `worldspace` | WRLD ONAM's world map scale, cell X and Y offsets [documented: the pages; the fields: xEdit wbWorldMapOffset]; 0 for no worldspace |
| GetDialogueInfoFlags `info`, SetDialogueInfoFlags `info flags` | INFO DATA's two flag bytes, the second from bit 8 [documented: the flag table]; the setter writes all 16 bits for the session, read back by the getter (`Game::info_flags`); the dialogue's own Say Once and Goodbye rules still read the record: follow-up |

## 5. The radio (batch 1)

| Function | NVForever |
|---|---|
| IsRadioPlaying, GetPlayingRadioTrack, GetPlayingRadioTrackFileName, GetPlayingRadioText | what plays: a station's line from its start to its end (`Game::radio_line_begins`/`_ends`), or what the engine reports (`Game::set_radio_playing`); the track is the line's info (or a song's sound), the text the info's first response, "" for a song [documented]. The engine does not report songs and file names yet: follow-up |
| ChangeRadioTrack | a skip for the engine; 0 with nothing playing [documented] |
| PlayRadioFile `path`, QueueRadioTrack `path`, ClearRadioQueue, GetRadioQueueSize, SetRadioQueueLoop `on` | the injected queue: relative .wav/.ogg paths only, 8 tracks; PlayRadioFile puts its file first (the last dropped when full) and needs the Pip-Boy radio on a station; ClearRadioQueue returns how many it took off [documented]. Playing the queue is the engine's: follow-up |

## 6. UI and items (batch 1)

| Function | NVForever |
|---|---|
| WatchTileValue `tile trait`, UnwatchTileValue `id` | a positive id (the same for a trait already watched; 0 when the trait is not there or 48 are watched); each frame a changed value fires ITR:OnTileValueChange (numbers) or ITR:OnTileStrValueChange (text); a watch whose trait is gone (its menu closed) is dropped [documented]; the events' parameters (id, path, value) [hypothesis: no page] |
| SetUIAlphaMap `tile texture`, SetUITexOffset `tile x y` | kept by tile path for the tile renderer; the offsets wrapped into 0-1, image and radial tiles only [documented]; 1 when the tile is there. Drawing them is the engine's: follow-up |
| GetItemModFlags, SetItemModFlags `[flags]` | on an inventory reference: the weapon's mod bits 1, 2, 4; the setter defaults to 1, 0 takes them off [documented]; as GetWeaponRefModFlags |
| SetWeaponEmissiveColor `r g b mult`, ClearWeaponEmissiveColor | kept for the engine's first-person weapon materials (follow-up); 0 with no weapon in the player's hand [documented: "no first-person weapon node"; hypothesis: the weapon in hand stands for the node]; Clear returns 0 when nothing was set [documented] |
