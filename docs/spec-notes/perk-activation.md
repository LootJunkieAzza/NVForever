# Spec notes: perks' activate choices — Cannibal and Mister Sandman

Clean-room description in plain language. Tags as in the other notes: **[documented]**
(public source cited), **[observed]** (read from the owner's masters with `fnvtool esm
decode` and `fnvtool script decompile`; nothing quoted beyond names, labels and FormIDs),
**[hypothesis]**, **[design]**. Sources: the GECK wiki's Entry Point page (the owner's
export, `Misc.xml`), the function catalogue (StartCannibal, GetCannibal, HasBeenEaten,
GetSandman), xEdit's PERK definition.

Code: `fnv_game::activate_choices` (the choices, the acts), `fnv_game::data::EntryPointDef`
(EPF2, EPF3, the entry's script unit), `fnv_game::crime::player_cannibalized`; the engine's
`script.rs` (`Runtime::by_hand` opens the list, `Runtime::pick_activate_choice`),
`script_hud.rs` (`activate_panel`), `activate.rs` (the default action activated again).
Tests: `crates/fnv-game/tests/perk_talk.rs`. Smoke: `perk-cannibal-devour`,
`perk-sandman-refused-awake`. Capture jobs: `tools/capture/probes/actor_changes.toml` ac4, ac5 (run
2026-10-18; ac5 and ac5b crashed the game on an awake and an unconscious victim; ac5c, on
a sleeping one, did not).

## 1. The records [observed]

Five PERK entries in the ten masters carry a button label (EPF2): Cannibal "Devour",
Ghastly Scavenger "Devour", Dine and Dash "Dine and Dash" (FalloutNV.esm), Mister Sandman
"Murder", and Honest Hearts' Devour. All five:

- PRKE type Entry Point, DATA `1B 09 02`: entry point code 27, function 9, and a third
  byte 2 [observed]. The wiki's entry point list starts with "Activate: run when the
  player activates something; presents a choice list to the player that can trigger a
  script" [documented]; code 27 is that one and function 9 "Add Activate Choice"
  [hypothesis by elimination: no other entry carries a button or a script]. The third
  byte is taken as the number of condition tabs [hypothesis].
- EPFT 4 (a script), EPF2 the label, EPF3 Run Immediately off, then SCHR/SCDA/SCRO: the
  entry's script [documented: xEdit PERK].
- Conditions: PRKC 0 on the perk owner (IsSneaking 1, IsChild 0, Mister Sandman's tab 0
  only IsSneaking), PRKC 1 on the activated thing (Cannibal: HasBeenEaten 0, GetDead 1,
  GetIsCreature 0 or GetIsID of one creature; Ghastly Scavenger: creature types 4 or 5;
  Mister Sandman: not a child, not essential, alive, conscious, not in
  MisterSandmanFaction, not a creature, GetSleeping not 0) [observed].
- Scripts [observed]: Cannibal `StartCannibal Player`, `Player.RestoreAV Health 25`,
  `Hunger 25`, `DamageAV RadiationRads 2`, `RewardKarma -1`, `ModPCMiscStat "Corpses
  Eaten" 1`, and Meat of Champions' effect when the player has that perk; Ghastly
  Scavenger 50/50/5; Dine and Dash gives `NVCannibalFood` (Human Remains); Mister Sandman
  `StartMisterSandman Player`, `RewardXP 50`, `ModPCMiscStat "Sandman Kills" 1`.
- Idles [observed]: `StartCannibal` (`3rdP_Cannibal.kf`, condition GetCannibal 1);
  `SandMan` and `SandManFloor`, folders conditioned on GetSandman 1, with children
  SandManBedLeft / BedRight / FlrLeft / FlrRight conditioned on GetFurnitureMarkerID
  1 / 2 / 3 / 4. Sounds: `NPCHumanPerkCannibal`, `NPCHumanPerkMrSandman`.

## 2. The choice list (NVForever's rule)

- When the player activates something by hand (E, or a test aid's activation), the
  entries of the perks the player owns (the rank rule of `crate::perks`) whose tab 0
  conditions pass on the player and tab 1 conditions on the target are offered
  [hypothesis: tab 1 is the target]. Offered: a box titled with the target's name listing
  the labels in the entries' priority order, then the default action — Search on a
  corpse, else Talk [design: the wiki's "choice list"; whether the original adds the
  default action and how it shows the list is to be seen in game, owner-review]. The box
  is a message box menu (MenuMode 1001 [hypothesis]); 1–9, a click or Enter picks.
- A picked choice runs its script with the target as the calling reference and the
  player as the action reference [observed: the fragments name no reference for
  StartCannibal / StartMisterSandman, so the act starts on the target]. The default
  action activates the target again as E does, without the list.
- Run Immediately is read but changes nothing: every shipped entry has it off.

## 3. The acts

- **StartCannibal eater** on a victim: GetCannibal 1 on the eater at once and for the
  clip's 7.667 s (`3rdP_Cannibal.kf`) [documented: "while in the cannibalism animation";
  observed: capture ac4, 1 at once and still 1 after 600 frames; hypothesis: that the act
  lasts the clip — in ac4 the player was not at the corpse and the act had not ended];
  HasBeenEaten 1 once the act is over, saved [observed: ac4, still 0 600 frames after
  the call; hypothesis: that it turns 1 at the end]; the eater plays the StartCannibal
  idle and its sound [hypothesis by name]. "A crime against nature" when seen [documented:
  Cannibal's DESC]: every witness turns on the player as for an assault [hypothesis].
  The catalogue says the eater "regains some health" [documented], but StartCannibal
  itself changed neither health nor karma [observed: ac4]: both come from the perk
  scripts' RestoreAV and RewardKarma [observed].
- **StartMisterSandman killer** needs a victim asleep in a bed (GetSleeping 3). On Ringo,
  asleep in his bed at 23:00 (GetSleeping 3, the player MoveTo'd beside him), it worked:
  GetSandman 1 at once and after 60 and 180 frames, 0 after 420; Ringo alive (GetDead 0)
  at once and after 60 and 180 frames, dead after 420 [observed: capture ac5c,
  2026-10-18, nvftest]. So the victim dies, silently [documented: "the silent
  sleeping-NPC kill"], when the act ends, which NVForever puts at the SandMan clips'
  5.967 s [observed: the four `SandMan*.kf`; hypothesis: the end is the clip's, the
  capture bounds it between 3 and 7 s]; the kill is the killer's, so karma and murder are
  judged as for any kill by the player (who the original records as the killer is not
  captured: the GetKiller step did not compile). Called by a script on an awake actor,
  and on one a script made unconscious, it crashed the original [observed: captures ac5
  and ac5b, once each, on Joe Cobb]: NVForever refuses it on any victim not asleep in a
  bed, and nothing happens (the call fails in the trace) [design: the crash is not
  copied]. The killer plays the SandMan child idle whose GetFurnitureMarkerID condition holds on the
  victim, else the first [hypothesis], and the sound.
