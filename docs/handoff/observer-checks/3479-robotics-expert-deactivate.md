# What does the original show on a robot Robotics Expert can shut down, and does a shut-down robot still detect the player?

Priority: P2
Needs: the observer with screenshots; no particular save (the robot is placed)
Spec: #3479, [perk-activation.md](../../spec-notes/perk-activation.md) §2

Ours [hypothesis]: nothing on the crosshair of a hostile robot (a name on a calm one), E
opens a box with "Deactivate" and a button that closes it, and an unconscious actor detects
no one.

In the original: `player.addperk 00031DC2`, crouch behind a placed protectron (00167ED5) it
does not detect (GetDetected 0), and take a screenshot of the crosshair; then press E and
take one of the box. Say which buttons it has, and whether "Deactivate" runs at once with
no box. After it, stand in front of the robot in daylight and read `GetUnconscious`,
`GetDetected player` and the sneak word for 10 s; read the same on an unharmed robot beside
it. Say also what E shows on the shut-down robot, and whether standing, or detected, E on
the robot does anything.
