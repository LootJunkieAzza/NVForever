# Spec note: freeing Anders from the cross at Cottonwood Cove (VMS46)

Tags as in `nif.md`: `[documented]` cites a page or file, `[observed]` says what was
looked at, `[hypothesis]` is a reading nothing has confirmed yet, `[design]` marks what
is NVForever's own. Everything below was read from FalloutNV.esm with `fnvtool dialogue
lines --speaker vRRCAnders`, `dialogue topics --topic …`, `script decompile` and `ai show
--actor vRRCAnders` [observed, 2026-11-15, issue #2472]. FormIDs are load-order FormIDs
of FalloutNV.esm alone. No text of the lines is copied here beyond the topic names the
choice shows.

## 1. Who and where

- Anders: NPC_ `vRRCAnders` 0x001413C2, placed as `VRRCAndersREF` 0x001413C3 in the
  WastelandNV cell 0x000846EA at about (55720, -96120, -511), next to the crucifix
  furniture `NVCrucifixFurniture` 0x0013A4BA [observed].
- His lines all belong to quest `VMS46` 0x00140C3B (Jack and Diane's errand at Red Rock
  Canyon). Its quest variable 1 is `andersfreed`: the conditions read it with
  `GetQuestVariable VMS46 1`, and the scripts set it by name (§3) [observed].

## 2. The dialogue

| Topic | Top-level | Line | Conditions (besides `GetIsID vRRCAnders`) | What it leads to |
|---|---|---|---|---|
| GREETING | | 0x00142AA5 | andersfreed == 0 | the topic list |
| AndersCross "Who did this to you?" 0x00142A8C | yes | 0x00142AA3 | andersfreed == 0 | adds AndersDrugs |
| AndersDrugs "You're smuggling drugs?" 0x00142A8B | no | 0x00142AA0 | andersfreed == 0 | the topic list |
| AndersJackandDiane "Are you Anders? Jack and Diane sent me to look for you." 0x00142A89 | no | 0x00142A9A | andersfreed == 0 | choices AndersFree, AndersRot |
| AndersFree "Let's get you down from there." 0x00142A8A | no | 0x00142A9C (Goodbye) | andersfreed == 0 | end script, §3 |
| AndersRot 0x00142A88 | no | 0x00142A98 (Goodbye) | andersfreed == 0 | he stays |

[observed] The top-level column is the DIAL DATA flag (dialogue.md §1: bit 1). Only a
top-level topic is listed without being learnt; any other is offered once a line's Add
Topics (or AddTopic) made it known, or as a line's choice [documented: GECK wiki
Category:Dialogue, "Top-level"; dialogue.md §4 "Choices"].

**The only line whose choices hold AndersFree is 0x00142A9A, of AndersJackandDiane**, and
no other line, script or quest names AndersFree [observed: every INFO's choices, Link
From and Add Topics and every script unit searched for 0x00142A8A]. AndersJackandDiane is
not top-level, is no line's choice, and is added by exactly two lines, both Diane's
(`VRRCDiane` 0x00140E6D) [observed]:

- 0x00146698 (VMS46, topic VMS46VRRCDianeTopic000, the player taking her errand; its
  begin script shows VMS46 objective 10 and the map marker); condition: Anders not dead
  (`GetDeadCount vRRCAnders == 0`). It is a choice of her DianeWork line 0x001466A0
  (VMS46 objective 15 not done, stage not 70).
- 0x00144651 (VMS45, VMS45VRRCDianeTopic001; begin script shows VMS45 objective 32).

So in the data **the player can offer to cut Anders down only after hearing one of those
two lines of Diane's**. Before that, the choices are exactly what issue #2472's log shows:
AndersCross, then AndersDrugs, and Goodbye [observed]. No condition on any of his lines
tests a stage of the Great Khans' quests; the gate is the learnt topic alone.

No other way to free him is in the data: his object script (SCPT 0x0014D3A7) only
restrains him once he sits (`GetSitting == 3` → `SetRestrained 1`, `IgnoreCrime 1`),
cripples his legs on load and counts his death; no activator, furniture or quest script
names him [observed]. Activating a restrained actor is talking to him (as for any
actor) [hypothesis].

## 3. What frees him

The end script of AndersFree's line 0x00142A9C [observed]: `andersfreed` to 1,
`SetRestrained 0`, out of faction 0x00155DBE and into 0x00134B6B, 0x000E78C5,
0x0013FB69, VMS46 objective 10 completed and 15 shown (when 10 is shown), VMS45
objective 32 completed (when shown), and `StartConversation player`: his greeting
0x00142AA6 (andersfreed == 1) thanks the player and its begin script sets `andersfreed`
to 2.

His package list (VRRCAndersRRC, VRRCAndersNotCrucified, VRRCAndersCrucified) chooses by
`andersfreed` [observed]: 0 travel to the cross 0x0013A4BA; 1 travel to an X marker
0x00144684 about 45 units from it; 2 sandbox near 0x0015E8AF (Red Rock Canyon, 3000
units). So he leaves the cross when the package of 1 or 2 replaces the crucified one.
In ours the package is chosen again once the thanks conversation is over, when
`andersfreed` is already 2, so he goes straight from the cross (exit clip
SpecialIdle_NVCrucifixExit) towards Red Rock Canyon [observed: smoke run
anders-leaves-cross, 2026-11-15]; whether the original first stops at the marker of 1 is
not known [hypothesis: the thanks start at once, so it would not].

## 4. NVForever

Nothing here is special-cased: the topic list is the top-level topics plus the learnt
ones (`fnv_game::dialogue`, `Game::build_choices`), and Diane's line adds the topic when
it ends (`Game::finish_line`). Issue #2472's missing choice is the data's rule for a
player who has not taken Diane's errand [observed: `fnvtool dialogue talk` from a new
game: Anders offers AndersCross and Goodbye; after Diane's line 0x00146698 and a save
and load he offers AndersJackandDiane, then AndersFree, whose end script frees him and
starts his thanks 0x00142AA6]. Smoke scenarios `anders-free-after-diane` (headless) and
`anders-leaves-cross` (in the game) hold this.

Whether the original offers some other way before Diane's errand (the owner remembers
freeing him whatever the quest stage) is observer check 175; owner-review item 82.
