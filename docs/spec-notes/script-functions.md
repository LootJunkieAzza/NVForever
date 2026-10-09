# Spec note: script functions (the command set behind compiled scripts)

Clean-room reference for the session that implements the script runtime (the "back end"
that compiled ESM/ESP scripts call into). It answers three questions:

1. **Which functions exist**, with what parameters and calling reference? → the catalogue
   `schemas/script/functions.json` (`fnv.script.functions/1`) and its readable form
   [`script-functions-catalogue.md`](script-functions-catalogue.md) (base game only).
2. **How does compiled bytecode name them?** → opcodes, section 3 and section 4.
3. **What should they do?** → our own paraphrased behaviour notes per function
   (`schemas/script/notes.json`, merged into the catalogue), each linking to its wiki page.

Tags: **[documented]** (source named), **[observed]** (checked against the owner's FNV
1.4.0.525 masters: FalloutNV.esm, the 4 story DLCs, GunRunnersArsenal, the 4 pre-order
packs), **[hypothesis]**.

## 1. Sources and how they were used

| Source | What we take | Licence handling |
|---|---|---|
| GECK wiki, `Category:Functions (All)` (geckwiki.com), private `Special:Export` dump at `C:\fnv-private\geckwiki\functions.xml` (3,486 pages, export of 2026-10-07) | Only the **structured** fields of each page's `{{Function}}` template: name, alias, origin code, calling reference, return type, argument names/types/optionality, categories | Page prose is **not** copied. Summaries/notes/bugs in `notes.json` are our own paraphrases (checked: no 8-word run shared with the source page). The dump stays private like the goldens. |
| Owner's masters (read-only) | Compiled script data (SCDA) lined up with the shipped source text (SCTX); condition (CTDA) function indices | Only opcodes, counts and names the wiki also documents are written; no script text or game content. |
| xEdit `Core/wbDefinitionsFNV.pas` (TES5Edit/TES5Edit, branch `dev-4.1.6`, MPL-2.0) | Condition-function table: an index and parameter types per function name; SCHR/CTDA layouts | Re-expressed as data; no xEdit code. Already a Layer 1 source. |

geckwiki.com sits behind a Cloudflare bot check, so scripted fetching does not work.
To refresh the dump, the owner opens `Special:Export` in a browser, adds the category
`Functions (All)`, ticks "current revision only" and saves the XML file. A later session
can read individual pages from that file; it does not need the website.

Rebuild everything (about 5 s):

```
python tools/capture/script_functions.py --wiki C:/fnv-private/geckwiki/functions.xml ^
  --data "<FNV>/Data" --xedit <path>/wbDefinitionsFNV.pas --notes schemas/script/notes.json
```

## 2. Scope: base game vs script extenders

The wiki's `origin` field says where a function comes from [documented]:

| Origin code | Meaning | In catalogue |
|---|---|---|
| `GECK1` | base game, present since the Fallout 3 GECK | 562 |
| `VEGAS1` | base game, added in New Vegas | 49 |
| `CONSOLENV` | base game console command | 17 |
| `observed-only` | (ours) called by shipped bytecode but no wiki page | 8 |
| everything else (`NVSE`, `FOSE1`, `JIP`, `LU`, `JohnnyGuitar`, `SUP`, `ShowOff`, `ITR`, …) | needs xNVSE or one of its plugins | 2,814 |

`family: "vanilla"` marks the 636 functions in the first four rows. **Shipped masters only
call vanilla functions** [observed: no opcode in shipped bytecode resolves to an extender
function]. Extender functions are catalogued for later mod compatibility. They have no
opcodes here, because a running extender assigns their opcodes and the base game files
contain none [hypothesis]. Build them only when mod support is planned.

Some vanilla rows are console helpers or wiki pseudo-entries (`Return` is a statement, see
§3.2; short entries such as `bat` and `clr` are console commands). Check `console_only` and
the categories before implementing.

## 3. Compiled script form: what the bytecode shows

