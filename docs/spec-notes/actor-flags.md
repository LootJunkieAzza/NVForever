# Actor flags and actor data from the ESM (ESM gap audit, 2026-10-22)

The ESM gap audit's medium group "Actor flags"
([esm-gap-audit-2026-10-21](../handoff/esm-gap-audit-2026-10-21.md)) lists NPC_ and CREA
flags and related records that were decoded but never used. Meanings are from the GECK
wiki pages Category:NPC, Class, Stats Tab - NPC, VoiceType, LeveledItem and
Category:Factions [documented]; bit layouts from xEdit (schema `gameplay.json`). Counts
are from FalloutNV.esm [observed: a raw read of the master, 2026-10-22]: 3,816 NPC_ and
1,578 CREA records.

## 1. No VATS Melee (ACBS bit 20)

"The player cannot go into VATS with this NPC if he has a melee weapon equipped"
[documented: Category:NPC]. One NPC (`vl38SuiteVendor`) and ten creatures, all ceiling
turrets. "Base Data does not include … No VATS Melee" [documented: Template Data], so the
flag is read off the actor's own record (the leveled entry it resolved to). In VATS mode
(docs/design/vats-mode.md) a mark on such an actor with a melee or unarmed weapon is
refused and logged; with a gun it is marked as before.

## 2. No Blood Spray, No Blood Decal (ACBS bits 11, 12)

Seven creatures have both: the three tumbleweeds, two of the explosive Nellis ants and two
spawn tests. A hit on such an actor plays its impact's sounds, but its impact model (the
spray) and its decal are not shown. The flags are read from the definition that supplies
the actor's model ("Use Model/Animation includes the Blood … tabs" [documented:
Category:NPC]; hypothesis: the two flags go with that tab). Logged at exit: `shot fx: …
impacts on actors flagged No Blood Spray not shown`, `decals: … hits on actors flagged No
Blood Decal left none`.

## 3. CREA ACBS bits 27–31 (corrected 2026-10-24)

