# CLAUDE.md — standing instructions for AI sessions on NVForever

NVForever is an open-source, clean-room reimplementation of the Fallout: New Vegas engine
(OpenMW-style). The owner wants sessions to work autonomously, including pushing.

## Charter (non-negotiable)

- Clean room: describe formats/behaviour in plain language in `docs/spec-notes/`, implement
  from those notes. No leaked Bethesda/Gamebryo source. No DRM circumvention (the Steam
  `FalloutNV.exe` code section is encrypted; do not unpack it).
- Tag every factual claim `[documented]` (cite page/file), `[observed]` or `[hypothesis]`.
  Never invent offsets, opcodes or field meanings.
- Ship no game data. Goldens and game-derived dumps stay private and are never committed
  (`.gitignore` blocks game files).
- Work in layer order (docs/design/layer1.md; Layer 2 = NIF/DDS/KF is next).
- NVForever never writes into the game folder or `Documents\My Games\FalloutNV`
  (`crates/fnv-install`, docs/design/install-layout.md).
- Use the Rust ecosystem fully (owner decision); plan in docs/design/dependencies.md.

## Parity lab rules (parity-lab/AGENT.md)

Never edit goldens, scenarios, tolerances or comparators to make tests pass; never
special-case inputs; never read holdouts or pass `--reveal`; never run `fnvlab baseline`.
Fix or explain every regression.

## Checks before every push

```
cargo test --workspace --release
python -m unittest discover -s parity-lab/tests
python -m unittest discover -s tools/capture/tests
```

With game data available (owner's install, read-only): build `fnvtool`, set
`parity-lab/lab.local.json`, run `python -m fnvlab run` and report the summary.

## Commits

End commit messages with the session's attribution lines. Keep commits focused; write
what changed and what was verified.
