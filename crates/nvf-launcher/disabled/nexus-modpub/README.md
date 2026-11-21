# Nexus and ModPub downloads (not built)

This code was removed from the running NVForever launcher. It is kept so it can be put back later. Nothing in this folder is compiled.

- `links.rs` was `crates/nvf-launcher/src/links.rs`. It parsed `nxm://` and `modl://` links, sealed the Nexus API key, downloaded the archive into the launcher's `downloads` folder, and registered the link handlers.
- `ui.rs` is the launcher UI that went with that: the Nexus API key window, the one-time "open these links" question, the Downloads tab, and the code that polled a download.

To restore it, move `links.rs` back to `src/links.rs`, add `mod links;` in `src/main.rs`, and put the functions in `ui.rs` back on `App` in `src/app.rs`. The line numbers in `ui.rs` are where those functions lived when they were removed.

The running launcher still installs an archive you drop on the Mods page. That path does not use this code.
