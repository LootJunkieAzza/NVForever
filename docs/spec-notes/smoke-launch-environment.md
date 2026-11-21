# Smoke launch environment placeholders

Implementation context (repository source inspection): at public main
`da13f4f8a65cb56d9f4f38823d79a0126ff9467b`, `tools/smoke/smoke/runner.py`,
functions `expand`, `run_launch` and `missing_requirements`, tool command
arguments support `{env:NAME}`. Game launch environment values substitute
`{repo}` and `{serve}` only. Required scenario variables are checked before
launch; an absent or empty variable skips that scenario.

Review context (maintainer feedback):
https://github.com/LootJunkieAzza/NVForever/pull/2562#issuecomment-6884654372 and
https://github.com/LootJunkieAzza/NVForever/pull/2585#issuecomment-6886691502
identify a texture filename and a seeded `NVFOREVER_HOME` authored with
`{env:NAME}` inside game environment values. Passing those values literally does
not select the owner's texture or seeded home. These are implementation and
review notes, not observations of the original game.

[hypothesis: proposed runner policy] Expand `{env:NAME}` in each game launch
environment value from the invoking process before starting the child. Preserve
one scalar string: no glob expansion, shell interpretation, argument splitting,
or recursive `{env:NAME}` expansion of inserted values. Preserve existing `{repo}`
and `{serve}` substitution. An absent or empty referenced variable returns a launch
error before any child starts, including direct `run_launch` callers; normal
`requires` skipping remains unchanged. No fallback home or texture is chosen.

[hypothesis: authored regression design] An owned Python child, started through
real `run_launch`, reports its actual environment and reads only authored files
under a temporary directory. One control checks the complete texture-bearing
console value; another reads an authored UDF under the selected home. Additional
controls check missing/empty opt-ins start no child, scalar metacharacters stay
literal, and existing repo/server substitution remains intact. These checks prove
runner delivery only, not a rendered texture, engine UDF execution or native game
behavior. No game launch or installed files are needed.
