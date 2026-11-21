# Slot machines (the casinos' one-armed bandits)

The original's slot machine minigame: a slot machine of the Strip's casinos opens a menu
where the player bets the casino's chips on three reels. Rules: `fnv_game::slots`; what
the casino games share (the casino record, earnings, levels, the ban):
`fnv_game::casino`; the menu: nvf-play `slots.rs` (drawn flat, §6).

Sources: FalloutNV.esm (the CSNO records, the machines' scripts, the SOUN and MESG
records), `menus\slot_machine_menu.xml`, the models in `meshes\terminals\nv_slotmachine\`
and the textures in `textures\terminals\nv_slotmachine\`, the GECK wiki (owner's export).
No screenshot of the original's slot menu was at hand. Nothing here comes from the
executable. Blackjack: blackjack.md; roulette: roulette.md.

## 1. Opening it

- Each casino's slot machine is an activator whose script runs, on activation by the
  player, `ShowSlotMachineMenuParams <casino> 1 <max> 0`, or a message instead while the
  casino's winnings quest has `bFloorManagerIncoming` (the Atomic Wrangler: `banned`)
  set [observed: decompiled scripts `SlotMachineTheTopsScript` 0x00144F31 (The Tops, max
  75), `vGomorrahSlot01Script` 0x00139FE1 (Gomorrah, 60), `SlotMachineWranglerScript`
  0x00161511 (the Atomic Wrangler, 40), `vVikkiVanceSlotScript` 0x0016318E (Vikki &
  Vance, 25)]. Vikki & Vance's script tests Gomorrah's quest and shows no message
  [observed]; NVForever runs the scripts as they are. No slot machine activator names the
  Ultra-Luxe [observed: the ACTI records named `*Slot*`: vTopsSlot01, vGomorrahSlot01,
  vWranglerSlot01/02, vVikkiAndVanceSlot01/02].
- `ShowSlotMachineMenuParams casino [minBet] [maxBet] [maxWinnings]` (sslmp, 0x1252)
  displays the slot machine menu; the player must meet the casino's requirements: its
  chips carried, not banned [documented: GECK wiki ShowSlotMachineMenuParams]. A
  `maxWinnings` of 0 is the casino's own (as roulette.md §1) [hypothesis]. Left out, the
  bets are 1 to 75 [hypothesis: The Tops' script's].
- Refused, a corner message says why (`fnv_game::casino::closed_line`) [hypothesis: the
  words are NVForever's].
- It is a menu: the world and the game clock stand still (`PlayState::Menu`), menu code
  1103 to scripts [hypothesis: the menu's `&SlotMachineMenu;` class number is not in the
  menu files and no shipped script asks for it; NVForever's own, next to roulette's 1101
  and blackjack's 1102].
- Placed machines: seven `vTopsSlot01` in `TOPSCasino` (0x0015CBDC, 0x0016546A …),
  Gomorrah's, the Atomic Wrangler's and Vikki & Vance's [observed: `fnvtool world refs`].

## 2. The casino record (CSNO)

As blackjack.md §2, and [documented: schemas/fnv/objects.json CSNO, from xEdit;
observed: the five casinos]:

| Field | Meaning |
|---|---|
| DATA 8, 7 × u32 | the reel stops: how many of the reel's stops show each symbol, 1 to 6 and W (the seventh). "The "W" or winning symbol is #7. The total number of stops on a slot machine reel must equal 14" [documented: GECK wiki GetCasinoReelStops]. Every casino: 2 2 2 2 2 2 2 [observed] |
| MOD2 | the machine's model: `terminals\NV_SlotMachine\NV_SlotMachine-Minigame.NIF` (The Tops, the Ultra-Luxe), `…-Minigame-Gom.NIF` (Gomorrah), `…-Minigame-Generic.NIF` (Vikki & Vance, the Atomic Wrangler) |
| ICON × 7 | the symbols' pictures, in the order of the reel stops: `NV_SlotMachine-Symbol1`, `2`, `3`, `4`, `6`, `7`, `5.dds` [observed: all five casinos] |

- The pictures [observed: the textures, 256 × 128 each]: Symbol1 a bell, 2 a BAR, 3 a
  seven, 4 a lemon, 5 cherries, 6 grapes, 7 an orange. With the ICON order the reel
  symbols are: 1 bell, 2 BAR, 3 seven, 4 lemon, 5 grapes, 6 orange, W cherries.

## 3. The reels and the payouts

- **A reel**: `nv_slotmachine-reel01.nif` (and `02`, `03`) is a drum of 14 faces,
  `Cylinder06:0` … `:13`, textured in turn Symbol1, SymbolS, Symbol2, SymbolS, … Symbol7,
  SymbolS [observed: the file's shapes]. The engine fills the faces from the record: 14
  stops, each symbol on as many as the record gives it [documented: §2]. In what order
  the stops lie round the drum is not in the data; NVForever deals them out in turn
  (1, 2, 3, 4, 5, 6, W, 1, 2 … while each has stops left) [hypothesis]. The three reels
  are alike [hypothesis].
- **A spin**: each reel stops at one of its 14 stops, each as likely [hypothesis: the
  stops being the weights, as the record's name says]. With 2 of each, each symbol shows
  on a reel 1 time in 7.
- **The payline**: the middle row of the three reels [hypothesis: the payout screen
  shows single rows].
- **The payouts** [observed: the machine model's payout screen, `Payout_Screen` with the
  shapes `Payout_Symbol1` … `Payout_Symbol6` and `Payout_SymbolS` and the numbers mapped
  from `NV_SlotMachine-Minigame05.dds`, "= BET x" then the number, row by row]:

  | Row | Symbols shown | = BET x |
  |---|---|---|
  | 1 | three bells | 10 |
  | 2 | three BARs | 20 |
  | 3 | three sevens | 30 |
  | 4 | three lemons | 40 |
  | 5 | three grapes | 80 |
  | 6 | three oranges | 160 |
  | 7 | one cherry, in the left place | 2 |
  | 8 | two cherries, in the two left places | 5 |
  | 9 | three cherries | 10 |

  The rows' pictures are the shapes' textures (`Payout_Symbol5` uses Symbol6.dds, the
  grapes; `Payout_Symbol6` Symbol7.dds, the orange; `Payout_SymbolS` Symbol5.dds, the
  cherries) [observed]. Read as: three of a symbol on the line pay the bet times its
  number; cherries pay by how many lie on the line from the left reel on (one in the
  left place 2, the two left 5, all three 10) [hypothesis: from the rows' pictures]. "Pay"
  is the chips handed back for the bet, which is taken as the reels spin: a win of
  `= BET x 2` on a bet of 5 gives 10 chips, 5 more than staked [hypothesis].
- With the shipped stops (each symbol 1 in 7 per reel) these pay back about 1.35 chips a
  chip bet [design: arithmetic of the two hypotheses above]; the casinos' max winnings
  end it (§4).

## 4. A spin

- **Betting**: the bet starts at the call's minimum and moves by 1 under 10, 5 under 50,
  25 from there (blackjack.md §5), between the minimum and the smaller of the maximum
  and the chips carried [design, as roulette's]. Bet Max sets the bet to that largest
  bet [hypothesis: it does not spin as well].
- **Spin** takes the bet from the inventory and turns the reels; the left reel stops
  after 1.4 s, the middle after 2.0 s, the right after 2.6 s [design]; then the line is
  paid (§3) and the result shows for 2 s [design].
- **Earnings and bans**: the net of each spin goes to the player's earnings at the
  casino and the winnings level (blackjack.md §4, `fnv_game::casino`); at the max
  winnings the player is banned and the menu closes after the result [hypothesis, as
  roulette's]. The menu closes too when no chips are left.
- The "Slots Games Played" statistic counts the spins [documented: the statistic's name,
  misc-stats.md; hypothesis: one a spin, as roulette's].
- **Leaving** (Exit) is not offered while the reels turn [hypothesis, as roulette's].

## 5. The menu (`menus\slot_machine_menu.xml`)

- The information box: 400 × 175, 10 in from the main rectangle's left and bottom, the
  bracket of lines, three rows in font 7 at 20 in, at 35, 80 and 125 down: the current
  bet, the chips, the casino's earnings [documented: `SMM_InfoRect`, its three
  displays]. The titles are set in code: "Current Bet:", "Chips:", "<casino> Earnings:"
  [hypothesis: as roulette's, seen on the owner's screenshots of roulette].
- Six buttons right-aligned 50 in from the right, under each other: Spin (W), Increase
  Bet (E), Decrease Bet (Q), Payout List (F), Bet Max (S), Exit (R) [documented: the
  order, `_PCButton_*`; the names from the hotrects; hypothesis: the words, set in code].
  The file puts them 144 above the box; NVForever stands them where roulette's are
  (roulette.md §5) [hypothesis].
- Dimmed: all six while the reels turn; Increase and Decrease at the limits; Spin with
  no chips [design].
- The status text (`SMM_StatusText`, font 7, right-aligned at the buttons' x, 10 down)
  [documented]: the result of a spin ("Three oranges: you win 800 chips." / "You lose 5
  chips.") or a refusal [hypothesis: the words are NVForever's].
- Payout List (F) shows the payout table of §3 in place of the reels, as the model's payout
  screen sets it out (the symbols, "= BET x" and the number); F again or Spin hides it
  [hypothesis: the model's `Payout_In`/`Payout_Out` sequences bring the screen in and
  out].
- The help message `HelpSlots` (0x00172096, Message Box, Auto Display) comes up the first
  time the menu opens, as roulette's [observed: the flags; hypothesis: once per game].
- Keys besides the file's: Esc and Tab leave, Enter spins [design].
- Sounds [observed: SOUN records named for them; design: which event each is for]:
  `GAMESlotsActivate` 0x0014D3EB as the menu opens, `GAMESlotsPullLever` 0x001489D3 at a
  spin, `GAMESlotsIncreaseBet01`–`03` 0x001489D4–D6 (in turn) and `…DecreaseBet`
  0x001489D7 as the bet changes, `GAMESlotsWinSmall` 0x001489D8 (pays up to 5 times the
  bet), `…WinMed` 0x001489D9 (up to 30), `…WinJackpot` 0x001489DA (more), `GAMESlotsLose`
  0x001489FA when the line pays nothing.

## 6. On the screen

- NVForever draws the menu flat, as Caravan's (caravan.md): three reels side by side,
  each a window three symbols high on a pale ground with the payline across the middle,
  the symbols the record's pictures (§2); while a reel turns its symbols run down
  through the window, slowing to the stop [design].
- Not built: the machine's 3D model (`MOD2`: the cabinet, the arm and its
  `Forward`/`Backward` pull, the coins' `Payout_*` fall, the lights), the reels as the
  drum models, the challenge `ChallengeSlotsSCRIPT`, the Atomic Wrangler's jackpot
  message `AtomicWranglerSlotsMsg` (its trigger is not read).

## 7. Tests

- `fnv_game::slots` unit tests: the record's stops dealt onto a reel, every payout row,
  cherries from the left, steps and limits.
- Smoke `slots` (tools/smoke/scenarios.toml): a slot machine at The Tops (0x0016546A in
  `TOPSCasino`) activated with 200 chips: its script opens the menu (bets 1 to 75) with
  the help the first time, a spin of 5 made to stop on three oranges pays 800, a spin
  made to stop on a cherry in the left place pays 10, a losing spin; screenshots of the
  help, the reels turning, a result and the payout list.
- Test aid `NVF_PLAY_TEST_SLOTS` (nvf-play `slots.rs`).

## 8. Open questions for the owner

- The original's payout rules: whether the cherries count from the left reel or
  anywhere, whether `= BET x N` includes the stake, the order of the stops round a reel.
- The words of the buttons, the info rows and the status line (a screenshot of the
  slot menu, one of a win and one of the payout list would settle them).
- Whether Bet Max also spins.
