# Does a sound with the 2D flag fall with distance, and does 2D Radius bring the radii back?

Priority: P3
Needs: a person listening; a save at Lonesome Road's silo launch for that part, a saloon or casino for the last
Spec: #3444, [audio.md](../../spec-notes/audio.md) §15.6

Ours: 2D alone is as loud from anywhere [documented: GECK wiki Sound], 2D with 2D Radius
goes by its radii [hypothesis]. Other actors' sounds keep to the record's distances
[design].

1. From a reference's script or the console (`prid` a persistent reference, then
   `PlaySound3D`), once next to it and once about 3,000 units away: `DRSVegasDoorOpen`
   (2D, 175 to 1,100 units) and `NVDLC04QSTMissileLaunchPt1` (2D and 2D Radius; also once
   beyond its 25,500 units). Say which are heard far off and whether as loud.
2. At Lonesome Road's silo launch: whether the lift-off (about 14 s after the launch
   begins) is heard in full, and whether its sound moves or fades as the missile flies
   away.
3. In a saloon, from across the room and from farther off (the player at one end of a
   casino): whether an actor getting off a chair to its left is heard (`NPCDocMStandUp`,
   2D, 265 to 1,600 units), and whether as loud as next to it.
