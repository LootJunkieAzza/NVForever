# Script trace file ownership across process handoffs

[observed: baseline19c0f08a portable source] TraceLog::create opens the configured path with File::create. Two live processes inheriting that path can truncate one another's buffered output; an older writer can later flush at its previous offset. This source path is independent of game scripts and assets.

[observed: earlier private local run, separate source] A save-load handoff left two processes with the same trace destination. The surviving JSONL had a22395989-byte NUL gap and lost preceding events. The log itself stays private. The earlier independently authored overlapping-writer regression passed after the local correction; it is historical proof, not this current-main port's execution.

[hypothesis: engineering design] Atomically create the requested trace with create_new, falling back on a PID/attempt-suffixed sibling when occupied. Retry at most64 names, propagate non-collision I/O errors, and fail visibly when exhausted. Include processId in the unchanged-format header, flush it immediately, and log the actual filename. Never truncate an existing trace.

[observed: this source candidate] The owned regression keeps two writers alive, verifies different paths, writes after their overlap, parses every line as JSON, checks no NUL and verifies each writer's events remain in its own file. It uses only temporary files and authored event labels. It does not itself execute a real multiprocess game/save handoff. Current-port listing, compilation, tests, lint, smoke and actual handoff acceptance are pending.

[hypothesis: limits] Failure after file creation may leave a partial new file; directory permissions, name exhaustion and unrelated filesystem failures remain visible. No gameplay, native parity, whole-mod support, FPS or crash repair is asserted.
