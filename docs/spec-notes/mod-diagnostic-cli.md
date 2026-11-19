# Mod diagnostic CLI boundaries

This is repository design, not an observation of the original game or native plugins.
The command formats and user workflow are in
[scoped mod diagnostics](../handoff/mod-diagnostic-cli.md).

The existing source compiler selects source or retained bytes in `GameData::build`.
The connection report consumes that result; it never substitutes a textual search for
the compiled IR. Its registration labels refer only to
`fnv_game::functions::lookup`, with `lookup_console` only in the console-number range,
matching the actual numbered Game dispatcher. Named calls are left unchecked. There is
no native compatibility conclusion from a response to guessed arguments, and this
report does not issue those calls at all.

`nvf_play::mod_log` emits an aid banner, load-order row, sampled failure lines and
per-plugin totals at exit. Those have different meanings: a sampled failure line is
not an invocation count, and an absent exit row does not mean zero failures. The log
reader preserves that distinction, separates appended launch markers, reports unknown
totals as null and exposes no message/path text by default. A new output file is
guarded before creation, including the log-only command's otherwise unremembered
install ancestors.

`fnv_game::trace::EventKind` distinguishes a call failure that continues with zero,
an interpreter error, a skipped statement, an unhandled function, an unread argument
and a script print. The Script Runner summary consumes these actual events after the
ordinary restart/new-game calls, without changing event production or result rows.
The runtime already deduplicates some diagnostics; the summary labels its counters
as trace events rather than promising exact call totals.

The CLI's resource limits are repository choices: selected connection units/call sites,
log bytes/line length/session count, and retained diagnostic entry/word sizes. Crossing
a connection/log input limit is an explicit error, not a truncated compatibility
result. Optional runner diagnostics instead report unavailable without partial counts
and preserve the pre-existing command exit/results. Borrowed lengths are admitted
before formatting or cloning event words. The authored
controls cover representative retention and input boundaries. They contain no mod
scripts, real logs, game assets or native implementation source.

The public GPL source inspiration and exact h9x000 commit identifiers are retained in
the handoff. Their historical outcomes are separate from validation of this adapted
implementation. The separately coordinated engine-stderr transport change, version
reporting, compiler grammar, callback policy, HUD production and renderer consumers
are outside this leaf.
