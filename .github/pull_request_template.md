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
- [ ] Nothing was taken from leaked, decompiled or disassembled Bethesda/Gamebryo code
      (no Ghidra, IDA or similar on the game or the G.E.C.K.), and nv-rs was not opened.
- [ ] Every new constant or behaviour is backed by a cited source or an observation
      in the original game, and no text the game shows was transcribed from it.
- [ ] No parity-lab goldens, scenarios, tolerances or comparators were changed to make
      a test pass.
- [ ] This is my own work (or I have the right to submit it), I license it under
      GPL-3.0-or-later, and my commits are signed off (`git commit -s`, the
      [Developer Certificate of Origin](https://developercertificate.org/)).
