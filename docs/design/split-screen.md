# Split screen: two players on one PC

Split screen puts a second player on the same PC. Player 2 is a co-op guest like any other
(`docs/design/coop-net-core.md`, world sync): in the host's world it is a stand-in actor, and
only its camera, controls, HUD and menus run in a game of its own. It is **off by default**,
needs the Co-op switch (Gameplay, Co-op), and a game that never turns it on is unchanged
[design].

## Two processes

- **Player 1's game** is the one the player started. Add Player 2 (the Co-op screen), the
  main menu's Split Screen entry (loads the newest save with split screen on) or `--split`
  turns it on. Once in the world it opens a lobby for this PC only, writes its session
  files and starts player 2's game.
- **Player 2's game** is this program again (`--split-p2 <session folder>`), started by
  player 1's, in a home of its own, joined to that lobby under the name Player 2. Its
  window is borderless on its half and never takes the focus. It continues as the
  current character, which its player loads on the Load Game page of its black main menu
  (picking among premade characters is a later part).

Why two processes: the world, its scripts and its memory are the host's. A second game
in the same process would need a second world; as a guest, player 2's game follows the
host's world as any guest's does and never runs it.

## The lobby

`nvf_coop::seam::loopback::Udp` is a UDP socket bound to 127.0.0.1 that refuses to send
anywhere else. The host's session runs on it with a random secret
(`nvf_coop::seam::SharedSecret`); the secret and the port go to player 2's game in
`p1.txt` (`join=127.0.0.1:<port>#<secret>`). Nothing is opened to the network: no router
port, no firewall prompt. Player 2's join, state and every later message go through the
host's checks (`nvf_coop::guard`, `crate::coop::checks`) like any guest's. When player 1's
game restarts for a load it opens the lobby again with the same secret; player 2's game
sees the new port in `p1.txt` and follows it (`Session::move_host`).

The lobby ends with split screen. This part opens no other lobby: a game that is a guest in
another player's lobby cannot start split screen.

## Input

- This PC's keyboard and mouse are player 1's. Player 2's game drops them before anything
  reads them (`bevy::winit::hardware_input`, the vendored bevy_winit; off unless asked).
- Controllers are given out when split screen starts: the one player 1 last used stays
  player 1's (or the first, for a player 1 who plays on a controller), player 2 gets the
  next. Player 1's game relays player 2's buttons and sticks over loopback UDP, and player
  2's game plays them as a controller of its own. What arrives is checked: finite values in
  range, the longest name, nothing left over, from player 1's port only, at a bounded rate
  (`PadState::decode`, `PadGuard`).

## Windows

Player 1's window takes the top half (the left half side by side; Display, Screen, Split
screen layout), player 2's the other, owned by player 1's so they move and minimise
together. A black backdrop under both covers the monitor while split screen is on. The
taskbar is lowered by marking player 1's window full screen for it.

## What is drawn

- The HUD and menus are scaled for the half (`nvf_play::interface::split_ui_factors`); the
  HUD pieces are made again when the scale changes (`HudRebuild`).
- The field of view of a wide half keeps the horizontal angle the setting has at 16:9
  (`split::view`).
- Sound: while player 2 is in player 1's place, player 1's game plays the world for both
  (`SecondListener`) and player 2's plays its interface and its own conversation; apart,
  each plays its own world. The other half is lowered during a conversation.
- Draw distances drop to 75% and the sun's shadows to 70%, with the shadow maps one size
  smaller, while split screen is on (`draw_distance::split_profile`); the settings keep the
  player's own values.

## Not in this part

- Player 2 joining a lobby of another player through player 1's game.
- The XP catch-up and progress kept with the host save.
- Premade characters to pick from (player 2 continues as the current character).
- A cover over the windows while a load restarts: the backdrop's black shows meanwhile.
- A memory budget for player 2's game.
