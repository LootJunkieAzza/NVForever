# Scripted weapon use: UseWeapon and the Use Weapon package

What the documentation and the masters say about making an actor fire a weapon by script
or package. The design built on it is docs/design/use-weapon.md.

## 1. The UseWeapon function

[documented: GECK wiki UseWeapon (Function), the owner's local export]

- Called on an actor reference; opcode 0x119C [observed: the function catalogue].
- Nine parameters, the last five optional: the weapon (a base form), where to stand (a
  reference), the target (a reference), how many times to use the weapon, whether to
  crouch when reloading (default on), whether to hold fire when blocked (default on),
  whether to "automagically hit the target" ("for scripted forced kills", default off),
  whether to "prevent all damage" ("for scripted protracted fake combat", default off),
  and where the target needs to stand ("default/NULL means anywhere").
- The catalogue types the last parameter as an int; the page's example passes a marker
  reference there: `BuddyRef.UseWeapon Pistol12 BuddyRef HollyRef 5 0 0 0 0 XMarkerRef047`,
  explained as Buddy drawing his pistol ("or grab one if one was nearby") and firing at
  Holly five times from his own place, "but only while she is near XMarkerRef047".
- The page links the Use Weapon package and FireWeapon.

## 2. Calls in the shipped scripts

[observed: `fnvtool script decompile`, FalloutNV.esm; the catalogue counts 13 calls over
the masters]

| Unit | Call | Context |
|---|---|---|
| SCPT 0x0011D217 | `useweapon <weapon> <stand ref> <target> 5 1 0 1` | a GameMode block that also calls `startcombat <target>` just before: a scripted kill (always hit on) |
| SCPT 0x0011C54C | `useweapon <weapon> <stand ref> <target> 5 1 0 1` | the same pattern with the two actors swapped |
| SCPT 0x000FED4F … 0x000FED52 (4 trigger scripts) | `<actor>.useweapon <weapon> <stand ref> <target> 4 1 1 0 0` | OnTriggerEnter of the player: four shots at one of four targets, then `<actor>.startcombat player` |
| SCPT 0x000E07CE | `<actor>.useweapon <weapon> <actor> rtarget 1 0 0 1 0` | a timed sequence: the actor stands on its own reference, one shot (always hit) at the next living target; three seconds later the script calls `<actor>.resetai` |
| INFO 0x0011C543 (end) | `<actor>.useweapon <weapon> <actor> <target> 3 0 1` | a dialogue result after `startcombat` calls; the actor stands where it is |

So: where to stand is often the actor itself; counts are 1 to 5; UseWeapon is called right
next to StartCombat, and ResetAI is used to end it.

## 3. The Use Weapon package

- PACK PKW3 "Use Weapon Data" [documented: xEdit PACK PKW3]: flags Always Hit (bit 0), Do
  No Damage (bit 8), Crouch To Reload (bit 16), Hold Fire When Blocked (bit 24); fire
  rate (Auto Fire, Volley Fire); fire count (Number of Bursts, Repeat Fire); number of
  bursts; shots per volley min and max; pause between volleys min and max (seconds).
  The names are all there is: what each does in play is not documented.
- Package type 16 in PKDT; GetCurrentAIPackage code 33 (docs/spec-notes/packages.md §1).
- Package idles never play for Use Weapon [documented: GECK wiki Category:Packages;
  docs/spec-notes/actor-behaviour.md].
- In the masters: 173 Use Weapon packages; 9 have a location 2; target 2 is always a
  specific reference; target 1 is an object type in 52 ("Weapons: None" 32, "None" 10,
  melee 7, …) [observed: `fnvtool ai survey`, packages.md §2]. The type's own page (Use
  Weapon Package) is not in the owner's export; the offline GECK wiki copy has it.
- "Continue During Combat: Actor will continue to fire and stand at the designated
  position, even during Combat. Great for setting up immobile enemies." [documented: GECK
  wiki page "Use Weapon Package", section Flags, revision 35491 of 2024-08-04,
  https://geckwiki.com/index.php?oldid=35491]. The page does not say at whom it fires
  once in combat; the general flag page says a Travel package with the flag fires "at
  their target" on the way [documented: GECK wiki page "Combat Package Flags", revision
  34683 of 2024-02-17, https://geckwiki.com/index.php?oldid=34683], and its
  per-type chart has no Use Weapon row. We keep the package's target 2 [design;
  docs/design/use-weapon.md §3, #2120]. 43 of FalloutNV.esm's 82 Use Weapon packages set
  the flag (sniper posts, scripted kills, the Ultra-Luxe kitchen's flamers, the Hoover Dam
  Securitrons); 38 of them carry conditions [observed: `fnvtool ai packages` and `ai
  show`, 2026-11-11].

