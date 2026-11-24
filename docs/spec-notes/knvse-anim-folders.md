# kNVSE's AnimGroupOverride folders (#1461)

kNVSE replaces actors' and weapons' animations with files a mod puts in a folder, with no
plugin and no script. NVForever reads the folders in `crates/fnv-game/src/nvse_anim_folders.rs`
and plays what they give through the override layer of the kNVSE functions
(docs/design/knvse-anim.md §2).

Sources: the GECK wiki export's CopyAnimationsToForm page, the one page that mentions the
folder [documented: "Copies the kNVSE AnimGroupOverride animations, including conditions
and other attributes"]; the kNVSE Animation Plugin Nexus Mods page (mod 71336) for
`_1stPerson`, `_male`, and `Mod1`–`Mod3` weapon-mod subfolder conventions [documented];
GECK wiki `GetEquippedWeaponModFlags` for weapon-mod bitmask bits `1`, `2`, and `4`
[documented]; and the file trees of the 25 mods installed in the owner's two Mod
Organizer folders that have one (8,837 files, 299 JSON files, 778 entries; survey of
2026-11-03) [observed]. No kNVSE DLL or source code was consulted. What the files show is
the layout mods use; details not explicitly stated by public documentation are tagged
`[hypothesis]` or `[design]`.

## 1. Layout [observed]

- `meshes\AnimGroupOverride\` holds JSON files directly (`RNC_AMAKFlamethrower.json`) and
  one folder per set of animations (`RNC_AMAKFlamethrower\`). Folder names are the mods'
  own; nothing in the name says which form they are for.
- In a set's folder, `_male\` holds the third-person files and `_1stPerson\` the
  first-person ones (3,974 and 4,441 files; any case: `_Male`, `_1stperson`). No `_female`
  folder was seen. Under them the files sit flat or in subfolders mirroring the game's
  (`_male\Locomotion\mtidle.kf`, `_male\human\2haaim.kf`) or with any name
  (`_male\New folder\2haEquip.kf`).
- A set's folder can hold further sets, named by their path (`Rock_ServiceRifle\JamLow\`
  with its own `_1stPerson\`); entries name them as `"folder": "Rock_ServiceRifle\\JamLow"`
  (14 entries).
- Files are `.kf` (8,517), with `.bak` copies, a `chances.txt` beside some jam files and
  `_male.mohidden` folders (Mod Organizer's hidden files). File names need not be the
  group's (`1hpReloadL v2.14.kf`, `2haJamF_a.kf`).
- `_1stPerson\Mod1\` … `Mod3\` folders (and `_male\Mod1\` … `Mod3\`) sit beside the
  plain files of a weapon (`Ani_DesertEagle\_1stPerson\Mod1\1hpReloadL v2.14.kf`)
  [documented: kNVSE Animation Plugin Nexus page (mod 71336), Mod1/Mod2/Mod3 folders
  apply only when the corresponding weapon mod slot is installed; hypothesis: combined
  bitmask subfolders `Mod12`, `Mod13`, `Mod23`, `Mod123` apply when all named slots are
  installed].
- Some override folders place `.kf` files directly inside the folder or in named
  non-view subfolders (without a `_male\` or `_1stPerson\` directory prefix).

## 2. The JSON files [observed]

A file is a list of entries, or one entry; 59 files are empty. An entry's keys:

| Key | Seen | Value |
|---|---|---|
| `folder` | 778 | the set's folder, under `AnimGroupOverride` |
| `mod` | 710 | a plugin's file name (`FalloutNV.esm`, `mil.esp`, `Fallout3.esm` for TTW) |
| `form` | 708 | an ID in that plugin, hex without `0x`: one (`"7E56B"`) or a list (254) |
| `condition` | 422 | a script condition (below) |
| `priority` | 84 | a whole number: 1, 2, 10 |
| `pollCondition` | 12 | `true` |
| `comment` | 10 | text |

- The forms are weapons (`FalloutNV.esm` `"432C"` with the folders `Rock_FatMan…`, and
  the mods' own guns) and the player (`"form": "14"`, PlayerREF's ID); 68 entries have no `mod`/`form` and are for every actor whose condition
  holds (`GetCreatureType == -1 && GetIsSex Female …`). [hypothesis: base actors, races and
  creatures work as forms too; none was seen.]
- Conditions are script expressions as an `if` takes them: `GetCurrentAmmoRounds == 0`,
  `GetPCIsSex Female == 1 && GetWeaponHealthPerc >= 85`, `(Goo1.AuxVarGetFlt "*_BnBEqSocket")
  == 1`, `GetINIFloat "Perk:b357MagnumrevolverCowboy" "RevolverAimAnimations.ini"`,
  `IsIdlePlayingEx ProtectronTurnedOff == 1`. `this` names the actor (`this != Player &&
  IsSwimming`), and functions without a reference run on it.
- A form-wide set comes first and its special cases after, with a condition and often a
  priority (the Fat Man: the plain set, then `_empty` on `GetCurrentAmmoRounds == 0`, then
  jam sets by `GetWeaponHealthPerc`). Files are named to sort first or last
  (`1Another_Millenia_base.json`, `zzz_heavy_fatman.json`).
- 20 entries name a folder that is not there.

## 3. What NVForever does

- **Reading:** once, when the game's Data folder is known: the JSON files directly under
  `meshes\AnimGroupOverride` in every read root (NVForever's layer first; Mod Organizer's
  virtual Data is one root), read in name order without regard to case. An entry is kept
  when its plugin is loaded and its folder has animation files; its `form` IDs become the
  plugin's load-order IDs. A file that is empty is skipped; one that is not JSON, an entry
  without a folder, one naming a plugin but no form [hypothesis: a slip; only entries
  with neither are for every actor], and one whose folder is missing or empty are logged
  and skipped
  (the broken-files rule). The start-up line `AnimGroupOverride: N entries from M files`
  says what was read. Only loose files are read [hypothesis: mods ship them loose; none
  was seen in an archive].
- **A set's files:** every `.kf` under its `_male` (third person) and `_1stPerson` (first
  person) folders at any depth, plus direct `.kf` files placed in the set's folder or in
  named non-view subfolders when no `_male`/`_1stPerson` prefix is present (applied to
  both views, ordered before explicit `_male`/`_1stPerson` files so explicit view files
  override direct files of the same clip name) [hypothesis; design]. Nested child sets
  (subfolders containing `_male`/`_1stPerson` below the top level, or subfolders named as
  their own `folder` by another JSON entry) and Mod Organizer `*.mohidden` directories
  are excluded so child sets never leak into a parent set [design].
- **Weapon-mod subfolders (`Mod1`–`Mod3`, combined `Mod12`/`Mod13`/`Mod23`/`Mod123`):**
  `.kf` files under a `Mod1`, `Mod2`, or `Mod3` directory (any ASCII case) are stored in
  weapon-mod-gated buckets keyed by the 3-bit weapon-mod bitmask (`1` = Mod 1, `2` = Mod 2,
  `4` = Mod 3) [documented: kNVSE Animation Plugin Nexus page (mod 71336); GECK wiki
  `GetEquippedWeaponModFlags`]. Combined bitmask directory names (`Mod12`, `Mod13`,
  `Mod23`, `Mod123`) and nested `Mod*` directories combine their bits [hypothesis]. A
  folder whose only `.kf` files live inside `Mod*` subfolders is kept rather than dropped
  as empty. During evaluation, matching buckets (`(equipped_mods & mask) == mask`) are
  appended after base files ordered by `(mask.count_ones(), mask)` so more specific mod
  combinations override single-mod and base animations [design]. Unmodded weapons
  (`equipped_mods == 0`) fall back to the base files only.
- **`chances.txt` weighted random selection:** when a directory containing `.kf` files
  also contains a `chances.txt`, NVForever parses `<file> = <weight>`,
  `<file>: <weight>%`, or `<file> <weight>` lines (ignoring blank lines, `;`/`#`/`//`
  comments, malformed lines, negative weights, and non-finite weights fail-closed)
  [hypothesis; design]. Matched `.kf` files (matched by lowercase filename or bare stem)
  with weight `0` are excluded; positive-weighted matched files form weighted pools
  (split by underscore-delimited base stem when multiple distinct multi-file variant
  families exist in the same directory, otherwise one pool for the directory) and select
  one path per pool deterministically via `AnimFolders`'s own dedicated `ChaCha8Rng`
  (separate from `Game::rng`, so animation packs never perturb unrelated gameplay rolls)
  [design]. Each actor's picks are cached per pool and stay stable across periodic polls
  until the actor's equipped weapon, weapon-mod mask, or matching entry set changes
  [design]. Unmatched `.kf` files (and directories whose `chances.txt` contains only
  author notes such as `Short: 20%` with no matching `.kf` filenames) remain
  unconditional and do not draw from either RNG [design].
