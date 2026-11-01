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

## 7. The dialogue menu, speech, gestures (batch 2)

| Function | NVForever |
|---|---|
| SetDialogueTopicHidden `topicOrInfo flag`, SetDialogueTopicOrder `topicOrInfo order`, ClearDialogueTopicOverrides | rules on the next list built (`Game::build_choices` → `Game::itr_menu`): hidden topics and lines dropped; ordered entries first by their order, the rest after, ties as they were; at most 32 of each; Clear returns how many it removed [documented]. A rule on an info wins over one on its topic [hypothesis] |
| AddDialogueTopicEntry `prompt id`, SetDialogueTopicEntryAlpha `index alpha` | each list build drops the last list's entries and alphas and fires ITR:OnDialogueMenuBuild (the speaker [hypothesis: no event page]), where scripts add them: ids 0-16777215, 16 entries, only while the menu is open or being built [documented]. The engine's menu draws them and calls `Game::itr_topic_selected(id)` when one is chosen (ITR:OnDialogueTopicSelected with the speaker and id): follow-up |
| GetDisplayedDialogueInfos | the lines of the entries shown; no array (0) with the menu closed [documented] |
| ForceSay `topic [target]` | the topic's first line (`GameData::topics`' order), whatever its conditions, said as Say or SayTo; 0 for an actor not present [documented] |
| IsSaying `topicOrInfo` | the line the actor says (Say, SayTo, or its line in the conversation with the player) is of the topic or is the line [documented] |
| Gesture `type amplitude duration [speed]` | nod, shake or tilt in any case, speed 0.4 when 0 or left out, one per actor, 32 actors [documented]; kept for the engine's head bone (`Itr2State::gestures`): follow-up |

## 8. Combat (batch 2)

| Function | NVForever |
|---|---|
| FakeHit `[attacker] [damage] [weapon] [location] [flags] [skipOnHit]`, FakeHitEx `[attacker] [damage] [fatigue] [limb] [weapon] [location] [flags] [skipOnHit]` | the damage, else the weapon's base damage, else 1; dealt as a combat hit (`Game::hit`: difficulty, OnHit, OnHitWith, kill, challenges) or with skipOnHit the bare health damage (`Game::effect_health_damage`); FakeHitEx's fatigue damage and, on locations 1-6, limb damage [documented]. The location table (0 torso … 6 brain) maps to GetHitLocation's 0, 1, 3, 5, 7, 10, 13 and the limb conditions 26, 25, 27, 28, 29, 30, 31; limb damage is taken as condition points [hypothesis]. `flags` is not documented and not read; blood, decals and the crime alarm are the engine's (follow-up) |
| FakeImpact `weapon [material]` | kept for the engine's impact effects (follow-up); 1 for a weapon [hypothesis: the return] |
| ForceCombatTarget `[target]` | the actor starts combat with the target (`Game::start_combat`) and GetCombatTarget names it while the target lives (`Game::combat_target_of`); no target clears it [documented]. Kept for the session (a load clears it) [documented] |
| GetTargetDetectedLocation, …InitialLocation, …LastFullyVisibleLocation, …LastSeenLocation `target` | [x, y, z] of where the actor's awareness last measured the target; 0 with no data [documented: the arrays and the 0]. NVForever keeps one place per target, so the four read the same [hypothesis] |
| ForceReload | kept for the engine's reload (follow-up); 0 with no weapon in hand [hypothesis] |
| RefillAmmo `count` | that many rounds of the weapon's ammo (NAM0) added; the player's magazine topped up to the clip, not past the count; refused for the dead, no weapon, or a weapon with a regeneration rate [documented] |
| UseAidItem `item` | `Game::consume`: one removed, its effects applied; 0 without it or for a non-ALCH [documented] |
| ResurrectActorEx `[flags]`, ResurrectAll | ResurrectActor in place (not the animated stand-up [hypothesis]); bit 0 rolls the base's inventory again (`World::reset_inventory`) [documented]; ResurrectAll every dead actor in the loaded cells around the player (its "high process") and returns how many [documented] |
| Ragdoll `[limb] [x y z] [spin] [duration] [flags]`, RagdollLimb `limb [x y z]` | kept for the engine's physics (follow-up); 1 on an actor [hypothesis]. Ragdoll's page in the export is the ragdoll form's, so its arguments are the catalogue's |
| IsRigidBodyAtRest `[node] [recursive]` | 1 for a reference: the engine does not report its bodies to the game [hypothesis; follow-up] |
| CreateDetectionSoundAt `x y z level`, CreateAnonymousDetectionSoundAt `x y z level radius [flags] [topic]` | a detection event at the point (`Game::queue_detection_event`), the calling actor's or no one's, the level in place of the action sound as CreateDetectionEvent's [hypothesis: the scale]; radius, flags and topic are not read (a radius ≤ 0 refuses) |

