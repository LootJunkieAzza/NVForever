# Owned AttachModel smoke inputs

This fixture generates an authored NIF into a new, separate Data view and HOME.
It does not copy installed scripts or assets. The ordinary `ModelCtx` preflight
must report actual prepared shapes and CPU geometry/material fields before
AttachModel is called; file hashes and a Python recorder are insufficient.

Create a pinned JSON inventory with schema
`nvf.attach-model.read-only-game-inputs/1`, absolute `sourceData`, and explicit
master/BSA `files` rows (`name`, `kind`, `resolvedSource`, `bytes`, `SHA256`).
Both kinds are required. Selected input files are linked into the new Data view;
read-only use is an operator contract plus hash checks, not OS access control.
The resolved output root and source Data must be disjoint. Creation refuses
overlap before making directories; admission checks it again for existing views.
This includes resolved directory aliases, while similarly named siblings remain
eligible. The generator and test-only literal integrity inputs prove no native
file format, installed profile, or game behavior.

Run `prepare.py create --new-root NEW --inventory INPUTS --inventory-sha SHA`.
Then call `prepare.admit` with the generated manifest hash, exact launch Data and
HOME, and `for_game=True` before passing its returned environment to the canonical
smoke runner. The smoke CLI enforces required scalars but does not itself call
this manifest validator. Supply Data through its existing `--data` option;
HOME/mods is not a loose-mesh source. Recheck source input hashes after a real
game process. A recorder-only view uses `for_game=False` and earns no game credit.

`NVF_PLAY_TEST_ATTACH_MODEL_PROBE_PATH` is the admitted full VFS model path,
`meshes/nvf_smoke/attach_model.nif`. Do not strip `meshes/`: ordinary
`ModelCtx::model` reads this complete key. The script's AttachModel specification
uses its own existing prefix convention.

The `nif-reference-attach-owned` scenario requires HOME, manifest and model-path
opt-ins. It logs script dispatch and removal submission, then judges actual
physical fields and retirement. Run `judge_fields.py --log THE_SAME_COMPLETE_LOG`
as well as the ordinary scenario verdict. Both must accept. The field judge pairs
the two prepared and attached widths with their bound material roughness and
checks exact captured entities, recipient, retention and disappearance.

Absent the diagnostic opt-in, no probe resource or per-frame system is installed.
When enabled, one ordinary-cache model is prepared before render extraction;
the probe neither retains CPU meshes globally nor invents models or references.
The preparation, launcher tests, diagnostic Rust module and GAME scenario require
qualification against the selected source and normal artifact. Their presence
alone is no rendering, native-parity or full-mod acceptance claim.

The empty-target submission can return either 0 or 1 under the existing core
mirror lookup while the physical producer separately consumes the queued request.
The scenario and judge record that return without treating it as attachment
success. Actual prepared geometry/material pairs, capture, retention and ordered
retirement remain required independently.