- **Which hold:** four times a second for each actor the engine draws, every frame for
  the player (a last-shot entry, `GetCurrentAmmoRounds == 1`, must hold at its shot): the
  entries for the actor's reference, its base or its equipped weapon, and those for every
  actor, whose condition holds on the actor (EvaluateString's compiler; a condition that
  fails to compile or run counts as false and is logged once). [decision: the original
  checks a condition when the animation starts, or continually with `pollCondition`
  (hypothesis); a quarter second covers both closely enough, so `pollCondition` is read
  but changes nothing.]
- **Which wins:** the holding entries in order of priority (none is 0), then of reading;
  within each entry, base files precede matching `Mod*` buckets in specificity order;
  their files are given in that order, and of two files of the same group the later wins
  (as the engine applies SetActorAnimationPath's paths) [hypothesis: the higher priority
  wins, and at the same priority the later entry]. SetActorAnimationPath's paths come
  after the folders' [hypothesis: a script's call wins over what a mod installed]
  (`Game::anim_overrides`).
- **CopyAnimationsToForm** adds the target form to every entry of the source, conditions
  and priorities with them [documented].
- **The engine** plays the third-person files whose name is a standing locomotion group
  (`mtidle.kf`, `mtforward.kf` …) on drawn actors, as it plays SetActorAnimationPath's
  (`engine/nvforever/src/anim_paths.rs`). A file's group is read from its name, not its
  sequence [hypothesis, as knvse-anim.md §2; the mods' `1hpReloadL v2.14.kf` shows names
  can differ].
