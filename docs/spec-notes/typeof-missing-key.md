# typeof missing key

[observed] Own native_typeof_missing_26 job, 2026-11-12, using the xNVSE 6.4.9 public script interface: TypeOf on an absent string-map key returns NULL; a present numeric key returns Number; directly indexing an absent key fails. The three-row job completed. The reference process later faulted on automatic shutdown; that is not a stability result.

[design] Distinguish absent-element errors so TypeOf can handle only that case without inserting a key or suppressing unrelated expression errors.
