# Co-op social layer

The things players do with each other in a co-op session, on top of the network core
([coop-net-core](coop-net-core.md)) and world sync. Nothing here runs outside a session,
except the emote wheel, which plays on your own body. Single player is unchanged. Every
choice below is NVForever's own [design] unless a line says otherwise.

## Actions: how the games talk

- An **action** is a short message one player's game sends to the others: an invitation, a
  duel's signal, a trade's offer, a ping, the NPC a player is talking to. It rides the
  reliable stream as one item of kind 0x40, `nvf_coop::action::ACTION`. So every action
  arrives once and in order, and needs no new network message.
- Its layout, little-endian:
  - the kind byte;
  - a `game` byte naming the blob's layout;
  - the sender (255 from a guest);
  - a `table` (u32): whom or what the action is for;
  - the blob, at most 1024 bytes.
- **A guest's action** goes to the host. The host's guard checks it first (below), then the
  host stamps the sender's id on it. The host's game takes it and passes it on to every
  other guest (nvforever `coop/social.rs`). The sender byte a guest writes is never read.
- **The host's own actions** go to every guest. Every game gets every action and takes the
  ones for it (`nvf_play::coop_tables::Tables`).

### The host's checks (guard rules)

`nvf_coop::action::Actions` is a guard `Rule` for the item kind `ACTION`. In order, it:

1. parses the item;
2. requires a sender the host knows;
3. requires a `game` this build registered;
4. applies a token bucket per guest and per game;
5. runs the game's own check (nvforever `coop/social.rs`, `host_rules`).

A refused action is dropped and counted (`Session::guard_stats`). A guest's state is checked too: its emote is one of this build's catalog, in a phase there is (`coop/social.rs`, `EmoteBounds`). The game checks:

| game | rate (burst, per second) | check |
| --- | --- | --- |
| 7, the wheel's messages | 24, 12 | The blob reads as a wheel message. Its table is another player's id, not the sender's. An invitation carries the sender's own nonce (its id in the top 4 bits), at most one a second. Every other message names a duel or trade between the sender and that player that the host saw begin (it keeps at most two per inviter, the host's own included, and a new one forgets only that inviter's oldest); a thrown body's names one the sender is in. A duel's countdown is 2 to 7 s, its winner one of its two players or nobody, a thrown body's speed within 1000 units/s on each axis. A trade's items are ones a player can carry; its counts at least 1, its prices 1 to 99,999 caps (0 declines), and a count times its price at most 2,147,483,647. |
| 10, a ping | 4, 1 | The blob reads as a ping, for everyone. Its place is a cell or worldspace of the host's load order. Its point is within 30000 units of where the guest last stood. |
| 40, the NPC talked to | 4, 2 | Table 0, one reference: 0 (none), or a placed actor of the host's load order within 20000 units of where the guest last stood. |

## The interaction wheel (`nvf_play::coop_social`)

- Looking at another player in reach shows **Interact**. E opens a wheel in the weapon
  wheel's style with two slices: **Duel** on the left and **Trade** on the right.
- Picking a slice asks the other player. They answer with Y or N; Enter and Esc work too,
  and on a controller the buttons are Y and X. The prompt names the sender. An invitation
  lapses unanswered, and a busy player declines it without being asked.
- **Duel**:
  - Pistols only, and only with room: outdoors, with an open line around the challenger.
  - Both turn their backs a few steps apart, and a hidden countdown of 2 to 7 s holds both
    still.
  - An orange sweep (VATS mode's look) is the signal to draw. Each game judges its own shot
    against the other's body, and the challenger's game says who landed first.
  - No damage is done: the host drops hits on the opponent while the duel lasts. The loser is
    knocked down and thrown back in its own game; the others see it lying through its ordinary state, not thrown the same.
  - A duel ends when either player goes. There is no stake (that is the minigames' piece).
- **Trade**:
  - Each player browses what the other carries and asks for items, on the barter menu's
    screen.
  - The owner prices each item asked for, and the asker buys for caps.
  - The caps are held until the owner answers, and given back if the owner cannot sell.

## One conversation per NPC (`nvf_play::coop_talk`)

- Each game tells the others which actor its player is talking to (game 40), again every
  few seconds while the talk lasts.
- An NPC in another player's conversation offers no second one: E says "Wait for <name>"
  (`activate.rs`, `Action::HostOnly`).
- While the talk lasts only the players' hits hurt it (`fnv_game::combat::talk_guard`).
  With the host's setting **Players can kill NPCs in conversation** off, nobody's do.

## Name tags and pings (`nvf_play::world_marks`, `safe_zone`)

- Name tags show the other players' names over their heads, through walls. They fade between
  two distances (settings; both distances are lists of seven).
- A ping (Ping, unbound until the player binds it) marks what is under the crosshair for
  every player:
  - a diamond, the distance and the name;
  - fainter away from the crosshair, and kept at the screen's edge when off screen;
  - heard from its direction.
- Marks stay off the HUD's own pieces (`safe_zone`).

## Emotes (`nvf_play::emotes`, nvforever `emote_play.rs`)

- Animations of the base game's own idles, played on the player's body for every player to
  see. The player's state carries the emote (`PlayerState::emote`).
- Holding Emote (unbound until the player binds it) opens a wheel of eight slots. The slots
  start as a default eight and are kept, one emote key per slot, in `emote-slots.txt` in
  NVForever's home. A screen to choose them is not part of this piece.
- A DLC's clips play only with that DLC loaded. No mod's clips are read.

## Settings

Gameplay > Co-op: `name-tags`, `name-tag-near`, `name-tag-far`, `kill-in-conversation`. All
four are greyed while the Co-op switch is off. Their entries are in
[settings-reference](../settings-reference.md).