- **The player's two rigs** (`engine/nvforever/src/player_body.rs`, `apply_anim_overrides`;
  first person from `_1stPerson`, third person from `_male`) take every clip choice from the files too:
  locomotion, equip and unequip, aim and aim down the sights, attacks, reloads, jams. A
  file's name is matched to the rig's own clip names (`1hpreloadl.kf`) by its stem: lower
  case, without `.kf`, up to its first space (`1hpReloadL v1.6.kf`, `1hpAttackRight
  last.kf`) [observed: Another Millenia's and Hit's Anims' files; hypothesis: the words
  after the space are the author's notes]. A file for any of a choice's names beats every
  file of the rig's own (a mod's plain `1hpreloadl` plays while sneaking even where the
  game has `sneak1hpreloadl`). Within one folder a plain name beats a spaced one
  (`1hpEquip.kf` over `1hpEquip Combat.kf`) [hypothesis]; across entries the later wins.
  A reload with rounds still in the magazine plays the `…_Partial` file first
  [hypothesis: from the file names only; 173 such files]. The log says `plays override
  <path>` for each choice so taken.
- **Files played in turn** (`_order_N`): "If you want variants to play in order, have the
  file names end with _order_[NUMBER].kf" [documented: kNVSE's Nexus page, Tip 2 of the
  folder guide; the same tip says other `_<letters>` variants are picked at random]. The
  files of a stem ending `_order_N` are one set, ordered by N; a file of the same stem
  without the ending, in the same folder, comes first [hypothesis: Another Millenia's
  `Sneak1hpAttackRightDOWN.kf` beside `…DOWN_order_2.kf`, with no `_order_1`]. Each
  attack of the player plays the set's next file, the first again after the last; the
  turn is the player's attack count, one for every set of both rigs, so the hip and
  down-the-sights sets keep in step [hypothesis: whether kNVSE keeps a turn per group,
  per set or per actor is not known]. A later entry holding the stem replaces the whole
  set [hypothesis]: the akimbo sets' last-shot folders (`…RightLast`,
  `GetCurrentAmmoRounds == 2`, only `_order_1`; `…LeftLast`, `== 1`, only `_order_2`)
  then play their one file at those shots. The up and down forms of an ordered file put
  the aim's ending before `_order_N` (`1hpAttackRightUP_order_1.kf`) [observed: the
  akimbo `_male` files]. The akimbo files move the weapon model's `ProjectileNode` and
  `ShellCasingNode` to the gun that fires (`order_1` the right one, with `##nmBar`,
  `order_2` the left, with `##nmBar1`) [observed: the node names in the files], and shots
  leave from that node, so each shot leaves the gun its clip fires.
