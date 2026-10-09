# Spec note: quests, conditions and dialogue as the editor documentation describes them

Clean-room notes for Layer 5 (quests, conditions) and Layer 6 (dialogue). The source is a
private export of 19 GECK wiki pages (geckwiki.com) the owner made on 2026-10-09, kept at
`C:\fnv-private\geckwiki\GECK-20261009092635.xml` and never committed. The pages are:
Quest Data Tab, Quest Stages Tab, Quest scripts, Quest Objectives Tab, Category:Quest
Objectives, Category:Conditions, Category:Quest Functions, Category:Dialogue, Topics Tab,
Conversation Tab, Combat/Detection/Service/Radio/Misc Tab, Dialogue Tree, Quest Item,
Quest and Dialogue Tutorial and Bethsoft Tutorial Basic Quest. Everything below is our own
summary; no wiki text is copied. Tags:
- **[documented: page]**: the page says it;
- **[observed]**: our in-game captures (layer5-captures.md) show it;
- **[hypothesis]**: neither.

Where the documentation and a capture disagree, the capture wins and the difference is
recorded in §5.

## 1. Quests

- **Starting.** A quest is running from the start of the game when "Start Game Enabled"
  is set (QUST `DATA` flag bit 0). Otherwise it starts when a script calls StartQuest,
  SetStage or SetObjectiveDisplayed on it [documented: Quest Data Tab; Quest scripts].
  SetStage starting a stopped quest is also [observed: layer5-captures.md §5.1].
- **Running vs completed.** These are independent. Completing marks the quest done in
  the Pip-Boy, but its script keeps running until StopQuest [documented: Quest scripts;
  Quest Stages Tab]. Also [observed: §5, Complete/Fail flags do not stop the quest].
- **Quest scripts** run only while the quest is running. Their variables stay readable
  and writable while it is stopped, keeping their values [documented: Quest scripts].
  Other scripts address them as `Quest.Variable` [documented].
- **Processing interval.** Normally every 5 s (an INI setting). A per-quest Script
  Processing Delay overrides it: 0 means the 5 s default, and a very small value such as
  0.01 effectively means every frame. SetQuestDelay changes it at run time [documented:
  Quest Data Tab; Quest scripts]. See §5 for how the Default flag behaved in the
  captures.
- **Priority** decides whose dialogue wins when several quests offer lines; higher wins
  [documented: Quest Data Tab]. Layer 6.
- **Quest conditions** (QUST-level CTDA) gate all dialogue in the quest. They are
  checked first, and only if they pass are the individual lines' conditions evaluated
  [documented: Quest Data Tab]. Layer 6.
- **Allow Repeated Stages:** with the flag set, every SetStage of a stage runs its
  results again. Without it, a stage already set does nothing on later calls
  [documented: Quest Data Tab]. Also [observed: §5, `L5.quest.repeated_stage`].
- **Allow Repeated Conversation Topics:** without the flag, one conversation never uses
  the same topic twice [documented: Quest Data Tab]. Layer 6.

## 2. Stages and objectives

- Stages are numbered 0–255. Each has one or more **stage items** (log entries), each
  with conditions, a result script and flags [documented: Quest Stages Tab].
- **SetStage** starts the quest if needed and evaluates every item's conditions. Each
  item whose conditions are true runs its result script [documented: Quest Stages Tab].
  Also [observed: layer5-captures.md §8, all five items of ControlRumble stage 0 ran, in
  record order]. Mods usually make the items' conditions exclusive so that only one runs
  [documented].
- **Stage conditions** have no reference of their own, so most reference functions are
  invalid there, and "Run on Target" is invalid on stages and on quest targets
  [documented: Quest Stages Tab; Quest Objectives Tab]. A variable used in a stage
  condition must be set before SetStage is called [documented]. What an invalid function
  returns is not documented. The runtime's choice to run stage conditions on the player
  is a [hypothesis] (scripts.md).
- **Complete Quest** on a stage item marks the quest completed; with a **Next quest**
  set, that quest becomes the player's active quest [documented: Quest Stages Tab].
- The **log entry text** is documentation only; it is never shown in game [documented:
  Quest Stages Tab].
- **Stage result scripts are "save baked"** [documented: Quest Stages Tab]. We read this
  as: a save stores the stage scripts, so changing them in a plugin does not affect saves
  already made [hypothesis on the meaning; Layer 7 saves].
- **Objectives** have an index (an identifier only; order means nothing), display text
  and zero or more **targets**. Each target is a persistent reference with conditions;
  while the objective is shown, every target whose conditions pass appears on the compass
  and map [documented: Quest Objectives Tab]. "Compass Markers Ignore Locks" makes the
  route to the target ignore locked doors [documented].
