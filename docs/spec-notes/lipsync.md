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
- Look and head channels: small numbers (to 0.18). They are equal and opposite to each
  other (§3.1), so the file stores them in one unit. What the game turns by one unit is
  not established; the measurements and the choices made for now:

**The eye morphs** [observed, 2026-10-18, `fnvtool actor tri --path <eye>.tri`, its
`best_rotation`: the rotation that best carries the moved vertices onto their targets,
least squares about their centroid]. Each Look morph of an eye `.tri` moves all 49
vertices of the eye, and the move is a turn of the whole eye (what is left over after
the best turn is 0.03 to 0.10 units, on an eye about 1 unit across):

| Morph at weight 1 | Male eyes | Female eyes | Child eye (left) | Axis, in the head's frame (x right, y forward, z up) |
|---|---|---|---|---|
| LookDown | 21.9°, 21.4° | 21.9°, 21.4° | 19.2° | about −x: the front of the eye goes down |
| LookUp | 13.6°, 13.9° | 13.6°, 13.9° | 12.0° | about +x: up |
| LookLeft | 21.8°, 23.6° | 24.6°, 23.4° | 22.7° | about +z (tipped 15–30°): to the actor's left |
| LookRight | 22.0°, 23.9° | 22.7°, 24.8° | 19.8° | about −z: to the actor's right |

(left eye, right eye). The head `.tri` LookDown and LookUp move 39 and 83 vertices
(the lids and the skin round the eyes; LookLeft and LookRight move none, §4.2).

