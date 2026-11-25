# Temporary cloned projectile record access

[documented] [TempCloneForm](https://geckwiki.com/index.php/TempCloneForm) returns a
temporary base-form clone; its form is not persisted in a save. The projectile
setter argument/field contract is described in [JIP batch 7](jip-records-7.md),
including numeric speed trait 2, light and muzzle-flash path. Public references:
[SetProjectileTraitNumeric](https://geckwiki.com/index.php/SetProjectileTraitNumeric),
[SetProjectileLight](https://geckwiki.com/index.php?title=SetProjectileLight).

[observed: baseline 0b16bea3 portable source] `Game::clones` retains the original
identity. `nvse::record_value` already reads clone overrides and inherited records,
but the VM host's separate `record_field` path does not use that helper. Three PROJ
setters check only `GameData::sig(clone)`, where no loaded record exists. This can
silently return default values or skip writes despite a valid temporary clone.

[implementation] Delegate VM record reads to the existing clone-aware helper.
Validate the inherited signature for only SetProjectileTraitNumeric,
SetProjectileLight and SetProjectileMuzzleFlash, while retaining the exact clone ID
for writes. The owned tests exercise inherited speed, zero-valued overrides,
clone-local light set/clear and muzzle path, invalid signature/trait controls,
original independence and a fresh Game. Other typed setter guards stay as before.

[observed: earlier owned portable qualification, separate source] The public three-
test baseline failed; changing only the setter guards left two passes and one
inherited-getter failure. The combined isolated change passed those tests and lint.
That aggregate source is historical evidence, not current 0b16 execution proof.

[observed: Fallout: New Vegas 1.4.0.525 + xNVSE 6.4.9 + JIP LN 57.30, 2026-11-24,
GSDocMitchellHouse (`0x00103DF9`), 10 official base masters, isolated staging copy,
read-only, committed probe `w4g_02_tempcloneform_world_proj_csty` in
`tools/capture/nvse/catalogs/wave4g_f2_extenders.json` (companion catalog PR #3803);
no DLL or source consulted] `TempCloneForm` performs a copy-at-creation snapshot on
`PROJ` records rather than live inheritance through `unclone()`: pre-clone mutations on
the source `PROJ` carry onto the clone, subsequent mutations on the source `PROJ` do
not bleed into already-created clones, and a clone-of-clone remains isolated from later
parent-clone mutations. Verbatim printed probe output:
`proj(0002CD5F:t1=3.00,t2=27520.0,t4=0.0|cl1=1234.5/0.75|afterSrc=5678.5:cl2=1234.5/0.75|cl2sub=1234.5)`.
NVForever snapshots all active `PROJ` overrides (`Game::snapshot_projectile_traits`) at
`TempCloneForm` creation time and reads untouched fields directly from the authored
record rather than falling back to the original's post-clone `field_overrides`.
Save/reload, scene placement, rendering, projectile physics and complete consumers
remain separate acceptance.