- A Do No Damage hit (a shot or a blow) runs the target's OnHit and OnHitWith blocks with
  the shooter as attacker and deals no damage of its own (`Game::harmless_hit`, #2433)
  [hypothesis from the data, observer check 134: G.I. Blues' staged ambush. Orris's
  VFSOrrisShootThug01-03Package are Always Hit and Do No Damage, one volley of 1 to 6
  shots, Once Per Day, each conditioned on his `shootingthugsstate` and the thug not
  unconscious. Each thug's OnHit block for Orris (SCPT 0x00118A77) knocks it out
  (SetUnconscious 1, PushActorAway) and moves the state on, the third adds his
  after-fight Dialogue package. Without OnHit the first package ended after its shot
  and none followed: the owner's soft lock].

## 4. SetRestrained

Restrained actors still try Use Weapon packages and UseWeapon and their animations but
never attack [documented: catalogue notes on SetRestrained].

## 5. Open questions

- What "blocked" means for Hold Fire When Blocked (someone in the way, a wall, both).
- Whether the shots of the function form one volley or follow the weapon's own rate.
- What Auto Fire against Volley Fire changes, and whether a burst is a volley.
- How far from the "target should be here" reference the target may be.
- Whether a use from a script holds against the actor's combat state, or the combat AI
  takes over (the shipped scripts call both together).
- Whether the actor "grabs" a weapon it does not carry, and from where.
- How a melee weapon is used: whether the actor walks up to its target from where it is
  to stand, whether a blow counts as one of `numberOfShots`, and what Always Hit and Do No
  Damage do to a blow (ours: docs/design/use-weapon.md §3.1, all [hypothesis]). Which
  shipped calls name a melee weapon was not surveyed.

## 6. Non-actor reference targets (#2652, #2715 shooter half)

- [documented: GECK wiki UseWeapon (Function), revision 30345, offline snapshot
  2026-10-28] The target parameter has type `ref`; the page does not restrict it to
  actors or specify an aim point for objects.
- [documented: #2652 issue report and its final #2715 comment, supplied record facts]
  LonesomeRoad.esm PACK 0x008923's end script gives ED-E one Always Hit / Do No Damage
  shot at ACTI NVDLC04Silo01MainframeREF (REFR 0x0075E5). Its script SCPT 0x008952
  waits in `OnHit NVDLC04EDEREF` and sets NVDLC04MQ01a 55. The commissary activators
  use the same Use Weapon / OnHit pattern. Other DLC scripts target XMarkers without
  waiting for a hit. FalloutNV.esm PACK 0x0012943D gives the NCR barkers Always Hit,
  Do No Damage, Repeat Fire at VFSPacerBusShelterREF (0x0011BD4E, STAT); BusShelter01
  has OBND z -1..227.
- [hypothesis: inferred from those supplied scripts, original not captured in this run]
  The original fires at objects and markers; finite marker shots release the actor,
  and harmless object hits pass the shooter to OnHit. Observer check 207 requests the
  original behavior and aim placement.
- [design, #2652; `engine/nvforever/src/use_weapon/targets.rs`] NVForever keeps drawn
  actor aim unchanged and resolves other present references from moved/placed positions.
  An actor target that is not drawn gets no aim point (#2782 review): the use waits and
  the ordinary attacks keep the actor, as before #2652.
  It adds the base OBND centre when available, otherwise fires toward the position
  itself [hypothesis: aim placement pending observer check 207]. Collision and hit
  dispatch stay on the existing object path; the shot count, rather than collision,
  finishes a finite use. The fight handover uses the same target availability rule.
- [documented: #2652 w4 brief, supplied fnvtool world list / world refs facts]
  The barkers' fight markers (0x001293F0 / 0x001293F5) and Pacer's cower marker
  (0x001293EC) belong to FreesideNorthWorld (0x0012D94D), but the shelter target
  (0x0011BD4E) belongs to FreesideWorld (0x0010BEEA). Both worldspaces name
  WastelandNV (0x000DA726) as parent (parent flags 4) and share coordinates:
  the shelter at (-3961,5246,1033) is near Pacer's marker at (-3863,5234,1049).
- [observed: main session's c2 run and trace supplied in #2652 w4 brief] The King's
  INFO end script runs, sets ncrfight to 1, gives the barkers rifles, moves them to
  their fight markers and calls EvaluatePackage, but the smoke records zero shots.
  [design: `fnv-ai/src/carry.rs::weapon_step`] Exact-space filtering prevents the
  barkers turning toward that sibling-worldspace target.
- [hypothesis, #2652; observer check 207] A Use Weapon / UseWeapon non-actor placed
  target in another worldspace with the same nonempty immediate parent is aimed at
  by its coordinates plus OBND centre, as if in the shooter's worldspace. Different
  parents, absent parents, and interiors do not qualify. Actor targets keep their
  existing behavior. [design: `fnv-ai/src/goal.rs::weapon_target_place`]
  Facing, resident use-owned combat aim, object aim and fight handover share this
  rule; ordinary reference positions, distances and combat targets do not change.