### 3.1 Where scripts live [documented: xEdit][observed]

A script is `SCHR` (header) + `SCDA` (compiled bytes) + `SCTX` (source text) + local
variables (`SLSD`/`SCVR`) + references (`SCRO` FormID or `SCRV` local index). Standalone in
`SCPT`, embedded in `INFO` (dialogue results), `PACK`, `QUST` (stage results), `TERM`,
`REFR` (patrol data) and `PERK` entries.
SCHR: unused u32, reference count u32, compiled size u32, variable count u32, type u16
(0 object, 1 quest, 0x100 effect), flags u16. In the masters, 14,511 units have both SCDA
and SCTX (SCPT 3,715; the rest embedded) [observed].

### 3.2 Statement stream [observed]

SCDA is a flat sequence of statements: **opcode u16, body length u16, body**. A statement
with opcode `0x001C` has no length. It is followed by a u16 reference index and then the
real statement, which it applies to that reference (`Ref.Func`). This reading consumes
every byte of all 14,511 SCDA blobs.

Each source line, after removing comments, blank lines and `short/int/long/float/ref`
declarations, compiles to exactly one statement. This holds for 14,501 of 14,511
scripts; the other 10 differ in line count. That alignment is how opcodes were named.

| Opcode | Statement | Body (observed shape) |
|---|---|---|
| 0x0010 | `Begin` | event code u16, u32 block length = bytes from the end of this statement through the matching `End` inclusive (5,702/5,702), then event parameters |
| 0x0011 | `End` | empty |
| 0x0015 | `Set` | target, then u16 expression length, expression. Target starts with `s`, `f`, `r` or `G` (local short, local float, `ref.var`, global) [hypothesis on the letter meanings] |
| 0x0016 | `If` | u16 = statements to skip to the next `ElseIf`/`Else`/`EndIf` at the same depth (16,846 of 16,851), u16 expression length, expression |
| 0x0017 | `Else` | u16 skip count as for `If` (2,618 of 2,619) |
| 0x0018 | `ElseIf` | as `If` (5,931/5,931) |
| 0x0019 | `EndIf` | empty |
| 0x001D | `ScriptName` | empty; always first |
| 0x001E | `Return` | empty |
| ≥ 0x1000 | function call | u16 parameter count, then the parameters |

Function-call parameters seen so far [observed]: `r` (0x72) + u16 = a reference by
1-based index into the script's reference list; `n` (0x6E) + i32 = an integer literal.
Enumerated parameters such as an actor value or an animation group are a bare u16
code. Example: `SetActorValue Assistance 0` gives `39 00` then `n`. Other leading bytes
also occur (`z`, `Z`, `G`, small integers) and have **not been analysed yet**.

Expressions (the bodies of `If`, `Set`, `ElseIf`, `Return`) are postfix (reverse
Polish) token streams separated by spaces (0x20) [observed]. Numbers and operators are
ASCII text: `If (b == 0)` compiles to `s <idx> 0 ==`. A call inside an expression is
`X` (0x58) + opcode u16 + parameter-bytes u16 + the same parameter encoding as above.
`r` + u16 before `X` or before a variable makes it act on that reference.

### 3.3 Begin-block event codes [observed]

Named from the aligned `Begin <Name>` line. "Scripts" counts how many use each code.

