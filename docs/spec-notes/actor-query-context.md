# Actor query and merchant context

[documented] `GetPlayerGrabbedRef` returns the reference held by the player's
grab, or no reference. [Source](https://geckwiki.com/index.php/GetPlayerGrabbedRef).
NVForever reads the existing grab state reported by the engine; no grab is created
or changed by the query.

[documented] `actor.GetPackageTarget` returns the actor's current package target;
while the actor is in a conversation it returns its conversation partner.
[Source](https://geckwiki.com/index.php?title=GetPackageTarget).
[hypothesis] Use the existing current-package resolver, linked-reference target
resolver, player conversation and actor-conversation records. Missing targets
and non-actor receivers return no reference. Selection keeps the command's actor
template policy and accepts placed leveled actors through the runtime selector.
[hypothesis] When the actor is both in the player's conversation and in a
conversation with another actor, the player's conversation wins; an actor's
conversation partner wins over its package target, as documented above.
[design] An omitted receiver uses the script's actor. An explicit empty receiver
retains the VM's existing `NoReference` failure and stops the script before host
dispatch, both for statement and expression source calls. The synthetic VM controls
assert that failure and that a following implicit-actor mutation does not run.
No adapter guard or failure-policy change is needed.

[documented] `RemoveMerchantContainer actor` clears the actor's merchant
container assignment
([Source](https://geckwiki.com/index.php?title=RemoveMerchantContainer), the
page named by its `schemas/script/functions.json` entry). `SetMerchantContainer`
assigns a persistent container and
[GetMerchantContainer](https://geckwiki.com/index.php?title=GetMerchantContainer)
queries that assignment.
[hypothesis] Keep an explicit saved clear distinct from a missing override:
clearing an authored assignment must not fall back to that same assignment in
barter, buying or repair. A later successful setter replaces the clear. The
clear is saved with the actor's reference changes.
[observed: Fallout: New Vegas 1.4.0.525 + xNVSE 6.4.9, 2026-11-24, `GSDocMitchellHouse`
(`0x00103DF9`), 10 official base masters, isolated clean-room staging copy (read-only
`PrintToConsole` probe; zero writes to game install or Saves),
`w4d_41_vanilla_player_and_lists` (`tools/capture/nvse/catalogs/wave4d_extenders.json`, PR #3803 / #3802):
`PROBE|w4d_41_vanilla_player_and_lists|xp(0->25)|modAttr(5->7)|lvli(n0=4,l0=5:0,l1=6:1,l65k=7:-1,l70k=8:4464)|pkg(doc=14,pl=0)|rmMerch=ok`;
zero DLL, PDB, or third-party source consulted] Calling `RemoveMerchantContainer` on a placed
actor (Doc Mitchell `0x00104C0F`) with no assigned merchant container completes cleanly as a
safe no-op.
[hypothesis] `RemoveMerchantContainer` returns 0, and a missing, empty or
non-actor argument changes nothing; no cited source or capture establishes
either.
[design] Without a script runtime, barter stock falls back to the reference's
authored container; with one, the runtime's merchant container, including an
explicit clear, is the only source.

[documented] `RemoveFac` is an alias of `RemoveFromFaction`, not another
independent function. [Source](https://geckwiki.com/index.php?title=Complete_List_of_Functions_in_Fallout_New_Vegas_%28OLD_LIST%29).
Reuse the accepted command and its existing argument encoding.

[hypothesis] The first three names are admitted through source names only;
native encoding, original-game edge cases and installed-mod execution remain
separate work. Synthetic tests establish NVForever behavior, not native parity.

[hypothesis] `IsActorDetected` retains its accepted measured-cache behavior and
also admits references carried by LVLN/LVLC after selecting their actual actor
with the runtime template resolver. This changes receiver admission only: player
perception and self-perception remain excluded, and querying does not remeasure
or change the cache.

### Wave 6D-7 Live Observations (2026-11-25)
- **Grab, package target, and merchant container CRUD** [observed 2026-11-25, Fallout: New Vegas 1.4.0.525 (`040020D0`) + `xNVSE 6.4.9` + `JIP LN NVSE 57.30 (5730)` + `ShowOffNVSE Plugin 184` + `JohnnyGuitarNVSE 528`, 10 official base masters, `GSDocMitchellHouse` (`0x00103DF9`), isolated clean-room staging, catalog `tools/capture/nvse/catalogs/wave6d7_actors_avs_aid_slots_observed.json`, probes `w6d7_45_actor_query_context_grab_pkg_target_merchant` and `w6d7p3_45b_remove_merchant_container_ref`]: `GetPlayerGrabbedRef` (`0`), `GetPackageTarget` (`0` for idle actors), vanilla `GetMerchantContainer` (`doc=1, chet=1`), and spawned NPC `SetMerchantContainer` / `GetMerchantContainer` / `rNpc.RemoveMerchantContainer` (`npcMc(0->1->0)`).
