# Spec notes: the items' minor fields (ESM audit low-impact list, "Items")

From the low-impact list of docs/handoff/esm-gap-audit-2026-10-21.md, "Items". Field
names [documented: xEdit wbDefinitionsFNV.pas WEAP, AMMO, ARMO, MISC, KEYM, CSNO;
`schemas/fnv/objects.json`]. Counts are FalloutNV.esm's [observed 2026-10-23, private
survey, not committed]: 261 weapons.

## 1. Weapons (WEAP DNAM)

Built:

- **No Jam After Reload** (flags 2 bit 2; 17 weapons): the first shot after a reload
  cannot jam [documented: the flag's name; hypothesis: "after reload" is the first shot].
  `nvf_play::combat` (`Combat::fresh_reload`).
- **Jam Time** (seconds; 1.4 on 66 weapons, 1.33 on 36, 1.67 on 34, 0 on 59): the length
  of a jam when the weapon has no jam clip, before the 1.5 s default (combat-rules.md
  §17) [hypothesis: the field is the jam's length]. Read with **Reload Time** (1.3 on 63,
  3.0 on 38, 0 on 57), which is not used: reloads last as long as their clips.
- **Rumble** (left and right motor strength 0–1, duration; set on 128 weapons, e.g. 0.8,
  0.8, 0.4 s on 12): a shot rumbles the pad with the left motor as the strong one and the
  right as the weak, for the duration [hypothesis: the Xbox 360 pad's left motor is the
  low-frequency one]; a weapon without them keeps the weight-based pulse (controller.md).
  Rumble Pattern, Wavelength and the "Rumble Alternate" flag (16 weapons) are not used:
  what they shape is not documented.

Not built, and why:

- **Short Burst / Long Burst** (flags 2 bits 9 and 11; 0 and 13 weapons): how many shots a
  burst is, is not documented; VATS mode's bursts come from the fire rate (vats-mode.md).
- **Don't Use 1st / 3rd Person IS Animations** (63 and 44 weapons): which clip plays
  instead is not documented; the aim clips are chosen by weapon type (animation.md).
- **Hide Backpack** (8 weapons): no backpack is drawn apart from the armour that carries
  it, so there is nothing to hide.
- **EAMT** on WEAP is "Enchantment Capacity" in xEdit, not an animation; the actors'
  unarmed animation type (NPC_/CREA EAMT) is built.

## 2. Other items

- **MISC and KEYM RNAM** "Sound - Random/Looping" [documented: xEdit]: built as the
  placed objects' own sounds are (audio.md §15.2): 1 MISC names one, 26 placed; no key.
- **AMMO QNAM** "Abbrev." is read (`AmmoStats.abbrev`) and shown nowhere: no document
  says where the game shows it.
- **ARMO MOD4 and ICO2/MIC2** (female world model and icons; 107 armours have MOD4): the
  ground model stays the male one; whose sex picks the ground model (the player's, the
  last wearer's) is not documented. **BNAM** "Overrides Animation Sounds" (5 of 378 set):
  armour foley is played for every armour that names sounds; what is overridden when the
  flag is clear is not documented.
- **CMNY value**: built since 20f6c6dc (caravan currencies).
- **CSNO**: the chip models are named in the code and only the last ICO2 deck back is
  kept (casino.rs); which deck back a table uses when a casino names several is not
  documented.

## 3. Checks

Unit test `weapon_rumble_from_the_record` (rumble.rs). The jam rule is logged per jam
(`combat: … jammed …`).
