# Poker (Texas Hold'em at a casino table)

The original game has no poker: its casino games are blackjack, roulette, slots and
Caravan. This is NVForever's own minigame, an addition, not a parity target (owner,
2026-10-16: "design me a poker minigame … using only assets that we have access to").
What follows is design unless tagged; the game's files it draws with are listed in §5
with what was looked at.

Nothing of the game is shipped: every picture is read from the player's own archives when
the table opens, and the table's shape is made by the code.

## 1. Rules (`fnv_game::poker`)

No-limit Texas Hold'em for the player and four opponents.

- **Deck**: 52 cards, shuffled by the table's own generator (ChaCha8, seeded when the
  table opens; a test aid can fix the seed).
- **A hand**: the button moves to the next seat still in the game; the small and big
  blinds are posted by the next two seats (heads-up: the button posts the small blind and
  acts first before the flop). Two cards each, then betting; the flop (three cards),
  turn and river, each followed by betting; then the showdown.
- **Betting**: fold, check, call, raise (to a total; at least the last raise's size over
  the current bet, unless it is all the seat's chips) and all-in. A round ends when every
  seat that can still act has acted since the last full raise and matched the bet. A short
  all-in raise does not reset who has acted (those behind must still match it).
- **All-in**: when at most one seat can still bet and nobody has a bet to match, the
  remaining cards are dealt out to the showdown.
- **Pots**: each contribution level makes a pot that the seats who reached it and did not
  fold can win (side pots); a bet nobody could match comes back to its seat as its own
  pot. The best five of seven cards wins; a tie splits the pot, odd chips to the first
  winner after the button.
- **Hands**: high card, pair, two pair, three of a kind, straight (ace low or high),
  flush, full house, four of a kind, straight flush; kickers decide within a kind.
- **Seats out**: a seat with no chips at the end of a hand is out. The table ends when the
  player is out or every opponent is.

## 2. Opponents

Each opponent has a style: tightness, aggression and a bluff rate, drawn when the table
opens. On its turn it estimates its share of the pot by dealing the unseen cards at
random a few hundred times (Monte Carlo, against the opponents still in), compares that
with the price of calling, and folds, calls or raises; raises are sized from a half to a
whole pot, rounded to the small blind. Bluffs are rare small bets or raises with a weak
hand. They see nothing they could not see at a real table.

## 3. Money (nvf-play `poker.rs`)

- **Sitting down** takes no caps at once: the player's stack is the buy-in, the smaller
  of 200 and the caps carried; with fewer than 100 caps the table refuses ("You need at
  least 100 caps to play.", a corner message).
- **Leaving** settles the difference: the caps the stack ended above the buy-in are added,
  those below are taken (`Caps001`, MISC 0x0000000F [observed: `ui_sounds.rs`]). Leaving
  in the middle of a hand folds it: chips already in the pot are lost.
- Opponents sit with 150 to 300 chips each.
- The blinds are 5 and 10.

## 4. The table on the screen

It is a `PlayState::Menu`, as lockpicking is: the world and the game clock stand still.

- **The view** (nvforever `poker_view.rs`): a camera of its own on render layer 11 draws
  the table into a picture shown behind the menu's text, from the player's seat, looking
  down at about 33°. The table is an oval (two half circles of radius 48 joined by
  straight sides 80 long; game units), its felt, padded rail and the floor under it made
  by the code and textured with the game's pictures (§5). One spot light above the table,
  a little ambient light; the room beyond is dark.
- **Cards** lie on the felt: the player's two face up in front of the camera, the
  opponents' face down at their seats, the board in the middle (an outline where a card
  is still to come). At the showdown the hands still in are turned over. Dealt cards
  slide from the dealer's place to theirs.
- **Chips**: each seat's stack in front of it, its bet nearer the middle, the pot in the
  middle; amounts are split into the game's chip colours (white 1, red 5, blue 10,
  green 25, black 100, yellow 500), each colour taking about three fifths of what is
  left so a stack shows several colours, at most 20 chips a stack. The dealer button is the
  Platinum Chip's picture on a disc.