| Code | Block | Scripts | Code | Block | Scripts |
|---|---|---|---|---|---|
| 0x00 | GameMode | 1,015 | 0x13 | ScriptEffectUpdate | 44 |
| 0x01 | MenuMode | 83 | 0x14 | OnPackageChange | 5 |
| 0x02 | OnActivate | 762 | 0x15 | OnLoad | 412 |
| 0x03 | OnAdd | 270 | 0x16 | OnMagicEffectHit | 5 |
| 0x04 | OnEquip | 17 | 0x17 | OnSell | 2 |
| 0x05 | OnUnequip | 14 | 0x18 | OnTrigger | 250 |
| 0x06 | OnDrop | 19 | 0x19 | OnStartCombat | 147 |
| 0x07 | SayToDone | 308 | 0x1A | OnTriggerEnter | 1,001 |
| 0x08 | OnHit | 226 | 0x1B | OnTriggerLeave | 121 |
| 0x09 | OnHitWith | 34 | 0x1C | OnActorEquip | 1 |
| 0x0A | OnDeath | 468 | 0x1E | OnReset | 45 |
| 0x0C | OnCombatEnd | 36 | 0x1F | OnOpen | 3 |
| 0x0F | OnPackageStart | 10 | 0x20 | OnClose | 2 |
| 0x10 | OnPackageDone | 21 | 0x21 | OnGrab | 4 |
| 0x11 | ScriptEffectStart | 257 | 0x22 | OnRelease | 3 |
| 0x12 | ScriptEffectFinish | 57 | 0x23 | OnDestructionStageChange | 16 |
| | | | 0x24 | OnFire | 6 |

Codes 0x0B, 0x0D, 0x0E and 0x1D are not used by any shipped script. Their block names are
unknown; do not guess them.

## 4. Function opcodes

- **Observed (354 vanilla functions).** Top-level calls are named directly from the aligned
  source line. Calls inside expressions are named by line co-occurrence: for each opcode,
  the documented function with the highest Jaccard index across the expression lines that
  contain it. Every observed opcode resolves to exactly one function and no two functions
  share one. Shipped bytecode uses opcodes 0x1001 through 0x127F.
- **The xEdit rule.** Opcode = 0x1000 + xEdit condition index. It holds for all 116
  functions that have both an observed opcode and an xEdit index [observed]. The 101
  vanilla functions that have an xEdit index but are not called by shipped scripts get
  their opcode from this rule (`opcode_evidence: "xedit_index"`) [documented + observed
  rule].
- **Unknown (181 vanilla functions)**, mostly console and debug commands, are never called
  by shipped scripts and have no xEdit index. Do **not** fill the gaps by counting
  (opcodes are not guaranteed to be dense or ordered by name). How to get them is an open
  item in §7.
- Undocumented spellings seen in shipped source are recorded per function as
  `observed_spellings` (`sifh` = SetIgnoreFriendlyHits, `Gifh`, `setcs`). They are compiler
  aliases the wiki does not list [observed].
- Eight functions are called by shipped scripts but have no wiki page:
  GetShouldAttack (0x1042), StartMisterSandman (0x107C), trapUpdate (0x110C),
  setAllReachable (0x1159), TriggerScreenSplatter (0x1240), SetScreenSplatterFade (0x1243),
  IsDLCInstalled (0x126F), SwapTexture (0x1269). Their behaviour is undocumented. Treat
  them as `[hypothesis]` from their names until observed in game.

**Anomaly** [observed]: `GetInCharGen` compiles to 0x127F in three Old World Blues scripts
and to 0x127C in one (OldWorldBlues.esm SCPT 0x010154E8). 0x127C is the only unresolved
opcode. A possible cause is a script compiled against an older command table, or source
edited after compiling [hypothesis]. The runtime should dispatch on opcode, not name, so
treat 0x127C as unknown until a source explains it.

Conditions: CTDA stores the same index as a u16 at byte 8 [documented: xEdit]. The
masters use 138 distinct vanilla condition functions in 80,333 conditions. The runtime
should share one function table between scripts (opcode) and conditions (index =
opcode − 0x1000) [hypothesis, consistent with the rule above].

## 5. Catalogue format (`fnv.script.functions/1`)

Top level: `sources`, `scan` (alignment statistics), `statement_opcodes`, `begin_blocks`,
`opcode_conflicts`, `unresolved_opcodes`, `anomalies`, `functions`. Each function has:

