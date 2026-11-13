# Built-in luck-to-skill getter fallback

[observed: own authored native_builtin_settings_24 public-script query, 2026-11-12] GetGameSetting fAVDSkillLuckBonusMult returned 0.5 in the recorded private reference fixture without a GMST override. The one result row and end record completed; later automatic-shutdown behavior does not turn this into native-client stability acceptance.

[observed: crates/fnv-game/src/actor_values.rs] The existing NVForever skill calculation already uses 0.5 for this setting. The public getter's fallback table lacks that entry.

[hypothesis] Add only the measured default through the existing getter fallback table. Loaded GMST records and session updates retain precedence; unknown names retain the existing failure. The authored functions-module regression checks 0.5 with no override, 0.75 loaded, 0.25 after a session update, and an unknown setting. These are narrow getter/precedence checks, not an audit of undocumented built-ins.

[hypothesis] The exact Elysium consumer package/version identity remains unverified; no specific package identity or complete compatibility claim is made here.

[observed: own native_prep_compiler_contracts_20261113, declaration_rhs_getter; capture SHA256 37751a539755c7b0bf3271dc13380110d481a5701a600e6ebd9b22039b0d1e29] A fresh runtime-UDF compound declaration using GetGameSetting fAVDSkillLuckBonusMult as its RHS returned 0.5. This corroborates that getter value in the later controlled private world; it does not independently prove absence of a record override or expand the default table.
