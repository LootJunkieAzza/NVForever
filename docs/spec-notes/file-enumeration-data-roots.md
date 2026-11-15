# Data-relative file enumeration

[documented] `GetFilesInFolder` (alias `GetFiles`) and `GetFoldersInFolder`
(alias `GetFolders`) enumerate names under Data, accept an optional wildcard
filter, and accept an empty directory to enumerate Data itself. Sources:
https://geckwiki.com/index.php/GetFilesInFolder (revision37182) and
https://geckwiki.com/index.php/GetFoldersInFolder (revision34868), read2026-11-14 by the original proposal author. Fresh independent retrieval
returned HTTP403; no new page revision was independently verified.

Repository source context (original7201 inspection; preserved in this main78408 integration):
`mod_files::Roots::read` is the ordered Data-layer list. The engine's
`script_input::mod_file_roots` supplies home/mods followed by VFS Data; the
fnvtool script/query routes supply Data, and script-runner can prepend authored
Data-layout roots. `script::ScriptState::new` separately selects the write root.
`docs/design/install-layout.md` describes home/mods layered over game Data.
Enumeration does not consult `Roots::config`, `Roots::reader`, or archives; this
leaf preserves that existing scope. The host's once-per-process root setup is
unchanged.

Repository source context: `mod_files.rs`. The old enumeration branch directly
joined a request to each root and its parent. That could include names from a
parent namesake directory; on Windows a parent-component request could escape
Data. Other Data path consumers already use `find_dir`/`on_disk` for component
validation and case-insensitive directory resolution.

[hypothesis: implementation policy] Enumerate only each Data read root through
that existing checked directory resolver. Preserve optional glob matching,
empty-directory behavior, original name spelling, first-root case-insensitive
deduplication and sorted results. Preserve extensionless, dot-prefixed and
deployment-marker filenames. This is the existing lexical policy, not a new
physical symlink/junction guarantee.

[documented] `ReadArrayFromFile` is game-relative:
https://geckwiki.com/index.php/ReadArrayFromFile. Its separate `read_root`
GameRoot/game-parent behavior is unchanged. INI sibling-path handling is also
unchanged. Restricting enumeration roots does not remove an ordinary metadata
file already inside Data or suppress a caller's schema warning.

[hypothesis: authored controls] Four dedicated-process Game query controls use
fresh owned directories. They discriminate parent-only files and folders, invalid
requests beside outside sentinels, and legitimate Data/layer/case/nested/dot-folder/
extensionless/filter neighbors. A game-relative array-read positive control
protects the other API's root policy. Original source assertions are unchanged.
The controls cover the public compiled-query boundary; execution results belong
to the exact tested revision. No native or installed-mod compatibility claim is
made from this authored coverage contract.
