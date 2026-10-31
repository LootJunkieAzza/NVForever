# NX NVSE ("NVSE Extender", #1017)

Code: `crates/fnv-game/src/nvse_nx.rs`. Sources: the GECK wiki export, Category:Functions
(NX) and each function's page [documented]. No probe: NX is not installed on the
owner's game (Data\NVSE\Plugins holds JIP, JohnnyGuitar, ShowOff, kNVSE, MCM and
ZeGaryHax) [observed: 2026-10-31].

## 1. NX variables

- Numbers (EVFl), forms (EVFo) and strings (EVSt) kept on a reference under a string
  key; any number per reference; keys are unique within a kind and may be shared across
  kinds; keys are case-insensitive; a key not set reads 0 (a string: an empty one, "NULL")
  [documented: Category:Functions (NX); NX_GetEVFl, NX_GetEVFo, NX_GetEVSt].
- They "are persistent across game sessions": NVForever saves them with the game
  (`GameSnapshot::nx`) [documented].
- NX_SetEVFl / Fo / St `key value` set; NX_ClrEVFl / Fo / St `key [match]` clear, every
  key that begins with `key` when `match` is 2 [documented]; another `match` clears the key
  alone [hypothesis]. Both return nothing.
- NX_GetEVFlAr / FoAr / StAr `prefix`: a string map of the kind's variables whose key
  begins with `prefix`, keyed as the variables are ("Keys in the returned array match the
  NX ones"), copies [documented]; the key's case is the one it was last set with
  [hypothesis]. The prefix is compared without regard to case [hypothesis: as the keys].
- Called on no reference, the setters do nothing and the getters read 0 [hypothesis].

## 2. The other functions

| Function | NVForever |
|---|---|
| NX_GetRandom `min max` | an integer from min to max, both included [documented], from the game's seeded generator; max at or below min gives min [hypothesis] |
| NX_IsInList `list item` | 1 the item is in the list, 2 its base is, 3 it is the base of a reference in the list, 4 its base is the base of a reference in the list, 0 otherwise, in that order [documented] |
| NX_GetConversationPartner | the actor in conversation with the player, whoever began it [documented]; 0 outside one |
| NX_GetSkeletonPath | the folder of the actor's base's skeleton model (MODL, as GetSkeletonModel reads it): `_male` for `Characters\_Male\Skeleton.NIF` [documented: "_male" for a human, "NVGecko" for geckos] |
| NX_IsUsingSkeleton `name [actor]` | 1 when a folder of the skeleton model's path (under meshes) is `name`, without regard to case [documented]; any folder of the path counts [hypothesis] |

Not built: NX_GetQVEVFL, a condition function whose use the page leaves to an outside
tutorial [documented: NX_GetQVEVFL].

## 3. Checks

- `cargo test --release -p fnv-game --test synthetic nx_variables` (case, prefix map and
  clear, kinds apart, the save, NX_GetRandom's range).
