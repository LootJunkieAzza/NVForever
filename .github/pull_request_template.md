## What changed

## How it was checked
<!-- Which checks from CLAUDE.md ("Checks before every push") and which smoke
     scenarios (`python tools/smoke plan`) you ran, and what they printed. -->

## Clean-room checklist
- [ ] No game data, goldens or game-derived dumps are included.
- [ ] Format and behaviour facts are written in `docs/spec-notes/` and tagged
      `[documented]` (with a source), `[observed]` or `[hypothesis]`.
- [ ] Nothing was taken from leaked or decompiled Bethesda/Gamebryo code, or copied
      from nv-rs (format facts only).
- [ ] No parity-lab goldens, scenarios, tolerances or comparators were changed to make
      a test pass.