- **`noBlend`** (a kNVSE text key): NVForever starts a clip that has it at once instead
  of fading it in [hypothesis: from the key's name only; kNVSE's list of its text keys
  is on the GECK wiki, which could not be read]. The key has a time: 0.15 s in
  `…_order_1.kf`, 0.05 s in `…_order_2.kf`, so its meaning may be narrower [observed].
  The clips' `Hit` key is at 0.017 s, one frame in; their `ProjectileNode` sits on the
  firing gun from 0 to 0.1 s, then moves to the other gun for the next shot [observed:
  the tracks sampled with NVForever's clip reader]. A 0.03 s fade-in left part of the
  node at its rest place (the right gun) when the shot left.
- **The shot's frame**: a shot that waits for a `Hit` key never leaves in the frame of its
  pull, even when that frame is longer than the wait (`fnv_combat::attack`): the clip
  starts at the pull and the shot is drawn from its pose a frame later [design]. Before,
  at about 60 frames a second, some shots left in the pull's frame from the last clip's
  fading pose: 9 of 15 left-gun shots left part way to the right gun [observed:
  NVForever run of 2026-11-10].
- **NPCs and co-op puppets holding a weapon** (`engine/nvforever/src/anim_overrides.rs`)
  take the third-person (`_male`) files of the entries that name the weapon and have no
  condition, sorted as the player's are (priority, then load order) [hypothesis: a guest's
  puppet has no actor on the guest's side, so conditions cannot be tested for it; the
  conditioned last-shot folders are left out]. These files replace, by stem as above, the
  stance's aim and equip clips (which place the weapon model's parts once when the weapon
  is drawn: the `Akimbo` node, the second gun of Another Millenia's dual pistols, goes from
  its rest place 2.5 units below the right gun into the left hand [observed:
  `DualGlock19\Glock19.nif` and the mod's `1hpAim.kf`]) and the attack, reload and aim
  up/down/sights clips; `_order_N` sets go round with that actor's own attacks. The log
  says `anim overrides: N form(s) with third-person files` once, and `from override
  files: [...]` for each stance so made.
- `respectEndKey`, also on those clips (0.25 s and 0.35 s), is not read.

## 4. Not built (follow-ups)

- NPCs' and puppets' weapon parts are posed once from the aim or equip clip's end, not
  played through an attack or reload, and conditioned entries do not apply to them.
- `_hurt` files (143 in the Millenia and Hit packs) are left out [hypothesis: played
  while the actor is hurt; kNVSE's page names a `Hurt` folder for crippled legs];
  `_A`/`_B`/`_1`/`_2` variants of an attack are not picked at random, as the page says
  kNVSE does (the plain file plays; a WEAP attack animation naming the variant plays it).
- NPCs and co-op puppets take only an entry's base files: its `Mod1`–`Mod3` buckets and
  `chances.txt` picks, resolved game-side for an actor (`Game::anim_overrides` /
  `Game::anim_overrides_now`), are not given to them (`Game::weapon_anim_files`).
- The group of a file from its sequence name instead of its file name, and engine-side
  variant-stem mapping when a selected file name carries a variant suffix (`2haJamF_a.kf`)
  for a base group (`2haJamF.kf`).
- An observer check in the original: which of two holding entries wins at equal and at
  different priorities, whether a script's SetActorAnimationPath beats a folder, and exact
  tie-breaking across simultaneous `Mod1`/`Mod2`/`Mod3` subfolders.
