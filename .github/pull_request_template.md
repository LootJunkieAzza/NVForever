## What changed

## How it was checked
<!-- Which checks (CONTRIBUTING.md, "Making a change") and which smoke scenarios
     (`python tools/smoke plan`) you ran, and what they printed. No copy of the game?
     Say so and the maintainer will run the game checks. -->

## Clean-room checklist
See [CONTRIBUTING.md](https://github.com/LootJunkieAzza/NVForever/blob/main/CONTRIBUTING.md#clean-room-rules).
- [ ] No game data, goldens or game-derived dumps are included.
- [ ] Format and behaviour facts are written in `docs/spec-notes/` and tagged
      `[documented]` (with a source), `[observed]` or `[hypothesis]`.
- [ ] Nothing was taken from leaked or decompiled Bethesda/Gamebryo code, or copied
      from nv-rs (format facts only).
- [ ] No parity-lab goldens, scenarios, tolerances or comparators were changed to make
      a test pass.
- [ ] This is my own work (or I have the right to submit it), and I license it under
      GPL-3.0-or-later.
