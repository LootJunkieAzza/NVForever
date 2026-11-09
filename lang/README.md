# Translations of NVForever's own words

NVForever writes some words itself: the settings page, a few messages, the terminal
hacking log. The original game's text is not copied (LEGAL.md). The game's own text
(items, dialogue, quests) comes from the plugins of your install, so a localized install
already shows it in its language. Only NVForever's own words need a translation, and
this folder holds them, one file per language (#1443). `ru.toml` (Russian) is the first
(#1948).

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

## Where NVForever looks

1. This `lang` folder beside `nvforever.exe` (the translations that come with NVForever).
2. `lang` in NVForever's home (`Documents\My Games\NVForever\lang`), for your own.
3. The folder `NVF_LANG_DIR` names, if set.

When two folders have the same id, the first one wins. The language is chosen on the
settings page (Interface, Language) and acts at once. It is kept as `language=<id>` in
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

What goes through these files today: the settings page (its tabs, rows, notes and ON /
OFF), the hacking log and the terminal refusals. A choice's name in a list (Off, Auto,
Amber …) is not yet translated, so the notes that mention one keep its English name.

The game's fonts hold 256 glyphs in the install's code page (docs/spec-notes/fonts.md),
so a translation can only use the letters that code page has: Cyrillic on a Russian
install (`[Text] CodePage=1251`), but not Chinese.