| Field | Meaning | Tag |
|---|---|---|
| `name`, `aliases`, `page`, `wiki` | documented name, short names, wiki page and URL | documented |
| `origin`, `origin_version`, `family` | wiki origin code; `vanilla` or `extender` | documented |
| `calling_reference` | wiki text for the reference it runs on (`ref.Func`); absent = no reference | documented |
| `params[]` | `name`, raw wiki `type`, normalised `kind` (`int`, `float`, `reference`, `form`, `string`, `axis`, `actor_value`, `variable`, `array`, `other`, `unknown`), `optional` | documented |
| `returns` | `type`, `name` | documented |
| `opcodes`, `opcode_evidence` | `observed` or `xedit_index` (§4) | observed / documented |
| `condition_index`, `xedit_param_types` | from xEdit | documented |
| `script_calls`, `scripts_using`, `condition_uses` | use in shipped masters, for setting priorities | observed |
| `observed_spellings` | undocumented spellings in shipped source | observed |
| `summary`, `notes[]`, `bugs[]` | our paraphrase of the wiki page; `bugs` = original-engine quirks reported there | documented (paraphrased) |
| `categories`, `console_only`, `wiki_condition_func` | wiki metadata | documented |

The wiki's argument types are free-form (`0/1`, `x/y/z`, `StatName`, `baseform`, …).
`kind` is a rough grouping; the bytecode encoding (§3.2) is what decides.

## 6. Guidance for the runtime session

- Dispatch on **opcode**, never on name. Build the table from `functions.json`. Unknown
  opcodes must fail loudly in tests and skip safely in game.
- Work order: sort vanilla functions by `script_calls` + `condition_uses`. About 40
  functions cover most calls. The top ten are Disable, Enable, SetObjectiveDisplayed,
  SetStage, SetObjectiveCompleted, EvaluatePackage, AddItem, ShowMessage, MoveTo and
  RemoveItem [observed].
- A function with `calling_reference` runs on the `0x001C` / `r` reference when one is
  given. Without one it presumably runs on the object the script is attached to
  [hypothesis; the shipped source relies on this, e.g. a bare `Disable` in an object
  script].
- `bugs[]` lists original-engine quirks. Whether to reproduce each one is a parity
  decision; record it in the parity lab, not in code comments.
- Read the wiki page in the private dump for full detail. Re-express what you use in
  `notes.json` (own words, tagged) and regenerate.

## 7. Open items

- ~~Parameter encodings beyond `r`/`n` (`z`, `Z`, `G`, raw codes); the trailing bytes of
  `ShowMessage`; the meaning of the `Set` target letters (§3.2).~~ Resolved 2026-10-09 in
  [scripts.md](scripts.md) §2.4–2.5: `z` = f64 literal, `G` = global, `Z` = an axis
  letter as a parameter and a reference value in expressions, raw codes = enumerated u16
  or strings, depending on the function (encodings are per parameter, listed in
  `crates/fnv-script/data/encodings.json`); ShowMessage's trailing bytes = format
  variables + duration; `s`/`f` = Integer/FloatOrRef locals, `r` = another script's
  variable. Every unit of the masters decodes with no unexplained byte.
- Opcodes for the 181 vanilla functions without evidence. Options: a clean-room source
  that documents them, or the owner compiling one-line test scripts in the GECK and
  reading the bytecode back (a valid observation, but the owner decides).
- ~~The 10 scripts whose line count does not match~~: lines without a letter or digit
  produce no statement; each script is explained in scripts.md §3.4. The GetInCharGen
  anomaly (§4) stays open: the decoder keeps 0x127C as an unknown opcode.
- Block names for the unused event codes 0x0B, 0x0D, 0x0E and 0x1D.
- ~~A Rust reader for this format~~: `crates/fnv-script` (Layer 5, scripts.md).
- Parameter lists the wiki leaves out: ShowCaravanMenu, SetNPCRadio, StartCannibal,
  EnableLoadingMenu and Unlock take more parameters than `params` lists (scripts.md
  §3.6); 112 called functions never have a parameter block (scripts.md §2.2).
