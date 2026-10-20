# SetActorsAI 0 on a walking actor

What an actor does while its AI is off (SetActorsAI 0, or for everyone the console's
ToggleActorsAI), and how NVForever carries it out. Code: `fnv_ai::carry` (`Ai::coast`,
`Mover::coast`), the engine's `residents.rs` (`PhysMover::coast`). Smoke scenario
`set-actors-ai-walk`.

## 1. What the original does

- **The documentation.** SetActorsAI 0 stops the actor from processing "packages,
  detection, Havok or animation" [documented: GECK wiki SetActorsAI, local export in
  `C:\fnv-private\geckwiki`].
- **What was seen** [observed: owner-decision-captures.md §7, job `od_05_set_actors_ai`,
  2026-10-19]. GoodspringsSettler01Ref was walking back to his place (TRAVEL procedure,
  IsMoving 1) when his AI went off, 30 loops into the walk:
  - he kept moving in a straight line on the heading he had (189.65°);
  - he covered about 1,100 units in 30 samples, past his goal, and did not stop there;
  - his package, procedure and heading stayed as they were;
  - his height followed the ground;
  - GetDetected player stayed 1;
  - after SetActorsAI 1 he turned (heading 23°, then 15°) and walked back.
- **Not seen.** Whether he played the walk clip (the job read GetAnimAction, but §7 does
  not report it), what happens at a wall or another actor, whether a standing actor
  moves, and how long the walk goes on past 30 samples.

So the documentation's "Havok or animation" does not stop the movement already under way.
Package choice, steering and turning do stop.

## 2. What NVForever does

- **AI off, standing.** An actor that stands, or turns on the spot, stays as it is. Its
  package, goal, path and heading are kept. This is the behaviour from before this change.
- **AI off, walking.** An actor that is on its way and moved in its last update keeps
  walking straight ahead. It goes along its heading at the speed it was walking or running
  at, for as long as the AI is off [observed: the settler, §1]. Its package, path and
  heading do not change. Each update tells the game where it is (GetPos and the rest).
- **How it moves.** The engine's character controller moves it, the same one that moves
  walks near the player (`PhysMover`). So it follows the ground [observed: the height
  followed the ground] and collides with walls, other actors and the player. Actors block
  and push it as they do any walking character (`nvf_play::blocking`).
- **Choices** (each a [hypothesis]; the capture did not show them):
  - **At a wall or an obstacle** it keeps walking into it and gets nowhere. It does not
    steer round, use the stuck rule, jump or turn: those are the AI's work.
  - **Its animation.** It is drawn walking while it moves and standing while it is blocked,
    because the engine picks the clip from the drawn movement of an actor on its way. The
    clip it walks with is the one it had.
  - **Where there is no character movement** it stands. That is away from the player
    (collision not loaded), in another space, without `--play`, headless (`fnvtool`, hours
    passed in the Wait menu), or after it falls out of the world. Once stopped, it stays
    stopped until the AI is back on.
  - **Doors.** It does not use doors (it does not steer), and a load door it walks into is
    a wall.
- **AI back on.** Its walk goes on from where it got to, along the path it had. It steers
  to the path's next corner, so one that walked past turns and walks back [observed: the
  settler turned and walked back]. The run-time FOLLOW package the original then showed
  for a few samples (FF001317) is not reproduced. Its cause is not known.
- **The log.** `actor <id> AI off: walks on at heading H deg, S units/s, from (x, y, z)`
  when the walk on starts. `actor <id> AI off walk ended (AI back on | stopped): N units
  straight from … to …` when it ends.
