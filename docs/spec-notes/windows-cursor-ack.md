# Windows cursor requests and native acknowledgement

## Observed failure

[observed: NVForever 060d69dd, actual Windows startup and Save 11 restore logs,
2026-11-17] Two native cursor-position calls failed with OS error 6. The first
preceded the first rendered frame; the second followed state restoration in the
replacement process. The logs do not establish teardown, a persistent input
failure, a controller failure, or the native cause.

[documented: pinned [Bevy 0.20.1 window backend](https://docs.rs/crate/bevy_winit/0.20.1/source/src/system.rs)] The ordinary Window setter stores
requested coordinates before native application. Its backend logs a rejected
native call and still copies Window into its private cache. Consuming the
script's one-shot before that call can therefore lose a rejected intent and
leave UI hit testing at a position the OS did not accept.

[documented: [winit 0.30.13 Windows implementation](https://docs.rs/crate/winit/0.30.13/source/src/platform_impl/windows/window.rs) and Microsoft
[SetCursorPos](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setcursorpos)]
The failing winit boundary calls ClientToScreen successfully, then SetCursorPos
fails. SetCursorPos takes coordinates, not an HWND. An invalid/destroyed HWND
has not been diagnosed. Microsoft describes the cursor as shared and requires
the current input desktop and window-station write access. This change does
not switch desktops or infer which requirement failed.

## Engine transaction

Every `[design]` item in this note is an engine design choice (ownership, bounds,
timeouts, schedule order), not a claim about the original game or its plugins.

[design] Windows script requests transfer into an entity-owned request/receipt,
with a new identity for every explicit request, including identical coordinates.
Submission never changes Window or the backend cache. At the existing main-thread
backend boundary, an available focused, visible, non-minimized owned window with
a settled client size and an observed cursor inside its client area may attempt
the native call. Otherwise it defers without acknowledgement.

[documented: Microsoft Win32 API] [GetForegroundWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getforegroundwindow)
identifies the current foreground window; [GetCursorPos](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getcursorpos)
reports screen coordinates, and [ScreenToClient](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-screentoclient)
converts those into client coordinates. [design] The Windows consumer also reads
these immediately before an attempt, through the live owned winit handle. Failure
to query ownership, a different foreground window or a cursor outside this client
area defers the request. These checks change no focus, desktop or cursor state.
They do not make the separate native call atomic with user activity.

[design] The production consumer accepts an injectable native operation for
offscreen tests. Success updates Window and the private cache together; the
ordinary Window synchronization consequently has no duplicate warp to send.
Failure preserves both previous positions and retains a rejected receipt.
Ordinary Window writers take precedence and keep their original backend path.
No cache type or field is made public. No CursorMoved, touch takeover, held
contact, controller state, or fallback input is fabricated.

[design] The script adapter permits one retry after native rejection: at most
two native attempts for one explicit intent. Readiness/acknowledgement expires
after 120 adapter sync frames. These are bounded engine policies, not claims
about original-game timing or evidence that a second native attempt will succeed.
Each native failure remains an error, with request/window identity, coordinates,
attempt count and native readiness facts. Deferral changes and retirement reasons
are recorded without allocating a per-frame queue.

## Ownership and coordinates

[design] A transient game receipt plus an ownership generation prevents old
results from surviving game replacement, menu close/reopen, page transitions,
HUD-cursor changes, or primary-window entity replacement. A final PostUpdate
ownership check precedes the Last-stage backend consumer. A new explicit request
supersedes an earlier request and receives a distinct identity; a later UI request
also cancels an earlier attempt before native application. Missing windows,
closed owners, ordinary Window writes and exhausted budgets retire intent with
a reason; retirement is not success.

[design] Accepted intent remains available to same-script GetCursorPos queries
before transfer, including when the previous UI pointer is known. Subsequent
frames use the authoritative Window/UI pointer while native acknowledgement is
pending or rejected. This is intent readback, not an OS observation.

[design] An acknowledged Window position is handed back before PreUpdate source
input callbacks run. Only a matching live receipt updates the recorded UI pointer
through the existing coordinate conversions; it emits no input event or click.
The Windows native ownership query also permits the initial warp when the cursor
is already inside the client area but no CursorMoved event has arrived.

[design] Raw coordinates remain logical window pixels. XML coordinates retain
the 960-high interface frame and the request's captured StartMenu frame, including
temporary MCM callback context. Conversion is resolved again from the current
window size before an attempt. Clamping stays inside the final owned physical
pixel. Other platforms retain their existing Window-setter path; Android receipt
and touch integration requires separate owner-led adoption and validation.

## Validation and limits

Native-result regression cases call the same production backend
consumer with injected readiness and native success/failure. They cover cache
publication, duplicate attempts, ordinary writers, unavailable windows and the
native attempt budget. Engine cases exercise source requests, repeated values,
stale results, menu/runtime/window retirement, expiry and source readback. The
existing MCM consumer case supplies a rejected native result before success and
then follows Window coordinates into actual menu input and source list routing
at menu scales 0.5, 0.75 and 1.0.

Offscreen schedule controls install the actual
ScriptSwitchesPlugin and ScriptInputPlugin. Source intent is submitted in the
Scripts stage and cancelled/replaced in the UI stage; an injected production
consumer in Last must receive no stale native attempt. A success control then
queries the actual source API inside the production PreUpdate input set before
Update receipt retirement. This exercises real schedule/Commands application
instead of invoking each ownership system manually.

Windows regression selectors:

- `bevy_winit`: `cursor_position::tests::`
- `nvf-play`: `script_cursor::tests::`
- `nvf-play`: `script_input::mcm_page_tests::mcm_page_warp_reaches_window_then_menu_input_and_source_list_routing`

The Windows `bevy_winit` cursor-position and `nvf-play` `script_cursor` controls
passed on `05cd76be` with injected native results. That run does not establish a
Windows MCM result. See PR #2884 for revision-specific results.
Actual Windows calls, physical OS cursor readback, first-click acceptance,
startup/load/handoff, keyboard/raw-camera and controller neighbors, and Android
integration are not qualified by these controls.
A successful native API return is not an independent GetCursorPos observation;
native rounding/clipping and following OS cursor events still require real-window
validation. No original-game/native-plugin parity or OS6 cure is claimed.
