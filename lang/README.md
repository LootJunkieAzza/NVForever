# Translations of NVForever's own words

NVForever writes some words itself: the settings page, a few messages, the terminal
hacking log. The original game's text is not copied (LEGAL.md). The game's own text
(items, dialogue, quests) comes from the plugins of your install, so a localized install
already shows it in its language. Only NVForever's own words need a translation, and
this folder holds them, one file per language (#1443). Shipped (#1948): `de.toml`
(Deutsch), `es.toml` (Español), `fr.toml` (Français), `it.toml` (Italiano) and `ru.toml`
(Русский).

## A language file

`<id>.toml`, UTF-8, for example `de.toml`:

```toml
name = "Deutsch"

[words]
"Language" = "Sprache"
"Accepted." = "Angenommen."
"{n} of {len} right" = "{n} von {len} richtig"
```

- `name` is how the settings page lists the language, in the language itself.
- Each line under `[words]` is the English text exactly as NVForever writes it, then
  your translation. A word with no line, or an empty one, stays English.
- `{n}`, `{len}`, `{needed}` and the like are filled in by NVForever. Keep them, in any
  order your language needs.
- Write your own words. Don't copy the original game's strings into a file
  (LEGAL.md).
- The settings page's own words are its tabs and headings (`Gameplay`, `Saving`, …),
  `Search`, `Changed`, `Keyboard search`, `Previous heading`, `Next heading` and what an
  empty list says (`No settings here.`, `No results.`, `No changed settings.`). The
  search finds a row by its words in the language shown and in English, and by its INI
  key, so a translation never hides a row from it. `Stands for: {rows}.` begins the note
  of a row listed for others (its `{rows}` the rows it stands for).

## Where NVForever looks

1. This `lang` folder beside `nvforever.exe` (the translations that come with NVForever).
2. `lang` in NVForever's home (`Documents\My Games\NVForever\lang`), for your own.
3. The folder `NVF_LANG_DIR` names, if set.

When two folders have the same id, the first one wins. The language is chosen on the
settings page (Display, Interface: Language) and acts at once. It is kept as `language=<id>` in
`NVForever.ini`.

Until one is chosen there, the start picks the language of the game's text when its code
page tells it and a file of it was found: Windows-1251 (a Russian install) chooses `ru`
(#1948). The start's log says so. Windows-1250 and 1252 are each several languages, so
they choose none and English stays.

## Checked

`cargo test --workspace` reads every file here: each must parse, each English line must
still be a text NVForever writes (found in the sources), and each translation must keep
its line's `{places}`. A line whose English is changed in the code fails the check until
the file is updated too.

What goes through these files today: the main menu and the pause menu (their entries,
Back, Defaults, the quit and keep questions), the settings page (its tabs, rows, notes,
ON / OFF and the names of the choices in its lists and on its choice meters: Off, Auto,
Amber, Very Hard …), the hacking log and the terminal refusals (#2095). A choice's
name with no line (a resolution, a display's name, a language's own name) shows as it
is. A note that names a choice names it as the page shows it, so a translated choice
name is translated in the notes too.

The game's fonts hold 256 glyphs in the install's code page (docs/spec-notes/fonts.md),
so a translation can only use the letters that code page has: Cyrillic on a Russian
install (`[Text] CodePage=1251`), but not Chinese.

So the language shown must suit the install's code page: German, French, Italian and
Spanish need Windows-1252 (an English, German, French, Italian or Spanish install), and
Russian needs Windows-1251 (a Russian install). A letter the fonts lack is drawn as `?`.
Every file here keeps to its code page's letters (no `œ`).