The schema followed an xEdit list that names bit 27 twice ("Not Pushable / Allow
Pickpocket"), so every later name sat one bit too low. The GECK wiki's ActorHasBaseFlag
table gives, for creatures, 27 Not Pushable, 28 Allow Pickpocket, 29 Is Ghost, 30 No
Rotating To Head-track, 31 Invulnerable [documented: owner's export, functions.xml]. The
records agree [observed: raw read of the ten masters, 2026-10-24]:

| Bit | Name | FalloutNV.esm | Examples |
|---|---|---|---|
| 28 | Allow Pickpocket | 124 | wild dogs, giant rats, big horners, ED-E's upgrade forms |
| 29 | Is Ghost | 6 (DM 4, OWB 4) | the tumbleweeds, Festus, the Gun Runners' protectron, Dead Money holograms, OWB's brain tank |
| 30 | No Rotating To Head-track | 4 | the tumbleweeds, CG02Andy |
| 31 | Invulnerable | 10 (DM 20, OWB 4) | Mr. House, Yes Man and his mainframe, House's securitrons, Festus, ARCHIMEDES, the Dead Money holograms |

What this changes:

- **Invulnerable** (`Game::is_immune`) now holds for the creatures above; before, the
  tumbleweeds and CG02Andy (bit 30) were invulnerable and Mr. House was not.
- **Is Ghost** is applied: the actor counts as SetGhost 1 from the start ("cannot be
  harmed … Projectiles pass through them … Enemies … will not go into combat or detect"
  [documented: GECK wiki SetGhost notes]): `Game::is_ghost` reads the flag from the
  definition supplying the base data, so shots and blows pass through such creatures
  (`nvf_play::combat::ghosts`), they take no damage and enemies ignore them. SetGhost 0
  clears it for that reference [hypothesis: the script wins over the base flag];
  GetIsGhost reads both. The earlier note here said "Is Ghost" (then read off bit 28)
  could not mean SetGhost's ghost because dogs and rats had it: that was bit 28, Allow
  Pickpocket.
- **No Rotating To Head-track** moves from bit 29 to bit 30 (actor-fields.md §1).
- **Allow Pickpocket**: docs/spec-notes/pickpocket.md.

Smoke: `ghost-tumbleweed` (shots pass through a tumbleweed), `blood-none-ant` (the No
Blood checks of §2 now on an explosive Nellis ant, as the tumbleweed is a ghost).

## 4. Allow PC Dialogue (CREA ACBS bit 21)

170 creatures have it, among them the securitrons, Mister Handys, ED-E's upgrade forms,
Yes Man's mainframe and some feral ghouls. A creature is offered for talk (the activate
prompt says Talk) when it has the flag and a greeting to say. Before, the greeting alone
decided ([hypothesis: both]; the CREA page is not in the wiki export). It is read from the
definition supplying its base data. As before, a hostile creature only shows its name.

## 5. NPC_ NAM6 Height

0 on 2,601 NPCs and 1.0 on 1,124; the rest lie between 0.85 and 1.1. The drawn scale of an
NPC is the reference's scale × the race's height for its sex (actors.md §3) × its own
height, where 0 counts as 1 [hypothesis: the NVSE functions GetNPCHeight / SetNPCHeight
read and write it; the wiki does not say how it is applied]. Hit volumes and the
character capsule are not scaled by it.

## 6. Chance none on leveled actor lists (LVLN, LVLC)

"The chance that the leveled list will not generate an item" [documented: LeveledItem];
actor lists are taken to work the same [hypothesis]. Before it picks an entry, a list
with a Chance none above 0 rolls 1–100 against it and stands for no actor when the roll
is within it. The engine's appearance resolve (`fnv_actor::resolve`) and the game's
(`fnv_game::templates`) roll it the same way from the reference's seed, so they agree. A
list with 0 draws nothing for it, so its picks are unchanged. Every list in FalloutNV.esm
has 0, so nothing changes in vanilla.

## 7. Classes (CLAS): nothing to build

Auto-calc NPCs (2,283 in the master) carry the attributes and skills the editor
calculated in their own DNAM and ACBS. The wiki gives no formula ("The level and class of
the NPC is used to determine the final values" [documented: Stats Tab - NPC]), so the
stored values stand. The class's services already serve Autocalc Service (vendor-services).
The Guard flag has no documented effect besides the NVSE GetClassFlag. Tag skills,
attributes and the Guard flag are not decoded.

## 8. Special Combat (FACT flag bit 2)

"If a faction is flagged as Special Combat, members can attack one another without
sending a combat alarm" [documented: Category:Factions]. Four factions:
`VPitFighter01Faction`, `VPitFighter02Faction`, `VThornDwellers`,
`vHDNCREngineerDialogueFaction`. When an actor starts fighting or notices one with whom it
shares such a faction, it sends its group no alarm (docs/design/combat-ai.md §12.3). Logged:
`combat group: … sends no alarm about …: both in …, a Special Combat faction`. Not built:
the wiki's other effect, ignoring fSneakSeenMin and iCombatTargetPlayerSoftCap.

## 9. Allow Default Dialog (VTYP DNAM bit 0)

"If checked, voice type includes access to generic dialogue … If unchecked, a line of
dialogue must include one of the following condition functions for it to be valid for
this voice type: GetIsVoiceType, GetIsID, GetInFaction, GetIsClass, GetIsRace, IsInList"
[documented: VoiceType]. The bit is read raw from VTYP DNAM (bit 1 is Female) [documented:
xEdit; observed: every generic adult voice has bit 0; the unique, super mutant, robot and
four MaleAdult09–12 voices do not; 61 of 100 lack it]. An actor whose voice type (from the
definition supplying its traits) lacks the bit says no line unless the line has one of
those six functions among its conditions. A line whose INFO names its speaker (ANAM) is
allowed too, since it is said by that actor only [hypothesis]. A VTYP without DNAM
(9 records) is not restricted [hypothesis]. The player's own lines are not gated.

## 10. Checks

Unit test: `fnv_sim::leveled::rolls_none`. Smoke: `vats-no-melee-turret` (a melee mark on
a ceiling turret is refused), `blood-none-ant` (hits on an explosive Nellis ant leave no
spray and no decal), `ghost-tumbleweed`, plus the dialogue, creature talk (`creature-no-talk`,
`creature-names`) and VATS scenarios already there.
