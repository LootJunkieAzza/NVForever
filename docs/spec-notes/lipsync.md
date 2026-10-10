# Spec note: lip sync (`.lip` files and the face morphs they drive)

Clean-room description in plain language (docs/design/dialogue.md step D6). The `.lip`
reader is `crates/fnv-voice/src/lip.rs`, the `.tri` morph reader
`crates/fnv-actor/src/tri.rs`, the animation in the game
`engine/nvforever/src/lips.rs`. Confidence tags: **[documented]** cites a public source,
**[observed]** was checked on the owner's install (FNV 1.4.0.525, Steam, English, the
ten masters and their archives), **[hypothesis]** needs verification.

**The `.lip` format has no public documentation that this work could find** (searched
2026-10-10: modding wikis, tool documentation, format notes; the only "LIP format" page
found describes Fallout 2's unrelated format). Its layout below is therefore
[observed], established from the files alone, and each point says how. No game or
editor binary was disassembled, no leaked or decompiled code was read, and no other
tool's code or format notes were consulted (nv-rs was not opened for this work).

How the observations were made (2026-10-10): all 60,702 `.lip` files and the 49 `.tri`
files were read from the archives with throw-away scripts (kept privately, outside the
repository) and then by `fnvtool voice lip-coverage` (`fnv.voice.lip_coverage/1`) and
`fnvtool actor tri` (`fnv.actor.tri/1`), which reproduce the counts below and are
repeated as tests (`crates/fnv-voice/tests/real_data.rs`,
`crates/fnv-actor/tests/real_data.rs`, run when `NVFOREVER_DATA` is set). Two checks
used the sounds themselves (decoded to loudness with a scratch program) and are marked
"scratch": they are not repeated by a test. No file, text or sound of the game is in the
repository.

Sources:
- FaceGen SDK manual, "File Formats" (https://facegen.com/dl/sdk/doc/manual/fileformats.html):
  the TRI layout.
- PyFFI documentation, `pyffi.formats.tri`
  (https://pyffi.sourceforge.net/pyffi.formats.tri.html): morph and modifier records of
  TRI files as Oblivion uses them; its example lists the morphs of an Oblivion mouth
  (`Fear, Surprise, Aah, BigAah, BMP, ChJSh, DST, Eee, Eh, FV, I, K, N, Oh, OohQ, R, Th,
  W`).
- GECK wiki, "ModifyFaceGen" (https://geckwiki.com/index.php/ModifyFaceGen): the console
  command `MFG` and its numbered Expression, Phoneme and Modifier IDs. The page refused
  the fetch tool (HTTP 403), so it was read through search-engine summaries of it; what
  those confirmed is listed in §3.2.
- xEdit record definitions as recorded in `schemas/fnv/gameplay.json` (INFO `TRDT`:
  emotion type and value).

## 1. Where the files are

One `.lip` beside each voice sound, same name (docs/spec-notes/voice.md §2.4): 60,702
files for 61,290 sounds; 588 sounds have none, and no `.lip` is without its sound
[observed]. Sizes run from 128 to 59,842 bytes, 343,494,953 bytes together [observed].
Voice folders of speakers that have no human head have them too (`robotyesman`,
`creatureuniquemrhouse`, … [observed: listing]).

## 2. The file

Everything in this section is [observed] over all 60,702 files unless tagged otherwise.
All numbers are little-endian.

### 2.1 Header (12 bytes)

| Bytes | Type | Value |
|---|---|---|
| 0–3 | u32 | 1 in every file [hypothesis: a format version] |
| 4–7 | u32 | the size of the unpacked body (§2.3) plus 16, in every file [why 16: unknown] |
| 8–11 | u32 | 1 in every file [meaning unknown] |

### 2.2 Packing

The rest of the file is the body with its zero bytes packed:

- a byte other than 0 stands for itself;
- a 0 byte is followed by a 16-bit count and stands for that many 0 bytes.

How it was established: read this way, every file unpacks to exactly the size its header
declares (minus 16), and that size is exactly 8 + 132 × the frame count the body starts
with (§2.3). Packing the unpacked body again — every run of zeros as one marker — gives
back all 60,702 files byte for byte, so the rule is complete in both directions. The
longest run in the files is 21,428 zeros; what a run longer than 65,535 looks like is
[hypothesis: split into several markers; no file has one]. Packing shrinks the bodies to
about 31 % (1,114 MB unpacked).

### 2.3 Body

| Bytes | Type | Meaning |
|---|---|---|
| 0–3 | u32 | N, the number of frames (15 to 1,260; 8,436,876 in all files) |
| 4–7 | i32 | S, the start frame: where frame 0 sits on the sound's clock (§2.4); −36 to 6 |
| 8… | N × 33 × f32 | per frame, one weight for each of 33 channels (§3) |

No value is NaN or infinite. Nothing follows the last frame.

Start frames: −4 in 14,752 files, −3 in 8,198, −5 in 6,328, −8 in 5,321, 0 in 4,837,
−9 in 3,704; above 0 in only 35.

### 2.4 Time

**30 frames per second; frame f belongs to the moment (f + S) ÷ 30 seconds after the
sound starts.** A negative S means the animation begins before the sound. This is
[observed], from four checks:

1. Frame count against sound length: over all files a straight-line fit of N against the
   sound's seconds has slope 30.04 frames per second (scratch).
2. The end: counting (last frame with any mouth-shape weight + 1 + S) against the sound's
   length × 30, the two agree within one frame in 45,042 files (74.2 %); in 8,847 files
   the mouth goes on 6 to 10 frames past the sound (a closing mouth after the last
   word), in 3,821 it stops 2 to 5 frames early; the rest are scattered (565 more than a
   second early, 70 more than a second late: sounds that were re-cut after their `.lip`
   was made, as far as one can tell). Without S the same count is off by S frames.
3. The start: where S is strongly negative the first mouth-shape weight appears about 3
   frames before frame −S (S −26, first weight at frame 23; S −18 → 15; S −5 → 1 …),
   that is 0.1 s before the sound; the frames before that carry only brow, eye and head
   channels (scratch, the Doc Mitchell lines).
4. Loudness (scratch, 3,106 files, every twentieth): sliding the summed mouth-shape
   weights along the sound's loudness (30 values a second), the best fit is at S + 1 to
   S + 5 frames in 84 % of the files, most often S + 2 and S + 3. So the mouth leads the
   loudness by about 0.1 s — consistent with weights that ramp up over 8 to 9 frames
   before a sound: after a pause in the animation (688 cases), the sound comes back 8 to
   10 frames after the first weight in 87 % of them.

What is not known: whether the game shows the frames that lie before the sound's start
(it would have to delay the sound or start mid-animation), whether it mixes neighbouring
frames between its own frames, and what it does when a `.lip` and its sound disagree in
length. The runtime's choices are in §5.

### 2.5 Total animation against total sound

The files hold 281,229 s of animation for 251,461 s of sound: the animation is usually a
little longer than its sound at both ends (S frames before, closing frames after).

## 3. The 33 channels

### 3.1 What the data shows [observed]

| Channel | Files using it | Lowest | Highest | Behaviour |
|---|---|---|---|---|
| 0 | 59,298 | 0 | 1.0 | mouth |
| 1 | 32,215 | 0 | 0.8 | mouth |
| 2 | 50,513 | 0 | 1.0 | mouth |
| 3 | 39,889 | 0 | 1.0 | mouth |
| 4 | 57,183 | 0 | 1.0 | mouth |
| 5 | 56,155 | 0 | 1.0 | mouth |
| 6 | 53,372 | 0 | 1.0 | mouth |
| 7 | 41,841 | 0 | 1.0 | mouth |
| 8 | 44,441 | 0 | 0.8 | mouth |
| 9 | 40,382 | 0 | 1.0 | mouth |
| 10 | 54,878 | 0 | 1.0 | mouth |
| 11 | 55,327 | 0 | 0.6 | mouth |
| 12 | 40,169 | 0 | 1.0 | mouth |
| 13 | 49,911 | 0 | 1.0 | mouth |
| 14 | 51,506 | 0 | 1.0 | mouth |
| 15 | 43,866 | 0 | 1.0 | mouth |
| 16, 17 | 58,526 | 0 | 1.0 | equal to each other in every frame of every file; short pulses, 5 to 7 frames long, peaking at 0.9–1.0, about one every 3.2 s |
| 18–21 | 0 | | | never used |
| 22, 23 | 56,358 | 0 | 1.43 | equal to each other in every frame of every file; slow swells |
| 24 | 45,560 | 0 | 0.09 | never non-zero together with 27 |
| 25 | 30,449 | 0 | 0.12 | never non-zero together with 26 |
| 26 | 20,213 | 0 | 0.09 | |
| 27 | 44,825 | 0 | 0.18 | |
| 28, 29 | 0 | | | never used |
| 30 | 55,648 | −0.08 | 0.17 | signed; (channel 24 − channel 27) = −(channel 30) in every frame of 42,312 files |
| 31 | 27,368 | −0.05 | 0.09 | signed |
| 32 | 29,371 | −0.07 | 0.11 | signed; (channel 25 − channel 26) = +(channel 32) in most files (scratch) |

Every file has at least one mouth weight. At most seven mouth channels are above 0.05 in
one frame, usually one to three (scratch).

**The letter test.** For the 60,600 files whose response text the records give, each
consonant channel was compared with the letters of the text: "used" means the channel
reaches a weight of 0.3 somewhere in the file.

| Channel | Letters looked for | Lines with them: channel used | Lines without: channel used |
|---|---|---|---|
| 2 | b, m, p | 50,300 of 50,978 (98.7 %) | 118 of 9,622 (1.2 %) |
| 3 | ch, j, sh, ge, tio | 22,736 of 26,516 (85.7 %) | 4,458 of 34,084 (13.1 %) |
| 4 | d, s, t, z | 56,152 of 57,958 (96.9 %) | 232 of 2,642 (8.8 %) |
| 7 | f, v, ph | 41,222 of 43,006 (95.9 %) | 521 of 17,594 (3.0 %) |
| 9 | k, g, q, x, c | 35,974 of 54,241 (66.3 %) | 31 of 6,359 (0.5 %) |
| 10 | n, l | 53,104 of 55,933 (94.9 %) | 140 of 4,667 (3.0 %) |
| 13 | r | 44,111 of 51,115 (86.3 %) | 160 of 9,485 (1.7 %) |
| 14 | th | 41,401 of 42,226 (98.0 %) | 467 of 18,374 (2.5 %) |
| 15 | w, qu, one | 34,435 of 41,213 (83.6 %) | 443 of 19,387 (2.3 %) |

A spelling test is crude (a silent letter, a c said as s), which is where the misses come
from; the separation is plain all the same. In the base game's lines, channels 11 and 12
are used by almost no line without an o (0.0 % and 5 %), and channel 1 by 1 % of them
(scratch).

### 3.2 Names

The game numbers its face movements for the console command `MFG`
[documented: GECK wiki, "ModifyFaceGen"]: 16 **phonemes** numbered 0–15 (0 Aah, 1 BigAah,
2 BMP, 3 ChJSh … 15 W), 17 **modifiers** numbered 0–16 (0 BlinkLeft, 1 BlinkRight,
2 BrowDownLeft, 3 BrowDownRight, 4 BrowInLeft, 8 LookDown, 9 LookLeft, 12 SquintLeft,
14 HeadPitch, 15 HeadRoll, 16 HeadYaw; "Modifier 9" looks left), and 15 expressions
(0 Anger … 14 CombatAnger), each set from 0 to 100. The numbers not confirmed one by one
(phonemes 4–14, modifiers 5–7, 10, 11, 13) are filled in alphabetically, which is the
order the confirmed ones follow and the order of the phoneme morphs in the head `.tri`
files (§4) [hypothesis].

16 + 17 = 33, the number of channels. **Channel k is phoneme k for k < 16, and modifier
k − 16 from 16 on:**

| Channels | Names | How sure |
|---|---|---|
| 2, 3, 4, 7, 9, 10, 13, 14, 15 | BMP, ChJSh, DST, FV, K, N, R, Th, W | [observed]: each follows the letters its name stands for (§3.1), at exactly the position the phoneme list gives it |
| 0, 1, 5, 6, 8, 11, 12 | Aah, BigAah, Ee, Eh, I, Oh, OohQ | [hypothesis: by position between the confirmed ones; 11 and 12 go with the letter o] |
| 16, 17 | BlinkLeft, BlinkRight | [observed: an always-equal pair of quarter-second pulses every few seconds, at the position of the list's first two modifiers] |
| 22, 23 | BrowUpLeft, BrowUpRight | [hypothesis: by position; an always-equal pair, as a brow pair would be] |
| 18–21, 28, 29 | BrowDownLeft/Right, BrowInLeft/Right, SquintLeft/Right | [hypothesis: by position; no file uses them] |
| 24–27 | LookDown, LookLeft, LookRight, LookUp | [hypothesis: by position; the pairs that never occur together (down/up, left/right) are opposite directions under these names] |
| 30, 31, 32 | HeadPitch, HeadRoll, HeadYaw | [hypothesis: by position; the only signed channels, as rotations would be, and the look channels mirror two of them — eyes turning against the head] |

The head `.tri` files label the long E `Ee` (the phoneme list: `Eee`); the reader uses
the `.tri` spelling.

### 3.3 Units

- Mouth channels: 0 to 1 [observed]; 1 is taken as the whole morph of that name (§4)
  [hypothesis].
- BrowUp reaches 1.43 [observed]; whether the game caps a weight at 1 is not known.
- Look and head channels: small numbers (to 0.18) whose unit is **unknown** — a share of
  the Look morph, radians, or something else. They are equal and opposite to each other
  (§3.1), so they share a unit.

## 4. The morphs (`.tri`)

### 4.1 Layout

[documented: FaceGen SDK manual, "File Formats"] The magic `FRTRI003`, ten 32-bit counts
(vertices V, triangles, quads, labelled vertices, labelled surface points, texture
coordinates, extension flags, difference morphs, statistical morphs, statistical morph
vertices K), 16 reserved bytes; V + K vertices; triangles and quads; labels; texture
coordinates; then the morphs:

- a **difference morph**: a label, a scale and V × 3 signed 16-bit deltas; the vertex
  moves by delta × scale at full weight;
- a **statistical morph** ("modifier"): a label and a list of vertex numbers; the K extra
  vertices after the first V are their targets, in order — at full weight the listed
  vertex sits at its target [documented: PyFFI, "ModifierRecord"].

[observed] All 49 `.tri` files of the install parse to their last byte this way; none has
quads or labels; a label's stored length includes its closing NUL.

### 4.2 Which parts carry which morphs [observed, `fnvtool actor tri`]

| Files | Vertices | Morphs |
|---|---|---|
| 8 heads (`headhuman`, `headfemale`, `headold`, `headoldfemale`, `headchild`, `headchildfemale`, `headghoul`, `headghoulfemale`) | 1,211 | 38 difference morphs: the 16 phonemes, BrowDown/In/Up Left/Right, 6 expressions (Anger, Disgust, Fear, Happy, Sad, Surprise), 10 moods (MoodNeutral … CombatAnger); 8 statistical: BlinkLeft/Right, SquintLeft/Right, LookDown/Left/Right/Up (LookLeft and LookRight move no head vertex) |
| 3 mouths | 27 | Aah, Anger, BigAah, Surprise, W |
| 3 lower teeth | 14 | Aah, BigAah, Ee, Eh, Fear, I, K, N, Oh, Th, W, CombatAnger |
| 3 upper teeth | 14 | none |
| 3 tongues | 27 | Aah, BigAah, Eh, Fear, N, Oh, Th, W |
| 8 eyes | 49 | statistical LookDown, LookLeft, LookRight, LookUp (49 vertices each: the whole eye) |
| 17 beards | 60–562 | most phonemes, expressions and moods; some have squints |
| 3 eyebrows, the eye patch | 168–248 | brows, blinks, squints, looks, expressions and moods |

The morphs are found **by label**: their order differs from file to file, and no part
but the head has them all. A part without a morph for a channel is not moved by it.

On a male head the largest displacements at full weight are: BigAah 1.94 units, ChJSh
0.91, OohQ and W 0.87, Oh 0.84, R 0.83, N 0.77, Ee 0.66, Eh 0.63, Aah 0.62, I 0.59, DST
0.54, K 0.54, Th 0.39, FV 0.38, BMP 0.28; BlinkLeft 0.80; BrowUpLeft 0.42 (the head is
about 20 units tall).

### 4.3 Morph and model

- Every `.tri` has a model of the same name beside it, with one shape of exactly V
  vertices, and **every triangle of that shape joins the same vertex numbers as a
  triangle of the `.tri`** (49 of 49; 18 `.tri` files list more triangles than their
  model). So vertex i of the model is vertex i of the `.tri` [observed].
- The positions agree within 0.001 units for 27 files. The heads differ from their
  `.tri` by up to 0.1 (six heads, 25 vertices), 0.53 (`headhuman`, 675 vertices) and
  0.84 (`headfemale`, 1,087 vertices); six beards by up to 0.43; the child mouths, teeth
  and tongues sit about 2 units away from theirs [observed]. The morphs are
  displacements, so they are added to the model's vertices as they are.
- Every placed NPC with a visible head (3,809 of 3,942 in the ten masters; 133 wear a
  helmet that replaces the head) uses one of the eight heads, and all its head parts and
  beards have a `.tri` [observed, `fnvtool actor appearance --all` against the list].
  Creatures and robots have none.
- How the game combines them is [hypothesis]: drawn vertex = model vertex + FaceGen
  shape (the `.egm`, docs/spec-notes/actors.md §4) + Σ weight × displacement over the
  morphs. For a statistical morph the displacement is target − vertex, both taken after
  the `.egm` shaped them (the `.egm` covers the V + K vertices, actors.md §4), so an
  eyelid's target follows the face.

## 5. What the runtime does (`nvforever --play`)

`engine/nvforever/src/lips.rs`. While `nvf_play::voice` reports a response being said
(with or without sound: `--no-voice` keeps the timing), and its sound has a `.lip`, and
the speaker is a drawn NPC with head parts:

- each head part with a `.tri` of its vertex count (head, mouth, lower teeth, tongue,
  beards, eyebrows; eyes and upper teeth have nothing to apply) is drawn from a mesh of
  its own; each frame its vertices are: the model's, FaceGen-shaped as the actor's face
  is, plus Σ weight × displacement over the channels applied;
- **channels applied**: the 16 phonemes, BlinkLeft/Right, the six brow channels and the
  squints (26 channels; the brow-down, brow-in and squint ones are never used by the
  files);
- **channels not applied**: LookDown/Left/Right/Up and HeadPitch/Roll/Yaw. Their unit is
  unknown (§3.3) and they come as a compensating pair, so applying the eye half alone
  would be wrong;
- the weights for the moment t after the response's sound started are those of frame
  t × 30 − S, the two neighbouring frames mixed linearly [hypothesis]; frames before the
  sound's start are not shown [hypothesis]; after the last frame the face is at rest;
- when the response ends or is cut (another line, the speaker gone) the weights fade to
  rest over 0.15 s [engine choice] and the actor gets its ordinary mesh back;
- normals are not recomputed [engine choice].

One log line per response: `lip: animating <path> (<tracks> tracks, <applied> applied;
<frames> frames, <seconds> s from <start> s) speaker <who>: <shapes> shapes, <morphs>
morphs`, or `lip: skipped <path>: <reason>` (no `.lip`, or the speaker has no face rig:
a creature, a robot, a talking activator, or an actor not drawn), and `lip: cut <path>
…` when a response is skipped or interrupted before its sound is over. `--no-lip` turns
it off. Test aid: `NVF_LIP_TEST_HOLD=<channel>=<weight>[,...]` makes every NPC hold those
weights (screenshots of single morphs; not a game behaviour).

Cost: nothing while nobody speaks. A speaking actor rewrites about 1,500 to 2,500
vertex positions a frame.

## 6. Open

**Owner's comparison, 2026-10-10.** The owner watched Doc Mitchell's lines in
`nvforever --play --new-game` with sound and reported the lip sync "identical to the base
games" [observed: owner, by eye and ear against the original game; Doc Mitchell's lines
only]. For those lines this supports the 30 frames per second timing, the start-frame
offset and the mouth shapes as applied. It is not a frame-by-frame measurement, and it
says nothing about the items below that are not shown at all (head and eye movement,
emotions) or about other heads (female, old, child, ghoul).

- The header's first and third fields and the "+ 16" (§2.1).
- Whether the game mixes frames, what it does with frames before the sound and with a
  `.lip` longer or shorter than its sound (§2.4); an in-game capture cannot read morph
  weights directly, but a frame-by-frame video of one line beside its sound would show
  the offset.
- The vowel channels' names and the modifier names other than the blinks (§3.2): by
  position only.
- The unit of the look and head channels (§3.3); head turns while talking are therefore
  not shown, and neither are the eye movements that go with them.
- Whether weights add up linearly, whether they are capped, and whether a statistical
  morph moves a vertex towards its target or by a displacement (§4.3).
- **Emotions.** A response carries an emotion type and value (INFO `TRDT`
  [documented: xEdit]) and the heads have expression and mood morphs (§4.2); how the
  game blends them with the lip weights is not established, and nothing applies them.
  Blinking and eye movement outside speech, and head tracking, are separate behaviours,
  also not done.
- The 588 sounds without a `.lip` (the face stays at rest), and the `.lip` files of
  voices without a human head (robots, Mr. House's screen): what the game does with
  them is not known.

## 7. Tools

- `fnvtool voice lip <data> <plugins…> --path <archive path>` or `--info <FormID>
  [--response <n>] [--speaker <id>]` (`fnv.voice.lip/1`): one file's tracks — per
  channel its name, range, first and last frame and all values — with the start frame,
  the sound's length and whether packing the decoded file again reproduces it.
- `fnvtool voice lip-file <files…>`: the same for loose `.lip` files.
- `fnvtool voice lip-coverage <data> <plugins…> [--lengths]`
  (`fnv.voice.lip_coverage/1`): the counts of §2 and §3; `--lengths` adds the comparison
  with the sounds (§2.4, check 2).
- `fnvtool actor tri <data> --nif-schema <dir> [--path <file>]` (`fnv.actor.tri/1`): the
  `.tri` files against their models (§4); with `--path`, each morph's scale and reach.

## 8. Checks (2026-10-10)

Windowed runs unattended with `--no-voice` (the animation runs on the line's clock
either way), after merging main up to #123; each exit 0 with "run finished".
Screenshots are kept privately, outside the repository.

- **The intro with real conversations** (docs/design/dialogue-play.md §5, 360 s): VCG01
  completed at stage 200; 52 `voice: playing` lines (Doc Mitchell's 23 SayTo lines and
  the 29 responses of the two conversations) and 52 `lip: animating` lines, none
  skipped, none cut.
- **Doc Mitchell's first line in close-up** ("You're awake. How about that.", 34 shots
  0.1 s apart, each timed against the `lip: animating` log line): mouth a little open
  0.10 s in, open at 0.32 s ("You're"), rounded at 0.48 s (the w of "awake"), teeth
  showing at 0.78 s, closed from 1.0 to 1.3 s (the pause between the sentences), open
  at 1.78 s ("How"), lips pressed together at 2.02 s (the b of "about") with the eyes
  half shut by a blink the file has there, open again at 2.31 s, teeth at 2.58 s (the
  last t), at rest at 3.35 s. The moustache follows the lip; the face keeps its FaceGen
  shape; no vertex strays.
- **A conversation** (`--cell Goodsprings`, `NVF_PLAY_TEST_TALK=12:EasyPeteRef=…`, the
  old male head with a full beard): every response animated (greeting, two answers,
  goodbye); with a `skip` step the answer was cut 0.38 s in (`lip: cut …`), and the
  shots 0.5 s to 1.7 s after the cut show the mouth closed while the choices are on
  screen; the goodbye line then animates. With `--dialogue-pause original` the log
  shows the same animation and cut (the mouth runs on the real-time clock, like the
  voice); it was not photographed, because the screenshot aid's clock stands still in
  that mode.
- **Single morphs held** (`NVF_LIP_TEST_HOLD`, Sunny Smiles, the female head): at rest
  the mouth is closed; `BigAah=1` opens it wide, tongue and dark mouth behind the
  lips; `Ee=1` spreads the lips over the teeth; `OohQ=1` purses them;
  `BlinkLeft=1,BlinkRight=1` closes both lids over the eyes; `BrowUpLeft=1,BrowUpRight=1`
  raises the brows. No gap opens between head, mouth and teeth.
- `--no-lip`: no `lip:` line but the notice that it is off; the voice lines as before.
- Goodsprings `--play` 40 s and the moving-camera smoke test (214 cells): exit 0.
- Not checked: sync by ear (the runs were silent; the owner's to judge), child and ghoul
  heads on screen (their `.tri` files parse and fit their models, §4.3), a speaker
  whose cell unloads mid-line.