- An objective is **displayed or not** and **completed or not**, set by
  SetObjectiveDisplayed / SetObjectiveCompleted (value 1 or 0) and read by the matching Get
  functions. CompleteAllObjectives completes every displayed objective [documented:
  Category:Quest Objectives]. Hiding one objective and showing another replaces it
  without completing it [documented]. Completing a never-shown objective and showing a
  completed one are [observed: layer5-captures.md §5].

## 3. Conditions

- Used on idles, quests (quest, stage and target conditions), dialogue, packages, perks
  and perk entries, effect items, terminals and camera paths [documented:
  Category:Conditions].
- Each condition item is a function, a comparison and a value, or a global when "Use
  Global" is set [documented].
- **Run On** [documented: Category:Conditions]:
  - **Subject:** the owner. In dialogue, the speaker; on quest targets, the player; on
    effects, the reference the effect targets.
  - **Target:** in dialogue, the listener; in packages, the package target; on effects,
    the effect's target.
  - **Reference:** a chosen reference in the world.
  - **Combat Target:** the owner's current combat target. For the player, also whatever
    the player is aiming at, even outside combat.
  - **Linked Reference:** the owner's linked reference.
- **AND/OR precedence.** The OR flag joins an item to the next one. A run of consecutive
  ORs forms one block, and the blocks are ANDed: `A AND B OR C AND D` means
  `A AND (B OR C) AND D` [documented: Category:Conditions]. The same rule was
  [observed: layer5-captures.md §8.3 b, `L5.quest.condition_groups`].

## 4. Dialogue (Layer 6 groundwork)

- Dialogue lives in a quest's tabs: Topics, Conversation, Combat, Detection, Service,
  Radio and Misc [documented: Category:Dialogue].
- An **info** (one line) has [documented: Category:Dialogue]:
  - **top-level** (offered among the default topics) and a **priority** (its place in the
    list);
  - an optional **prompt** replacing the topic text;
  - an optional **speech challenge**, with a difficulty and the Speech skill. In New
    Vegas the choice shows the required level, or "have/need" when the player falls
    short; whether the line is available still depends on its own conditions;
  - an optional fixed **speaker**, which acts like a GetIsID condition;
  - an optional sound played instead of the voice.
- **Two result scripts** per info: **Begin** runs when the actor starts the line, **End**
  when the actor finishes it [documented: Category:Dialogue]. These are the INFO begin/end
  units Layer 5 already reads (scripts.md §1).
- **Info flags** [documented: Category:Dialogue]:
  - **Run Immediately:** the results run when the conversation is built, not when the
    line is spoken.
  - **Say Once:** each actor says the line only once.
  - **Say Once a Day:** once per actor per "day", which is a 12-hour game setting
    (`fSayOncePerDayInfoTimer`).
  - **Goodbye:** the actor leaves dialogue after the line.
  - **Random / Random End:** qualifying Random lines are stacked until a non-Random line
    or a Random End, then one is picked at random.
  - **Always Darken / Info Refusal:** change how the player's prompt is shaded afterwards.
- **Response order** within a topic is kept as the previous-info link (`PNAM`)
  [documented: Category:Dialogue].
- **Special topics:** GREETING (an actor's default greeting) and the player-action
  topics (firing, jumping, knocking things over, picking up objects near an actor, and
  others) [documented: Topics Tab]. Random conversations start with a HELLO whose "link
  from" is empty, follow the topics' link-to/link-from chain (a random pick when several
  are listed) and end with GOODBYE unless a Goodbye-flagged info ends them first.
  Conversation infos use only their first response [documented: Conversation Tab].

## 5. Differences between the documentation and our captures

| Topic | Documentation | Captures | Used by the runtime |
|---|---|---|---|
| Script Processing Delay and the "Default" flag | Default checked = the INI's 5 s; a delay of 0 = 5 s; 0.01 ≈ every frame [documented: Quest Data Tab; Quest scripts] | the record's delay is used whenever it is non-zero, whatever the flag (VCG01: flag set, 0.01 → every frame; CGTutorial: flag set, 1.0 → 1 s); 0 → 5 s [observed: layer5-captures.md §5] | the observed rule |

Everything else in this note agrees with the captures where both cover it.

## 6. Use

- Layer 5 runtime (`crates/fnv-game`, scripts.md §4+): §1–3. The runtime session was
  given these rules on 2026-10-09 for checking.
- Layer 6 dialogue: §4 is the starting point. Capture what it leaves open (selection
  among qualifying infos, timing of the Begin/End results relative to the voice line).
