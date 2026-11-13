# CompileScript sibling paths

[observed: own authored native_compile_path_25 public-script probe, 2026-11-12] Calling a UDF through `../CompileScript/NVFContract/generated.gek` returned 42. The internal-parent alias `../CompileScript/NVFContract/../NVFContract/generated.gek` also returned 42. Both result rows completed; this does not establish native cache-form identity or physical filesystem containment.

[hypothesis] Normalize only CompileScript/GetUDFFromFile paths relative to `nvse/user_defined_functions`, retaining a lexical Data boundary. Reject lexical escapes and absolute/drive/stream paths before reading. Use the normalized Data-relative key for NVForever's existing cache. Ordinary file APIs keep their existing strict path handling. Authored tests check execution, alias cache identity, GetUDFFromFile, force recompilation and rejected requests not reaching the reader.

[observed: existing reader and owned junction check reported in COORDINATION-REPAIR-AGENT.md] The existing file reader can follow a filesystem junction out of a configured root; an owned audit fixture measured that behavior without sensitive files. This patch establishes lexical normalization/cache identity only. It does not enforce physical symlink/junction/reparse containment. A physical containment policy requires separate upstream agreement and tests.

[documented: crates/fnv-game/tests/main.rs and Cargo.toml] Tests that mutate the process-wide mod_files ROOTS run in isolated named test binaries. This fixture retains its independent reader process and is explicitly registered as compile_script_paths, rather than joining the parallel integration modules. Modern Minimap motivates the sibling-path case; complete Modern Minimap compatibility is not certified.
