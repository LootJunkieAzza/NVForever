# TempCloneForm omitted form argument

[documented] [TempCloneForm](https://geckwiki.com/index.php/TempCloneForm)
allows the form argument to be omitted when a calling reference supplies its base.
The same public documentation describes temporary clones as session-only bases,
not saved forms. This patch does not change clone lifetime or save behavior.

[observed: baseline 19c0f08a source] The dispatcher returns a null form when its
first argument is absent, before consulting the calling reference. The existing
base resolver already distinguishes a placed reference from its base.

[observed: 2026-11-23, `GSDocMitchellHouse` (`0x00103DF9`), fresh game, Fallout: New
Vegas 1.4.0.525 + xNVSE 6.4.9 with the 10 official base masters, clean-room black-box
script probe `core_11_tempcloneform_calling_ref` (reproducible via `#3678` / PR `#3679`,
`tools/capture/nvse/catalogs/wave4a_extenders.json`: `ref rRef = Player.PlaceAtMe rBase 1;
ref rCl = rRef.TempCloneForm`); no DLL or source consulted] Calling `rRef.TempCloneForm` on
a placed reference with no form argument returns a valid dynamic clone (`IsFormValid` = `1`,
`FF00131B`, with `TempCloneForm rNull` returning `00000000`:
`PROBE|core_11_tempcloneform_calling_ref|1,FF00131B,00000000`).

[hypothesis: implementation design] Only an empty argument list uses that base
resolver. An explicit form retains precedence; an explicit null and an absent
caller retain their previous no-clone result (not settled by
`core_11_tempcloneform_calling_ref`). Keep allocation and alias storage
unchanged. Four independently authored fixtures use real script-created activator
references and check the returned clone identity and original base, without
depending on the separate model-metadata correction.

[observed: qualification boundary] An earlier private source with related metadata
changes passed four caller controls and twelve related tests. That is historical
evidence only. This current-main port has not been compiled, listed, executed or
linted. Its four fixtures are registered in the existing integration binary.
Scene placement of clones, renderer admission, IDLE animation, clone-of-clone
semantics, full B42 behavior and native parity remain separate work.