## 9. Paths, navmeshes, ground, doors, references (batch 2)

The navmeshes live in the engine (`nvforever` `nav.rs`), so the game asks through a
lender (`crate::nav_ask`, built as `crate::ray`'s ray caster): the engine lends an answerer
for a script tick; without one (headless, and until the engine lends it: follow-up) there
is no path, no navmesh and no cover.

| Function | NVForever |
|---|---|
| CanPathToRef, GetPathDistanceToRef, GetPathNodeCount, GetPathToRef, GetNthPathNode `target [index]`; GetPathLength `target [source]` | the path from the calling reference (GetPathLength: the source actor, else the calling one) to the target: 1/0; its length along the points, -1 without one; its point count (0 without); its points as [x, y, z] arrays; the n-th point (0 without) [documented] |
| IsPointOnNavmesh `x y z [anchor]` | the anchor, the player when none [documented] |
| GetCoverPointsInRadius `x y z radius` | [x, y, z, flags] each, at most 128; 0 for a radius outside (0, 8192] [documented] |
| GetBestCoverFromThreat `threat search radius [max]` | the cover points near the search point that a cast from the threat to the point's chest (48 units up) cannot reach, nearest first, [x, y, z, flags, distance], max 8 by default, 1-32 [documented; hypothesis: the chest height] |
| GetDistanceToGround, GetDistanceToTerrain | posZ − the first hit of a cast down (`crate::ray`, World then Terrain; Terrain only for the terrain), signed; GetDistanceToTerrain 0 indoors [documented]. The cast starts 64 units above the reference [hypothesis: "an internal start value"] |
| MoveToGround, MoveToTerrain `[distance]` | onto it, or down by the distance but not through it, through SetPos Z; 0 when there is nothing below [documented] |
| GetRefExteriorDoor `[ref]` | the reference outdoors; else the fewest-doors way out through the load doors (enabled, teleporting), and the outside door of the last one [documented; hypothesis: "the exterior-side load door" is the door one comes out of] |
| GetRefNextTeleportDoor `[ref] [refresh]` | the first load door on the fewest-doors way from the player's cell to the reference's (cells of one worldspace count as one); 0 when already there or no way [hypothesis: the fewest doors stands for the shortest path] |
| GetRefsSortedByDistance `max [type] [depth] [taken] [heading] [count] [base]` | the references (the player left out) of the cells around the player's within `max` (3D), nearest first: type 0 any, 200 actors, 201 items, else a form type; depth -1 five cells, exteriors only; heading in degrees off the player's facing [hypothesis: the angle's measure]; count the most; base a base form [documented] |

## 10. Perks (batch 2)

| Function | NVForever |
|---|---|
| GetPerkEligibility `perk [delta] [flags]` | the string map perk, name, ownedRank, nextRank, maxRank, eligible, status, blockers, indeterminate [documented]. Status: owned_max (every rank owned, without flag 8), scripted (a trait, hidden, not playable or least level 0 without its flag 4, 1, 2, 16), blocked, unknown, takeable [documented: the names; hypothesis: the order] |
| GetPerkBlockers `perk [delta] [flags]` | the blockers: kind, function, comparison, currentValue, requiredValue, missingValue (≥: required − current; >: one more), argForm, argValue, text [documented; hypothesis: the text "Guns >= 45, have 30" and the missing value of >] |
| GetEligiblePerks `[delta] [flags]` | the eligibility maps of the takeable perks, by least level then name [hypothesis: the order] |
| GetPerkConditions `perk [flags]` | the perk data gates (Level with GetLevel, or NonLevelUp; Trait, Hidden, NonPlayable), the top-level requirements and the entry points' filters, with source, perk, rank, entryPoint, entryFunction, tabIndex, rawFunctionId, commandOpcode, function, runOn (xEdit's order), comparison, comparisonValue (a global's value), isOr, usesGlobalComparison, kind, argForm, argValue [documented] |
| GetPerksForForm `form [delta] [flags]` | the perks with an entry point that acts on the form: one-tab filters with GetIsID, IsInList or IsWeaponInList checked against it ("filter"), no tabs on the combat entry points ("broad"), several tabs with flag 32 when a tab names it ("contextual"), unknown one-tab filters with flags 64 and 128 [documented]; status owned_active, takeable, future (only the level blocks it), blocked, contextual, unknown [documented: the names; hypothesis: the rules]. "Combat-oriented entry points" are the ones NVForever reads in combat (`crate::perks`) [hypothesis] |

The requirement functions ITR evaluates itself are GetActorValue, GetBaseActorValue,
GetPermanentActorValue, GetLevel, HasPerk and GetItemCount [documented]; with a delta array
another function is indeterminate, without one the game's own condition answers it
[documented]. Requirements are combined with their OR flags as conditions are
(`cond::combine`); when they fail, each failed condition is a blocker [hypothesis].

## 11. Recipes, containers, references, menus (batch 2)

| Function | NVForever |
|---|---|
| GetAvailableRecipes `[category]` | the recipes whose conditions pass for the player and whose skill and ingredients the player has (`crafting::missing`); a recipe category filters, another form does not [documented] |
| GetVisibleContainerInventoryCount | how many kinds of item the container holds; NVForever rolls a container's leveled lists when it fills it, so what it holds is what it shows [hypothesis] |
| Duplicate `[count]` | no page: that many copies of the calling reference's base placed at it (PlaceAtMe), the last returned [hypothesis: the name and the catalogue's Ref result] |
| SetRaceAlt `race` | that NPC reference alone takes the race (`Game::actor_race` reads it first), its look rebuilt; not saved [documented]. NVForever keeps the race per reference instead of cloning the base [design: the same result for every reader] |
| ShowBarterMenuBlacklist, ShowBarterMenuWhitelist `list [flag]` | the barter menu with the merchant, the list's items barred from (or alone allowed for) sale in `Itr2State::barter_filter` for the engine's menu (follow-up); the flag as ShowBarterMenu's discount [hypothesis] |
| StartNewGame | `Itr2State::start_new_game` for the engine (follow-up) [documented] |
| CenterOnCellAlt `cell` (COCA) | CenterOnCell; 0 when the cell is not found [hypothesis]; the main menu's new session is the engine's |
| ToggleAllPrimitives (TAP) | the switch for the engine's markers (follow-up); returns 0 as the console toggles [hypothesis] |

## 12. Follow-ups for the engine

The navmesh answerer (`nav_ask::with_nav` around the script tick, as `ray::with_caster`),
the radio's songs and file names (`Game::set_radio_playing`) and the injected queue,
the dialogue camera and camera angle, the custom dialogue entries and their alphas, the
barter filter, gestures, ragdolls, impacts, reloads, eye meshes, the weapon emissive
colour, the UI alpha maps and texture offsets, contact events, the new-game request and
the primitives switch: each is kept on `Game::other.itr` / `itr2` for the engine to read.
The combat AI's weapon search and switch (`ItrKept`) and the dialogue's own Say Once and
Goodbye rules (`Game::info_flags`) wait on those systems.
