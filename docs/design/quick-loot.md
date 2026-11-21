# Quick Loot

A design feature, not a parity target: the original has no such list. It follows Fallout 4's
looting list (#2933), in the original's HUD look. Every figure here is a design choice
[design, #2933] unless tagged otherwise.

## 1. What it does

With the NVF Features › Quick Loot row **Enabled** on (off by default), a container, a body
or, while sneaking, a person under the crosshair shows what it holds in a framed list beside
the crosshair. The game does not stop for it.

| Key (the player's binding) | What it does |
|---|---|
| Activate (E) | takes the selected stack (a pocket: steals it, rolling its chance) |
| Ready Item (R) | takes everything; not from a pocket, where each item is its own roll |
| Wait (T) | opens the full container or pickpocket menu, as Activate would without Quick Loot |
| mouse wheel | moves the selection (up moves it up) |

While the list shows, those keys do nothing else: no reload or holster, no Wait menu, no
camera zoom, scope zoom or hotkey step, no grab (`quick_loot::holds_input`, read by
`control`, `wait`, `weapon_wheel`, `aim_zoom`, `grab`, `activate`). The crosshair's prompt
is not drawn while it shows: the list stands for it.

## 2. What shows it

The activation target (`activate::Target`) is:

- a container that opens (`Action::Open` on a CONT). A locked one keeps its lock prompt;
- a body or an ash pile to search (`Action::Search`); the ash pile lists its actor's items;
- a pocket the player may pick (`Action::Steal` with no refusal,
  `fnv_game::Game::pickpocket_refusal`). One that has caught the player keeps its prompt.

Not on a reference whose script has an OnActivate block, one set off by its activate parents
only, or one that cannot be activated: the game's own activation decides those, so their
prompt stays and E acts as it always did. That holds for pockets too: a sneaking target whose
script has an OnActivate block (a quest guard that withholds `Activate`) cannot be picked by
hand, so it gets no list. A perk's activate choices on a body (Cannibal and the like) are
not offered by E or R; T's full menu path offers them as before. Not under the weapon wheel, VATS, the character
creator, a kill camera, an idle that holds the camera, or a script's DisablePlayerControls
(movement or rollover text). Not while the HUD is hidden (its keys then do what they always do).
The list stays on a reference only while it stays what it was: a pocket whose owner stands up,
dies or turns hostile, a knocked-out body that wakes, or a change of theft makes the list anew
(or none). The frame a list appears it takes no keys (the systems that read Ready Item and the
wheel before it saw no list).

## 3. The rules are the container menu's

The list is a `menu::model::MenuModel` opened on the target as the container menu opens it,
the pickpocket's `Steal` (rules, both Sneak skills, its seeded rolls) included. So:

- someone else's container: a take is theft (`crime::ItemsTaken`; karma and witnesses as
  in the menu); the list and its keys are in the crime colour, "Steal" and "Steal All";
- a pocket: each take rolls the stack's chance (`Steal::chance` for the whole stack, the
  figure the list shows); a failure is caught (`pickpocket::Pickpocketed`), the list closes
  and stays closed on that person;
- a body's dropped weapon is a row and is picked up from the ground when taken.

Only a take is sounded as one: while the list shows the game runs on, and a shot's spent
ammo or a hotkeyed Stimpak is not a take. What came in is said as a script's AddItem says it ("Stimpak (2) added"; four or more stacks:
"N items added (M in all)"), and each take has its item's pick-up sound (Take All its own
sound), as in the menu. The rows are read again every half second and after each take.

## 4. Look

Design A of five mockups (#2933), with the changes after the first build: a dark backing (the **Background opacity** slider, 0% to 100%, 70% by default) between two HUD-coloured lines with short ticks at their
ends; the name and the selection's place ("3/8") over a rule; up to six rows, the selection
boxed and filled faintly; a short bar above or below where more rows are; the keys' lines under
the box, centred on it. The name and the rows' words start at one line; the box is at least as
wide as the keys' line with that room before it on both sides, wider for wider rows. The item
names and counts are in the figures' size (regular), the name over the rule in its own. Every line of words is one height, the rows' (40 units): the name's, the columns' heads', each
row's and the keys' line; the words' capitals are centred in it, whatever their face (the
game font's capitals measured by their ink: its glyph pictures carry room for the glow).
The box is centred on the crosshair's height whatever its rows, and starts 150 units right of
the crosshair; it is never wider than the room to the right bracket of the HUD's bottom right
pieces (`hud::RIGHT_EDGE_IN`): a name too long for that is cut with "...", before its count, which always shows; a list wider than that room even so (every column on a narrow screen, long keys) is drawn smaller until it fits. The container's own name widens the box as far as the room allows. The bars' edges are on whole screen pixels, so equal rooms look equal. In the HUD colour
(`interface::amber`, recoloured with it), the crime colour for theft, at its own scale
(the **Scale** row, 50% to 100% in steps of 10%, 100% by default; not the HUD
scale), in the Menu Font row's font (the game's font 7, or the sharp font,
`pause::MenuFont`), redrawn when either changes. Its own words (keys, heads, Empty) follow the Language row (`lang/*.toml`). The place and the columns' heads are in the
small face (font 3, or the sharp font's small size in bold); the columns' figures in the same
size, not bold.

Columns, each with its own row (all off by default, greyed while Quick Loot is off): **Show
weight**, **Show value**, **Show value per weight**; a pocket always shows each item's
**chance**. With no column the count stands right-aligned in a column of its own; with any,
it goes beside the name, "Stimpak (2)", and the heads' line has **ITEM** over the names, where they start; the heads at 50% opacity [design, #2933]. The columns are as wide as their
widest figure in the drawn font, 40 units apart, so they align in both fonts.

Each drawing checks itself (`quick_loot::overlaps`: texts that meet, a text outside the list
or off its row) and logs `quick loot: drawn "<name>" (… font Original|Sharp …): no overlaps`,
or what overlaps; the unit tests run it for every combination of columns in both fonts.

## 5. Not done

- A controller has no wheel: its D-pad still steps the hotkeys; the selection moves with the
  mouse only.
- MenuMode 1008 scripts (GetMenuTargetRef) and a container's open animation and sounds do not
  run for the list; T's full menu runs them.
