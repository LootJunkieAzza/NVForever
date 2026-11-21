# Cached INI file quotes

[observed: own authored native_description_28 public-script probe, 2026-11-12] The cached read of a quoted file template returned `Air %.0f`, preserving the percent placeholder without its surrounding file quotes. The `read` row completed; this is an API observation, not native-client shutdown/stability acceptance.

[hypothesis] Remove one matching outer double-quote pair only while loading file values into the cache. Caller defaults and values already created/set in the cache retain their existing semantics. The authored `small_libs.rs::cached_ini_file_quotes_are_removed_without_changing_default_or_cached_values` regression checks empty, unmatched and internal quotes, caller defaults, created cached values, HasINISetting warmup and reload. Those boundary checks are NVForever regression expectations; the native read row does not independently establish every boundary.

[observed: change scope] This engine patch contains no Titans mod-source/template rewrite. The separate Titans `%z` source workaround is excluded; no full Titans compatibility claim follows from quote removal.
