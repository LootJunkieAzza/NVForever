# Android LAN test diagnostics

This optional debug-build support helps investigate issue #2436, including the reported
R5 crash loading a save created on the phone. It does not establish that crash's cause.
No save conversion, deletion, automatic new game, or game-data upload is involved.

The launcher offers **LAN test logs**, off by default. The user supplies the paired PC's
private IPv4 upload URL and a masked session token. Only debuggable builds expose the UI;
the INTERNET permission and cleartext network configuration exist in the debug source
set. Release builds receive neither. The transport further restricts requests to literal
RFC1918 IPv4 HTTP endpoints at `/api/v1/log-bundles`, with no user information, queries,
fragments, DNS, redirects, system proxy, cookies, or automatic authentication.

The PC accepts a POST with `X-NVForever-Token`, `Content-Type: application/zip`, and a
bounded Content-Length. A successful response must contain `ok: true`, an ID, and the
exact uploaded ZIP's SHA-256. A transport deadline bounds connection, reads and stalled
writes. Retries resend the same complete immutable archive; the receiver deduplicates
identical uploads. No resumable byte-range POST is attempted.

Only a worker created by the separate launcher process performs networking. The game
process and its crash hook never wait for network. The worker checks for changes every
30 seconds while enabled and retries queued snapshots on subsequent launcher starts.
Turning off or changing pairing cancels the active connection. Old-pair snapshots are
never uploaded to a new pairing. Invalid acknowledgements or pairing rejection pause
uploads until the user saves corrected settings. Transient failures use bounded backoff.

Collection reads only strict NVForever-owned run/event/crash text filenames directly
under the app's `home/logs`. It refuses symbolic links and leaves originals untouched.
It selects bounded recent file tails, omits unclassified lines, and filters sensitive
content before archiving. In particular, saves, game assets, unrelated logcat, device
identifiers, full-system telemetry, tokens, private filesystem paths and player/save
identity are excluded. This conservative filtering can omit useful evidence; omitted
and truncated data are counted. Generic ZIP entry names avoid revealing local filenames.

Additional metadata contains validated source revisions, package version/API level,
the app's own historical game-process exit codes, and bounded restart breadcrumbs made
from fixed phase enums, timestamps and app-owned PIDs. No raw Intent arguments or exit
descriptions are included. A killed old game process is expected during load handoff;
the successor's launch and exit evidence must be assessed separately.

The generated queue is limited to four archives and 48 MiB, with a maximum 16 MiB per
archive. A full queue stops collection rather than evicting unacknowledged snapshots.
Acknowledgement removes only generated queue files. The uploader never deletes original
run logs; the engine's pre-existing log rotation still applies. A bounded history of
acknowledged snapshot IDs prevents repeated uploads of unchanged technical content.

A Java-only support APK may reuse an already-qualified native library pair only when
all native compilation inputs are unchanged. `SOURCE.txt` records the complete support
APK's corresponding source. `NATIVE-SOURCE.txt` separately records the native libraries'
revision when supplied by the packaging helper. Final verification checks both packaged
native hashes, both source references, actual licences, signing key and APK alignment.
R5 itself remains immutable. Pairing tokens are never included in source, public APK
configuration, repository fixtures, or public reports.

Host tests use synthetic files and a local test HTTP server for archive scope/redaction,
limits, acknowledgements, offline retry, cancellation and pairing isolation. Android
sources must also compile against the supported SDK. Device acceptance remains separate:
background/process survival, actual Wi-Fi loss/reconnect, successor failure and real
crash evidence are not established by host tests or CI.
