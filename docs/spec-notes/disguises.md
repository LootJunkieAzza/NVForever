# Spec notes: faction disguises

What makes faction armour a disguise, and who sees through it. Read from FalloutNV.esm
(records and the script source the masters carry, SCTX) on 2026-11-15 for #2611
(Arizona Killer: the NCR ranger armour Cato Hostilius gives the player). Code:
`crates/fnv-game/src/functions/actor.rs` (`player_enemy_of_factions`),
`crates/fnv-game/src/crime.rs` (`crime_enemy`), `crates/fnv-game/src/combat_group.rs`
(`raise_alarms`). The reputation side (reputations set to 0 while worn) is in
reputation.md.

## 1. The records [observed: FalloutNV.esm]

- **The armour.** The form list FactionGearNVNCR (0x000EE69A) holds the NCR pieces the
  scripts test with GetEquipped, among them ArmorNVNCRRangerCombat (0x00129254) and
  ArmorNVNCRRangerPatrol (0x000CE553); both carry NCRFactionOutfitWarningScript
  (0x0012B03C). HelmetNVNCRRangerCombat (0x00145EC5) carries no script. The other
  factions' outfits have scripts of the same shape (Legion, Brotherhood, Great Khans,
  Powder Gangers, White Glove Society).
- **The factions.** For the NCR:

  | faction | what its record says |
  |---|---|
  | NCRFactionNV 0x000A46E7 | Track Crime; Neutral to PlayerFaction; Enemy to the Legion and to the other outfits' `…Enemy` factions |
  | ArmorNCRFactionNV 0x00134B6C | Hidden, Track Crime; one reaction: Friend to itself |
  | ArmorNCRFactionNVEnemy 0x00134B47 | Hidden; Enemy to the Legion, Brotherhood, Great Khans, Powder Gangers, VBSOscarVelascoFaction |
  | VNCRSnifferFaction 0x0012676F | Hidden; Neutral to PlayerFaction, Ally to NCRFactionNV and itself |

  Every NCR soldier of Hoover Dam that the Arizona Killer assassination passes is in
  ArmorNCRFactionNV besides NCRFactionNV: Private Watson, Colonel Moore, Captain Graham,
  the K9 ranger (VHDKimballK9Trooper01REF), the MPs, the sharpshooter and Kimball's two
  ranger guards. All but Watson and Moore are in VNCRSnifferFaction as well (through the
  templates VHDKimballRangerTemplate and vHDRangerKimballGuardTEMPLATE, or their own
  lists) (`fnvtool actor factions`).
- **The pulse.** The spell DisguiseFactionPulseActorEffect (0x001413D1): one effect,
  DisguiseFactionPulseEffect (script effect, DisguiseFactionPulseScript 0x001411A4),
  magnitude 1, area 25, duration 5 s, range Touch; SPIT flags 0x60 (Area Effect Ignores
  LOS clear). The quest DisguiseFactionPulseQuest (0x001411A8) has a 1 s script delay.

## 2. What the scripts do [observed: the SCTX of the scripts named]

- **Putting the armour on** (NCRFactionOutfitWarningScript, OnEquip by the player):
  keeps the reputations and sets them to 0 (reputation.md), shows the tutorial and the
  NCR warning messages, starts DisguiseFactionPulseQuest, makes PlayerFaction and
  ArmorNCRFactionNV friends both ways (SetAlly with both flags 1) and puts the player in
  ArmorNCRFactionNVEnemy, so that the Legion, the Khans, the Brotherhood and the Powder
  Gangers take the player for NCR. When the player has sided the Brotherhood or the
  Khans with the NCR (VStoryEventBoS, VStoryEventKhans), their armour factions are made
  the player's friends too.
- **Taking it off** (OnUnequip, when no FactionGearNVNCR piece is left on): stops the
  quest, makes PlayerFaction and ArmorNCRFactionNV enemies both ways (SetEnemy with both
  flags 1, which is Neutral by its documented flags), takes the player out of
  ArmorNCRFactionNVEnemy and gives the reputations back.