**What the channels' values come to.** A Look channel is a morph weight like any other
(its name is a morph's label in the eye and head `.tri` files) [hypothesis, the natural
reading]: the files' Look values (to 0.18) turn an eye by at most 4°. In the lines
looked at (25 of Doc Mitchell's, every twelfth of his 370 files, scratch) the head
channels swing by 0.03 to 0.07 within half a second and reach 0.04 to 0.09.

- **Directions.** HeadPitch: **positive tips the head up** [observed 2026-10-22, below;
  this replaces the first guess, "nods down", taken from the eyes turning against the
  head]; with HeadPitch = LookUp − LookDown in the files the eyes then turn with the head
  in pitch, not against it. HeadYaw = LookLeft −
  LookRight: a positive HeadYaw goes with eyes turning left: **the head turns to the
  actor's right**. HeadRoll has no partner; positive is taken as tipping the top of
  the head to the actor's right.
- **Size of a unit.** Two readings fit the files:
  (a) **one unit = one radian** — a value of 0.05 is a 3° nod, the largest (0.17) 10°;
  the eyes then make up only part of the turn (0.05 of LookUp is 0.7°), so the gaze moves
  with the head a little;
  (b) one unit = the eye's own turn per unit (about 22°): the eyes would make up the
  whole turn, but the head would move by 1° in a typical swing and at most 4°, hardly
  to be seen. The mirroring cannot be exact under (b) anyway, because the eye turns
  22° per unit down and 14° up while the head channel is one number.
  **The runtime takes (a)** [hypothesis]: the owner sees the original's heads move
  plainly in conversation (2026-10-18), which (b) would not give. The game also takes
  the face modifiers from the console (`MFG`, §3.2) on a 0 to 100 scale, and what
  HeadPitch at 100 does there settles the question: **capture job
  `tools/capture/jobs/lip_head_units.steps.json`** (queued 2026-10-18, not run yet:
  Doc Mitchell in profile and from the front at rest, with HeadPitch at 50 and 100,
  HeadRoll, HeadYaw, LookDown and LookLeft at 100, and the Anger expression at 100;
  the `MFG` syntax `mfg <modifier|expression|phoneme> <index> <0–100>` is taken from
  the search summaries of §3.2 [hypothesis] and is checked by the run). The constant is
  `lips::HEAD_RADIANS_PER_UNIT`.
  **Run 2026-10-22, unsettled** [observed]: the job runs unattended (save `nvfdoc`, plugin
  v7) and puts Doc in front of a first-person camera, but in an automated run the console
  has no selected reference: `prid 00104C0F`, `prid 104C0F` and `prid DocMitchellREF` all
  left a following `SetAngle` without effect, and neither `mfg expression 0 100` nor
  `mfg modifier 14 100` changed his face, so every picture shows his resting face from the
  front. Whether `mfg` itself works is therefore still open. What did work: placing Doc
  with a script step (`BuildRef` of FormID 00104C0F, decimal 1068047; `MoveTo player`, then
  `SetPos`). Turning him with a script step between pictures crashed the game (once).
  **By hand, 2026-10-22** [observed, owner's six screenshots in dialogue with Doc Mitchell,
  `prid 104C0F` then `mfg …` typed in the console; which picture is which was read from
  the pictures, the owner's file names did not come through]: `mfg modifier 14 100` tips
  the head **up** by about 60°, `14 50` by about 30° (eyeballed from the face's tilt);
  `mfg modifier 16 100` turns it about 70–80° **to the actor's right**;
  `mfg expression 0 100` closes the left eye (a wink), so expression 0 is not Anger. So
  100 on the console turns the head by about one radian, which is reading (a) if the
  console's 100 is a `.lip` value of 1 [hypothesis]. Roll and the Look modifiers were not
  tried.

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

### 4.4 Emotions

- Every response has an emotion type and value (INFO `TRDT`) [documented: xEdit, as
  `schemas/fnv/gameplay.json`]; the type codes are 0 Neutral, 1 Anger, 2 Disgust,
  3 Fear, 4 Sad, 5 Happy, 6 Surprise, 7 Pained [documented: GECK wiki
  GetDialogueEmotion, the owner's export].
- In `FalloutNV.esm` (29,674 responses) [observed, 2026-10-18, `fnvtool dialogue lines
  --all`]: 22,684 Neutral, 2,052 Anger, 1,928 Happy, 753 Surprise, 707 Sad, 643
  Disgust, 553 Fear, 354 Pained. Values run from 0 to 100; 50 is by far the most
  common (22,637 of the Neutral ones; about 40 % of the others), then 10, 5, 25, 30;
  100 appears in every type.
- The heads carry a morph for each of the seven types by the type's name (`Anger`,
  `Disgust`, `Fear`, `Sad`, `Happy`, `Surprise`, `Pained`), and the male mouth carries
  `Anger` and `Surprise` [observed: `fnvtool actor tri`, §4.2; `Pained` is among what
  §4.2 counts as moods]. On the male head at full weight: Disgust moves vertices by up
  to 0.57 units, Anger 0.42, Pained 0.38, Happy 0.30, Fear 0.28, Surprise 0.26, Sad
  0.22; the mouth's Anger 1.38 and Surprise 0.93.
- How the game turns a response's emotion into a face is **not established**: which
  morph (by name is the plain reading), at what weight, how fast it comes and goes, and
  how it combines with the `.lip` weights.

## 5. What the runtime does (`nvforever --play`)

`engine/nvforever/src/lips.rs`. While `nvf_play::voice` reports a response being said
(with or without sound: `--no-voice` keeps the timing), and its sound has a `.lip`, and
the speaker is a drawn NPC with head parts:

- each head part with a `.tri` of its vertex count (head, mouth, lower teeth, tongue,
  beards, eyebrows; eyes and upper teeth have nothing to apply) is drawn from a mesh of
  its own; each frame its vertices are: the model's, FaceGen-shaped as the actor's face
  is, plus Σ weight × displacement over the channels applied;
- **channels applied to the morphs**: the 16 phonemes, BlinkLeft/Right, the six brow
  channels, the squints and (since 2026-10-18) LookDown/Left/Right/Up — 30 channels; the
  Look channels move the eyes and the lids round them (§3.3); the brow-down, brow-in and
  squint ones are never used by the files;
- **HeadPitch, HeadRoll, HeadYaw** turn the head bone (§5.1);
- the weights for the moment t after the response's sound started are those of frame
  t × 30 − S, the two neighbouring frames mixed linearly [hypothesis]; frames before the
  sound's start are not shown [hypothesis]; after the last frame the face is at rest;
- when the response ends or is cut (another line, the speaker gone) the weights fade to
  rest over 0.15 s [engine choice] and the actor gets its ordinary mesh back;
- normals are not recomputed [engine choice].

One log line per response: `lip: animating <path> (<tracks> tracks, <applied> applied;
<frames> frames, <seconds> s from <start> s) speaker <who>: <shapes> shapes, <morphs>
morphs; head turn up to <degrees>° (pitch <p>, roll <r>, yaw <y>); emotion <type>
<value>` (the emotion part only for a response that has one, `emotion <type> <value>
(no morph)` when its face has no morph for it), or `lip: skipped <path>: <reason>` (no
`.lip`, or the speaker has no face rig: a creature, a robot, a talking activator, or an
actor not drawn), and `lip: cut <path> …` when a response is skipped or interrupted
before its sound is over. At the end of a run: `lip summary: <n> responses animated,
<n> skipped, <n> cut; head turned on <n> (largest <degrees>°); emotions shown on <n>
(<type> <count>, …)`. `--no-lip` turns all of it off, and the idle face (§5.3) with it.
Test aid: `NVF_LIP_TEST_HOLD=<channel>=<weight>[,...]` makes every NPC hold those
weights (screenshots of single morphs; not a game behaviour); the expression names
(`Anger` … `Pained`) can be held too, and the head channels turn the head.

### 5.1 Head turn

- The head channels are taken as radians (§3.3 reading (a),
  `HEAD_RADIANS_PER_UNIT` = 1) [hypothesis], positive pitch tipping the head up (§3.3,
  observed with `mfg`), positive yaw
  to the actor's right, positive roll tipping to the right [hypothesis, §3.3].
- The turn is applied to the head bone (`Bip01 Head`) in the head's own frame (its bind
  pose taken as facing the actor's +y with z up, as head tracking takes it,
  docs/spec-notes/actors.md §12.1), **on top of whatever the bone holds** after the
  frame's animation, head tracking (§12) and a conversation's look
  (`dialogue_look.rs`) have written it: the head first faces where those put it, then
  nods. The neck bones are not turned [engine choice: the turns are a few degrees].
- The turn is taken off again at the start of the next frame if nothing posed the bone
  since, so head tracking and the conversation's look never see it as animation and
  never wind it up [engine choice; `lips::untouch`].
- It eases out with the rest of the face when the line ends or is cut (§5).

### 5.2 Emotions [engine choice, hypothesis where marked]

- A response's emotion (§4.4) is shown through the head part morph of its type's name
  (`Anger` … `Pained`) [hypothesis: by name]; Neutral shows nothing.
- Its weight is the emotion value ÷ 100 [hypothesis: the value is a percentage, as its
  0–100 range and the console's 0–100 expressions suggest], **times 0.6**
  [engine choice: the expression morphs at full weight distort the mouth the phonemes
  then move; 0.6 keeps a value of 50 at 0.3].
- It comes in over 0.3 s from the start of the response and goes with the face's
  easing when the line ends or is cut; between two responses of one line with the same
  emotion it does not drop [engine choice].
- It is **added** to the `.lip` weights, as the `.lip` channels are added to each other
  (§4.3): the expression sets the face, the phonemes move the mouth over it
  [engine choice].
- A response without a `.lip` shows no emotion either (the face is not moved at all).

### 5.3 The face between lines (blinks and eyes) [design]

NVForever design, not captured: the original's blinks and eye movements outside speech
were not measured. Every drawn living NPC with a face rig within 700 units of the camera
(about 10 m; the face is too small to see further) [design]:

- **blinks**: both lids together, a pulse of 0.2 s (up in 0.07 s, down in 0.13 s, peak
  weight 1), one every 1.5 to 5 s, 3.2 s on average — the length and the mean spacing
  are those of the blinks in the `.lip` files (§3.1: 5 to 7 frames, about one every
  3.2 s) [observed there; design here], the spread is a design choice;
- **eyes**: when head tracking has the actor looking at someone and the head stops short
  of them (the turn limits, the easing), the eyes make up the difference, at the
  measured turn of the Look morphs per unit (§3.3: 22° to the side and down, 14° up),
  at most weight 0.6 (about 13° to the side) [design]; on top of that a small glance
  that changes every 0.6 to 2.5 s (within ±0.06 sideways, ±0.04 up and down, a third of
  the time back to the middle) and moves in 0.05 s [design];
- while the actor says a line the `.lip` file drives blinks and eyes instead, and the
  idle face picks up again when the line has eased out; dead actors, actors whose head
  tracking is switched off (asleep, scenes: `NoHeadLook`) and the test aid's held faces
  are left alone;
- the random draws come from a generator of each actor's own, seeded with its
  reference, so a run repeats and other systems' rolls are not shifted;
- a face that rests (no blink, eyes in the middle) between moves keeps its mesh; the
  actor gets its own mesh back once it is beyond 840 units.

Log: `face idle: <n> faces near the camera` when the count changes by 5 or more, and
at the end `face idle summary: <n> actors had an idle face; <n> blinks, <n> glances,
eyes followed a head-tracking target on <n> of them`. `--no-face-idle` switches only
this part off.

Cost: a speaking actor rewrites about 1,500 to 2,500 vertex positions a frame; an
idle face only while it blinks or its eyes move (weights that changed by less than
0.004 since the last write are not written again).

### 5.4 Morphs other systems ask for [engine choice]

`face_morphs::FaceMorphs` on an actor's root (the entity with `ActorRig`) is a weight
0 to 1 per morph label that another system wants shown; the combat face sets
`CombatAnger` there (`combat_face.rs`). `lips.rs` reads it:

- labels are the expressions (`Anger` … `Pained`), the nine moods of the heads
  (`MoodNeutral`, `MoodAfraid`, `MoodAnnoyed`, `Cocky`, `MoodDrugged`, `MoodPleasant`,
  `MoodAngry`, `MoodSad`, `CombatAnger`; §4.2) and the `.lip` channels; another label
  is named once in a warning and left out;
- the weights are added to the line's (§5) and the idle face's (§5.3), and an actor
  with any is given a moving face while it is within 840 units of the camera, idle face
  on or off; it is let go when the list is empty again;
- **one expression at a time**: while a response with an emotion other than Neutral is
  shown, the expression and mood weights asked for are scaled by 1 − the emotion's share
  (§5.2), so the line's emotion takes over as it comes in and hands back as it eases
  out; a Neutral response leaves them. [The rule is the GECK wiki's note on
  `ModifyFaceGen` that an actor shows one expression at a time, as read through search
  summaries (§3.2); how the game itself mixes a mood with a line's emotion is not
  known.]

Log: `face: <reference> shows asked morphs: <label> <weight>, …` once per actor
(`(no morph)` after a label its face does not carry). Test aid
`NVF_FACE_TEST_MORPHS=<label>=<weight>[,...]`: every drawn actor with a face rig asks
for those morphs (smoke `face-morphs-asked`; not a game behaviour).

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
- The unit of the look and head channels (§3.3): taken as radians for the head and as
  morph weights for the eyes [hypothesis]; capture job `lip_head_units` queued.
- Whether weights add up linearly, whether they are capped, and whether a statistical
  morph moves a vertex towards its target or by a displacement (§4.3).
- **Emotions** (§4.4, §5.2): the morph by name, the value as a percentage, the 0.6
  scale and the timing are guesses; the capture job's Anger picture shows what the
  game's expression at 100 looks like. The ten mood morphs are not used.
- Blinks and eye movement between lines (§5.3) are a design, not the original's.
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

## 9. Checks (2026-10-18): head, eyes, emotions, idle face

Windowed `--play --no-voice` runs by the smoke runner (scenarios `lip-head-turn`,
`lip-emotion`, `face-idle`, `lip-face-held`, `lip-face-rest`, and the tool check
`lip-eye-turns`); all passed.

- Chet's conversation (GSGeneralStore): 4 responses animated, head turns of 1.9° to 4.7°
  on each; the greeting's third response showed Surprise.
- Joe Cobb's angry line to Trudy (saloon): `emotion Anger 50` on his line and Trudy's
  answer; 3 idle faces, 25 blinks and 46 glances in 40 s, the eyes following a
  head-tracking target on all 3.
- The saloon for 30 s: 1 idle face (Sunny Smiles), 9 blinks, 16 glances.
- Held weights (`HeadPitch=0.17, HeadYaw=0.15, LookLeft=0.5, Anger=0.6`, Sunny Smiles
  from 100 units, against the same view at rest; frames kept privately): the head turns
  to her right and dips, the brows draw down, the eyes go to the side; head, neck, hair
  and eyes stay together.
- Not checked: the original's own head nods beside ours (the capture job of §3.3), the
  head turn on a talking actor in close-up frame by frame, female, old, child and ghoul
  heads in close-up.