- **Text** in the HUD's colour with the game's fonts (as the lockpicking menu): the
  casino and the game at the top left, the blinds and the hand's number under it; the pot
  and the street at the top right; each opponent's name, chips and last action near its
  seat; the player's chips, what it costs to call and the player's best hand at the
  bottom left; the choices at the bottom right ("E) CALL 40", "R) RAISE TO 60",
  "F) FOLD", "TAB) LEAVE"). The result of a hand comes in the middle.
- **Sound** (nvf-play `poker/sound.rs`): the casino's floor as the game sounds it — its
  background loop and, now and then, a murmur, a laugh, the card tables and the slot
  machines (each table's records are listed there) — and its music, the casino's own radio
  station played at the listener as the Pip-Boy plays one: Tops Radio, Gomorrah Lounge
  Music, Ultra Luxe Radio [observed: the stations the casinos' loudspeakers play]; the
  Lucky 38's empty floor has none, the plain deck plays the Tops'. The world's music,
  background loops and radios are quiet while the table is open [design].
- **Keys**: E checks or calls, R raises to the amount shown, the mouse wheel or Up/Down
  change that amount by the big blind (Shift: by 5 big blinds), A goes all-in, F folds,
  Tab or Esc leaves. The choices can be clicked. Opponents take 0.6 to 1.2 seconds a
  choice; the next hand is dealt 3 seconds after the last ends (Space at once).

## 5. The game's files used

All from the base archives (`Fallout - Textures.bsa`, `Fallout - Textures2.bsa`)
[observed: `fnvtool bsa list`, the pictures decoded and looked at, 2026-10-16]:

| What | File | Seen |
|---|---|---|
| Felt | `textures\terminals\nv_blackjack\nv_blackjack-table02.dds` (Ultra-Luxe: `…-table02u.dds`) | 1024², plain worn green felt (black for `u`), no markings |
| Rail | `textures\terminals\nv_blackjack\nv_blackjack-table01.dds` | 2048 × 512: a metal trim at the top, dark padded leather under it, a chip tray at the right end |
| Casino name on the felt | `textures\terminals\nv_blackjack\nv_blackjack-casino-logo01/02/03.dds` | 512 × 256 on black: The Tops, Gomorrah, Ultra-Luxe |
| Cards | `textures\terminals\nv_playingcards\deck<x>\card<x>_<suit><rank>.dds`, `card<x>_back.dds` | 128 × 256 each; suits `c d h s`, ranks `01` (ace) to `13` (king), `j01`/`j02` jokers; decks `t` (Tops), `g` (Gomorrah), `u` (Ultra-Luxe), `l` (Lucky 38), `b` (a plain red back) |
| Chips | `textures\terminals\nv_chips\nv_minigame-chips.dds` | 512 × 256: six chip faces in a row at the top (white with blue, red, blue, green, black, yellow), each one's edge stripes under it |
| Dealer button | `textures\clutter\platinumchip\platinumchip.dds` | the Platinum Chip |
| Floor | `textures\clutter\rugs\nvtopscasinorug_d.dds` | the Tops' carpet |
| Fonts | `textures\fonts\glow_monofonto_large.fnt`, `baked-in_monofonto_large.fnt` | docs/spec-notes/fonts.md |

Which part of each picture is used (the chips' faces and stripes, the rail's bands) was
read off the pictures by eye [observed]; the code names the rectangles.

## 6. Opening the table

- The console command `poker [casino]` (`t`, `g`, `u`, `l`, `b`; default `t`) opens it
  (NVForever's own word, as `advlevel`).
- Test aid `NVF_PLAY_TEST_POKER=<seconds>:<step>[,…]`: `open[:<casino>]`, `seed:<n>`
  (before `open`), `caps:<n>` (the player's caps set before `open`), `check`, `call`,
  `raise:<to>`, `allin`, `fold`, `auto` (the player's seat is played by the opponents'
  rules from then on), `next` (deal the next hand at once), `exit`. At exit, when the
  table was opened: "poker summary: …".
- Not built: opening it from a table in a casino (no placed table in the game is a poker
  table), a controller's buttons, the sounds of the player's own moves (cards dealt, chips
  bet), chips of the casino instead of caps.
