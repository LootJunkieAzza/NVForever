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

## 3. Is Ghost (CREA ACBS bit 28): not applied

124 creatures have bit 28, which xEdit names "Is Ghost". They include the wild dogs
(`VCrTier1DogWildMedNR` and others), giant rats and big horners that the player kills in
normal play. So in the original the bit cannot mean what SetGhost means ("cannot be harmed
… Projectiles pass through them" [documented: GECK wiki SetGhost]) [observed]. Applying it
would make those creatures unkillable. Not applied; SetGhost works as before. What the bit
does in the original is an open question.

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
a ceiling turret is refused), `blood-none-tumbleweed` (hits on a tumbleweed leave no
spray and no decal), plus the dialogue, creature talk (`creature-no-talk`,
`creature-names`) and VATS scenarios already there.
