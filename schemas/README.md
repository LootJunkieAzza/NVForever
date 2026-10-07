# Record schemas (schema-as-data)

(`script/` holds the script-function catalogue, not record layouts; see
`script/README.md`.)

`fnv/*.json` describe Fallout: New Vegas record layouts in the `fnv.schema/1` format
(documented at the top of `crates/fnv-esm/src/schema.rs`). They are loaded at runtime,
so a wrong layout is fixed by editing JSON, not code.

- `common.json` — shared types and subrecords whose layout is identical everywhere
- `world.json` — CELL, WRLD, LAND, LTEX, REFR, ACHR, ACRE
- `objects.json` — base objects (STAT, WEAP, ARMO, CONT, …)
- `gameplay.json` — GLOB, SCPT, QUST, DIAL, INFO, NPC_, CREA, FACT, levelled lists, …

**Provenance.** Every entry carries `confidence` and a `note` naming its source. The
current entries are `documented`: layouts were read from the xEdit project's public
definitions (`Core/wbDefinitionsFNV.pas`, `Core/wbDefinitionsCommon.pas`, branch
`dev-4.1.6`, Mozilla Public License 2.0, https://github.com/TES5Edit/TES5Edit) and
re-expressed in this project's own format; no xEdit code is included. Every layout was
then checked against the user's FNV 1.4.0.525 masters with `fnvtool esm coverage`:
zero subrecords decode with leftover bytes, and the only errors are two 2-byte QUST DATA
subrecords on records with old form versions (probably a legacy layout [hypothesis]; see
`docs/spec-notes/esm.md`).

**Rules for editing** (also apply to agents):
- Never change a layout just to make the coverage numbers go up. A layout change needs a
  cited source or an observation that is written up in `docs/spec-notes/`.
- Unknown fields are named `unknown_N` / `unused`, never guessed.
- Subrecords that schema v1 cannot express (unions, position-dependent meaning,
  length-prefixed strings, string arrays) are left out and listed in
  `docs/design/layer1.md`; they show up as `unknown` in coverage.

Check after editing:

```
fnvtool esm coverage <Data>/FalloutNV.esm --schema schemas/fnv --max-rows 50
cargo test -p fnv-esm --release repository_schemas_load
```