- **The pulse** (DisguiseFactionQuestScript, every run of the quest's script): the player
  casts the pulse on itself (CastImmediateOnSelf); every 300 s a reminder message.
  The script's own notes say it filters the references within the radius, and a sniffer
  it finds is taken out of the disguise faction, "thus potentially turn[ing] them
  hostile to players with whom they have less than stellar relations".
- **The pulse's effect** (DisguiseFactionPulseScript): on start, on anyone but the
  player, a member of VNCRSnifferFaction leaves ArmorNCRFactionNV (the other factions'
  sniffers leave theirs); when the effect finishes the sniffer is put back.
- **A sniffer that starts fighting the player** (NCRSnifferScript, CrHDDogNCRBuddyScript,
  VHDKimballMPScript, VHDKimballMP01SCRIPT, VHDRangerKimballGuard01/02SCRIPT,
  VHDCaptainGrahamSCRIPT, HVRangerScript; OnStartCombat Player): when the pulse quest
  runs and the sniffer holds the pulse's effect (IsSpellTarget), SendAssaultAlarm on the
  player for NCRFactionNV and for VNCRSnifferFaction, and the sniffer's linked handler
  challenges the player when within 512 units.
- **Kimball's speech** (VHDKimballSpeechSCRIPT): the guards are alarmed when the player
  has a weapon out (no camera or binoculars) within 3,072 units of
  VHDAssassinationSetupREF while carrying weapons is not allowed and the NCR fame is
  under the third threshold, unless a Stealth Boy runs. The disguise sets the fame to 0,
  so a disguised player must keep the weapon away there.

## 3. Who sees through the disguise

- **Only sniffers, only near** [observed: §1, §2]: a soldier stays in ArmorNCRFactionNV,
  the player's friend, unless it is a sniffer the pulse reached: within the effect's
  area, 25 feet (about 533 units, area-effects.md §2) of the player, and in sight (the
  spell does not ignore line of sight). Watson and Moore are no sniffers and are never
  taken out. The effect lasts 5 s and the pulse comes back every second, so a sniffer
  near the player stays out of the faction, and is back in it about 5 s after the
  player is away.
- **Seeing through is not attacking** [observed: §1]: out of ArmorNCRFactionNV a
  sniffer is left with NCRFactionNV and VNCRSnifferFaction, both Neutral to the player.
  It attacks only a player the NCR have something against: a low reputation (which ours
  does not yet turn into hostility, reputation.md §4), a faction enemy flag
  (SetPCEnemyOfFaction NCRFactionNV, a witnessed murder), or a grudge of its own.
- **The friendship outranks the player's bad standing** [hypothesis: what the pulse is
  for, by the script's notes; the original not probed, observer-checks.md check 173]:
  the pulse would change nothing if the NCR's hostility won over ArmorNCRFactionNV's
  friendship, and VHDControlRoomGuardTrigger2SCRIPT, which turns the NCR on the player
  for good, makes both NCRFactionNV and ArmorNCRFactionNV enemies of the player. Ours:
  a faction enemy flag of one of an actor's factions does not count while another of
  its factions is the player's ally or friend (`player_enemy_of_factions`); a flag on the
  friendly faction itself does (a crime among the player's friends stays a crime), and
  so do an actor's own crime grudges (it was the victim or saw the crime). A faction
  reaction of Enemy yields to a shared ally or friend faction too (factions.md §4,
  #2560).
- **Blowing the cover** [observed: §2]: a sniffer near the player that detects the
  player and starts fighting sends the assault alarm for all NCR; that, not any
  sight of the disguise from afar, is how the whole dam turns on the player.

## 4. Alarms and the disguise [design]

Ours passes an alarm to an actor's allies when it starts fighting and (NVForever's own
addition) when it only notices a hostile, so a camp wakes as one (combat-groups.md §1,
`combat_group.rs` `raise_alarms`). A hearer that would not attack the foe itself joins
with a grudge, but never against its own ally or friend (`comrades`). A soldier the
disguise fools counts the player as a friend (ArmorNCRFactionNV), so no alarm, from a
glimpse or a fight, turns it on the player; before #2611 the faction enemy flag made the
player its enemy, and one sniffer's notice (#2611: the K9 ranger at 3,519 units, value
-14.9) went round the dam. Only the sniffer scripts' SendAssaultAlarm (§2) speaks for the
whole faction when a sniffer near the player starts fighting.

## 5. Open

- Whether the original lets the disguise friendship outrank a faction enemy flag (a
  murder seen by the NCR, SetPCEnemyOfFaction) the way ours does: observer-checks.md
  check 173, owner-review.md item 81.
- The "unknown" shot of #2611 (a hit on the player for 54.2 with no attacker logged, the
  NCR measured 4,960-4,990 units off and hidden): not reproduced without the owner's
  log; the hit log names no attacker when the shot came from no actor the engine knows
  (a turret's or a scripted projectile, or a shooter not in the high process).
- Reputation's hostility (Vilified NCR attacking on sight) is not built (reputation.md §4);
  when it is, the disguise's reset of the reputations to 0 is what hides it.
