# Lighting cold campfires

NVForever's own feature, not in the original [design]. Code: `crates/fnv-game/src/campfire.rs`
(the rule and the placing), `engine/nvf-play/src/campfire.rs` (the prompt, Ready Item, the
settings row), `engine/nvf-play/src/reticle.rs` (a prompt with two actions). Tests:
`crates/fnv-game/tests/campfire.rs`, the `campfire` and `reticle` unit tests of nvf-play, the
smoke scenario `campfire-light-vanilla`.

## What and why

Many of the base game's campfires stand cold: logs and stones with nothing burning. The
original has no way to light one; its campfires open the recipe menu [documented:
FalloutNV.esm records, the scripts `CraftingCampfireRecipesScript` and
`VCG03CraftingCampfireRecipesScript` open the recipe category `CampfireRecipes`]. With the
switch on, the player can light a cold campfire, for the light at night and the look of a
camp.

## The data [documented: FalloutNV.esm records]

- The crafting campfires are the activators `CampfireCrafting01` to `04` and `VCG03CampfireCrafting03`, all named
  "Campfire". Their scripts open the recipe category `CampfireRecipes`.
- Their models (`clutter\camps\campfire01.nif` to `04`) are logs, tyres and cinder blocks:
  no flames, no particles, no light.
- 110 are placed (100 in WastelandNV, 10 in interiors). 44 have a fire placed within 150
  units, mostly `FXFireSmall01` (model `Effects\Ambient\FXFireSmall01.NIF`,
  whose flames come from its add-on node), `FXFireMed01`, and the light `FireBrightOrange`
  (radius 200, flickering). The other 66 stand cold.

## The rule [design]

- **A campfire**: a reference whose base is an activator whose script names the recipe
  category `CampfireRecipes` among its references. The category is found by its Editor ID in
  the load order, not by a list of FormIDs, so a campfire another plugin makes from those
  scripts counts too.
- **Burning**: an enabled, not deleted reference, placed or created, stands in the
  campfire's cell within 150 units of it (`FIRE_NEAR`, from the survey above), whose base is
  a light (LIGH) or a moveable static whose model path contains "fire".
- **Lighting** a cold campfire places one `FXFireSmall01` and one `FireBrightOrange` (by
  Editor ID) at its position, in its cell, with its angles, as PlaceAtMe places references.
  Without either form in the load order nothing is placed. The two are created references,
  saved with the game like any PlaceAtMe's, so a lit campfire stays lit; nothing puts it out.
  Once lit it burns by the rule above and is not lit again.

## In play [design]

- Over a cold campfire the prompt reads "E) Craft   R) Light fire" (the keys as Activate and
  Ready Item are bound), the pair centred where the one action stands.
- Activate still runs the campfire's script (the recipe menu). Ready Item lights it and, that
  frame, neither draws, reloads nor holsters the weapon. Nothing happens while a script has
  the player's movement off, as for Activate.
- Over a burning campfire, or with the switch off, the prompt and Ready Item are the
  original's (the drawing of created lights and flames is not switched; see The switch).
- Log: `campfire: 0x… lit (fire 0x…, light 0x…)`; the created fire and light are drawn and lit
  as any created reference (`dropped reference … drawn`, `lights: created light … lit`).

## The switch

The Gameplay › Game row `light-campfires`, "Light campfires", off by default and kept in the
INI (docs/settings-reference.md). It gates one thing only: the campfire-lighting action. Off,
there is no second action and nothing is placed (`Game::light_campfires`;
`Game::light_campfire` does nothing while it is off).

**The rest of this change is not behind the switch.** Drawing what a script creates runs
for every player, switch on or off: a light a script places (`PlaceAtMe` of a LIGH)
now sheds its light, and every created model's add-on nodes (flames, sparks) now give off
particles (`nvf-play` `dropped.rs`, `nvforever` `lights.rs` and `particles.rs`). This is a
fix towards the original, which draws both, and it changes single player for everyone: any
script that places a light or a model with flames now shows them.
