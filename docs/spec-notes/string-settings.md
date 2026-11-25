# String game settings and the talk prompt

[documented: [SetStringSetting](https://geckwiki.com/index.php/SetStringSetting)]
The source function accepts a setting name and its new string value.
[documented: [GetStringSetting](https://geckwiki.com/index.php/GetStringSetting)]
The getter returns the named string setting. These are game settings, separate from
INI settings and their section-qualified names.

[design] Source SetStringSetting writes the existing World game-setting store. Like
its console neighbor, it refuses a missing setting or a setting of another type.
It keeps the supplied string intact, including spaces, percent signs and an empty
value. The void source command has the VM's ordinary numeric-zero result; this is
not a native return-value observation. Strict/default host-error recovery and the
VM's explicit-empty receiver refusal are unchanged.

[design] GetStringSetting reads the effective session value through the same World
reader as the console setter. The numeric game-setting route is unchanged. A missing
or non-string getter still returns the existing empty string. Changes remain scoped
to the current World session; no new save field or INI/game-folder write is added.

[hypothesis] A mod that changes sTargetTypeTalk expects its next regular talk prompt
to use that value. NVForever maps the existing Talk action to this effective string;
without a string setting it keeps its own label. Reference/base XATO overrides retain
precedence. Other action labels, target names, access restrictions and input bindings
are unchanged. sTargetTypeCrown remains a readable/writable string if present, but
there is no new crown action or inferred mapping to another actor category.

[hypothesis] Native invalid-setting, persistence, numeric encoding and complete
installed-mod behavior remain unverified. This service does not establish any
Improved Console provider identity, version or capability.

[validation] The prior hosted engine run reached all three authored prompt controls,
but their layout assertions used `Line::text`, which intentionally omits spaces.
The preceding bound-prompt assertions passed; the console case also read its edited
played-World setting before failing. The source-driver call, XATO and session checks
later in those cases were not reached. The fixture now checks the renderer's existing
gap-aware text and the actual glyph advances, retaining exact spaced expectations.
Production is unchanged. Current validation results are recorded in PR #2914;
native and installed-mod behavior remain unqualified.

## Wave 6D-11 read-only GMST and INI getter baselines

- **GMST string and numeric getters (`xNVSE 6.4.9 + JIP LN NVSE 57.30 (5730)`):**
  `GetStringGameSetting` and `GetStringSetting` match case-insensitively (`"sYes"` / `"syes"`
  -> `"Yes"`), return empty string for missing or numeric-prefixed names, and
  `GetNumericGameSetting` returns exact integer/float values (`iAVDCarryWeightBase = 40`,
  `fJumpHeightMin = 64`) and `-1` for missing or `s`-prefixed string GMSTs
  [observed: 2026-11-25, `GSDocMitchellHouse` (`0x00103DF9`), Fallout: New Vegas 1.4.0.525 +
  xNVSE 6.4.9 + JIP LN NVSE 57.30 (5730), 10 official base masters, read-only `PrintToConsole`
  probe `w6d11_33_gmst_string_and_numeric_getters_readonly`
  (`tools/capture/nvse/catalogs/wave6d11_lists_arrays_strings_aux_math.json`):
  `yes(lu=[Yes],nv=[Yes],lo=[Yes]/[Yes])|mis(lu=0,nv=0)|xStr(lu=0,nv=0)|num(i=40,f=64,lo=64,mis=-1,str=-1)`;
  no DLL or source consulted].
- **INI read-only getters (`xNVSE 6.4.9 + ShowOffNVSE Plugin 184 + JIP LN NVSE 57.30 (5730)`):**
  `GetNumericINISetting` (`-1` on missing / no-colon / string setting), `GetStringINISetting`
  (`"ENGLISH"` for `"General:sLanguage"`), `GetINIFloat`/`GetINIString`/`GetINISection`/
  `GetINISectionNames` on missing files, and ShowOff `HasINISetting`/`GetINIFloatOrCreate`/
  `GetINIStringOrCreate` defaults confirmed
  [observed: 2026-11-25, `GSDocMitchellHouse` (`0x00103DF9`), Fallout: New Vegas 1.4.0.525 +
  xNVSE 6.4.9 + ShowOffNVSE Plugin 184 + JIP LN NVSE 57.30 (5730), 10 official base masters,
  read-only `PrintToConsole` probe `w6d11_34_ini_getters_readonly_xnvse_lu_showoff`
  (`tools/capture/nvse/catalogs/wave6d11_lists_arrays_strings_aux_math.json`):
  `nvse(g=5:5,nc=-1,ms=-1,sn=-1,lang=[ENGLISH],f1L=39)|hlp(mis=2,numAsS=2)|lu(f=0,nc=0,sL=0,sec=0,nms=0)|so(has=0,f=42.5,s=[dflt])`;
  no DLL or source consulted].
