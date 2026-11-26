# Observer checks, one file each

New questions the original game can settle go here, one file per check (#3674). A new file
does not conflict with another pull request's new file, and no number has to be reserved.
The numbered checks from before are in [observer-checks.md](../observer-checks.md); its
introduction has the tools, the saves and the observer's limits, for these checks too.

**Adding a check.** Name the file by the issue number and a few words, lower case with
hyphens: `3323-suitcase-on-a-bed.md`. A second check for the same issue gets its own words.
Nothing else lists the checks, so there is no index to edit. In the file:

- a heading: the question;
- `Priority:` P1 (changes many fights or the whole run), P2 (a rule met often) or P3
  (smaller or rarer);
- `Needs:` what the run takes: a save (which), unattended or a person at the keyboard,
  screenshots or video, an add-on;
- `Spec:` the issue and the spec note the answer goes into;
- what NVForever does now, with its evidence tag, then the steps in the original and what
  to report.

[3323-suitcase-on-a-bed.md](3323-suitcase-on-a-bed.md) is an example.

**Citing a check.** By its name without `.md`: "observer check 3323-suitcase-on-a-bed".

**A partial answer** stays in the file, under what was asked, with its date, the capture
job and the evidence tag.

**Settled.** Write the answer into the spec note, fix the engine if needed, take out the
mentions of the check (`git grep <name>`) and delete the file.

**By priority** (from the repository's root, `P1` for the first):

```
git grep -l "^Priority: P1" -- docs/handoff/observer-checks
```
