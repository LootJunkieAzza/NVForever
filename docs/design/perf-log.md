# The performance log (#2798)

Not a behaviour of the original game: what NVForever writes to its own log about how a run
went, so that a player's log can say why the frame rate is low. On in every run, with no
setting and nothing on screen. Code: `engine/nvforever/src/perf_overlay/perf_log.rs` (the
lines), `perf_overlay/split.rs` (the spans it shares with the overlay's frame split row),
`perf_overlay/usage.rs` (the CPU/GPU use sampler). Tags as in docs/design: **[design]** our
choice, **[documented]** a cited source, **[observed]** measured here, **[hypothesis]** to be
checked.

## 1. Why

Three player reports of build 0.7.0 (2026-11-16, passed on in #2798) had frame rates far
below their hardware's, each pointing somewhere else: VRAM running out at 4K, vsync holding
a 60 Hz screen to 30 or 20 fps, a main-thread limit, a cap. Before this, a log said the
setup once at the start (adapter, window, present modes, memory budgets) and, while
playing, only when the memory budget pressed the caches; the overlay's readings
(`NVF_FPS_LOG`) were logged only while it showed, and the player reports' histograms go to
the report server, and only for players who opted in [design, #2798].

## 2. The lines

### 2.1 `perf (<why>): …`, one line a stretch of play

A stretch ends after `PERIOD_SECS` (60) seconds of play, at each change of place (the main
menu, a worldspace, an interior: `place change`) and at the exit (`exit`). Its figures are
of its frames of play only [design, #2798]:

| Part | Means | From |
|---|---|---|
| `60.0 s of play in WastelandNV, cell GSGoodspringsCenter (-3,-1)` | the play time of the stretch, the place (an interior is marked `(interior)`) and, outside, the cell under the camera with its grid square | the real clock; the streamer and the world index |
| `58.2 fps, frame mean 17.2 ms, 1% low 33.1 ms, slowest 120.4 ms` | frames a second, their mean time, the mean of the slowest hundredth, the slowest | `Time<Real>` between frames, the 1 % low as the overlay takes it (`readings::low_of`) |
| `main 6.0 ms, render 14.0 ms (wait 10.2 ms)` | the mean main-schedule span (the game's own systems, before the frame cap's wait), the render schedule's span and, inside it, the wait for the window's next image | the frame split's spans (`split.rs`), taken all the time into the log's own accumulators |
| `GPU 15.3 ms` or `GPU time <why not>` | the sum of the top-level render passes' GPU times, a lower bound of the GPU's frame (the shadow passes are not timed) | Bevy's render diagnostics, each pass's moving average over the history Bevy keeps (a few seconds of frames); only when they are recorded: added at the start for the overlay's split row, or by a measuring aid. None are added for the log (§4) |
| `VRAM 6.1 / 10.8 GB` | this process's GPU memory against its budget | `MemoryBudget::vram_use_mb` (checked every 2 s) |
| `RAM game 5.2 GB, machine 61%` | the game's working set (sampled), the machine's RAM in use | the sampler; `MemoryBudget::ram_share` |
| `CPU 23%, game 1.42 cores, GPU 97%` | the machine's CPU use, the cores the game keeps busy (1.0 is one core's whole time), the busiest GPU engine's use (`GPU game` where only the game's own use is read) | the overlay's sampler (`usage.rs`), a second one of the log's own, run for the last `SAMPLE_SECS` (5) of a minute's stretch; `not sampled` for a stretch that ended before |
| `upscaling DLSS Quality  1280x720 -> 1920x1080  model Default` | the upscaler that draws, its render and output size | the overlay's upscaling row text (`readings::upscale_text`) |
| `frame generation off` | `FSR 3.1, running`, `DLSS 2x, running`, or a choice that is not running | the Frame generation row and whether it runs |
| `frame cap none` | the cap the limiter keeps: the page's, variable refresh's, frame generation's, the lowest | `frame_pace::effective_cap` |
| `display sync off at 60 Hz` | the Display sync row and the window's monitor's refresh rate | `FramePace`, `Refresh` |
| `hitches 0` | frames of play over `HITCH_MS` in the stretch | |
| `left out 4.1 s loading, 0.0 s paused` | real time behind a loading screen (and the frame it came down in) and under the pause menu, not in the figures | `nvf_play::loading::Loading`, `nvf_play::pause::Paused` |
| `hint: GPU` | which limit the figures point to (§3) | |

The main menu is a place of its own (`main menu`) and counts as play; every other menu (the
Pip-Boy, a conversation) is play too, since the picture is drawn. A game left paused writes
no line [design].

### 2.2 `perf hitch: …`

A frame of play over `HITCH_MS` (250 ms, the player reports' `hitch` threshold,
`reports/anomaly.rs`, player-reports.md) [design, #2798]: its time, the place and cell,
the last main, render and wait spans the log took and which part was longest (the main
thread, the render thread's own work, or the wait for the window's image). The spans are
the last ones finished, so they are those of the long frame or the one before it
[design]. Within 10 s of a loading screen coming down the line says `after a load` (the
world may still be streaming in). At most one line a second; the next line logged says how
many were not.

### 2.3 `perf summary: …` at the exit

First the whole run: minutes of play, hitches, the peak VRAM (at a 2 s check), the peak
working set (of the samples) and machine RAM share, the loading and paused time. Then a line
for each place with frames, the slowest by its 99th percentile first, at most
`SUMMARY_PLACES` (8), a worldspace or interior marked so (the main menu and `no world` named
alone): minutes, frames, median, 95th and 99th percentile, the share of frames
over 33 ms, the hitches, and the share of frames in each of the player reports' buckets
(`reports/summary.rs` `BUCKETS_MS`: 0, 8, 12, 16.7, 25, 33, 50, 100 ms); the percentiles are
read from those buckets, interpolated inside one, as the session summaries' are.

## 3. Reading a line: which limit

The `hint:` applies these in order, with the thresholds named in `perf_log.rs` [design,
#2798]; the same reading by hand:

- **VRAM**: VRAM at or over 85 % of its budget (`memory_budget::GPU_PRESSURE`, the line at
  which the caches are pressed) says `VRAM near its budget`, before whatever else is said.
  Look for `memory budget: near a budget, caches pressed` lines and hitches: textures and
  models are let go early and loaded again.
- **Frame cap**: a rate within 3 % under the cap is held there (`held at the frame cap`).
- **Vsync**: with plain Vertical sync and frame generation off, a frame mean within 5 % of
  one refresh is held at the refresh rate (`held at the refresh rate by vsync`). Within 5 %
  of two, three or four refreshes, frames miss a refresh and wait for the next: with vsync
  and no variable refresh a frame that misses a 60 Hz refresh shows for 33 ms [observed:
  docs/spec-notes/display-sync.md §1, #1082: Primm's 18-19 ms frames came out at 41-44 fps],
  so the rate falls towards 30, or 20 (`vsync holds it at 1/2 of the refresh rate`). Bevy
  presents vsync as FIFO relaxed where the surface offers it, else FIFO [documented: same
  note §1]; FIFO relaxed shows a late frame at once instead of waiting, so the fall is
  clearest under FIFO, and the start's `frame pace: … vsync presents as …` line says which
  mode this machine uses. What took more than a refresh is then named as below; turning
  vsync off or Variable refresh shows the rate the game makes. Under Variable refresh no
  refresh fraction is read: the screen refreshes when the frame is ready [same note §1]; nor
  with frame generation running, whose `fps` is the game's frames, not the frames shown.
  A wait for the window's image near the whole frame with vsync on is the display's time,
  not the GPU's.
- **Main thread (CPU)**: the main span 75 % of the frame or more (of a refresh, under vsync's
  fraction): the game's own systems. `game N cores` near 1 or more while the machine's CPU
  use is low is the same picture: one thread holds the frame.
- **Render thread (CPU)**: the render span less its wait 75 % of the frame or more: preparing
  and recording the draws.
- **GPU**: the GPU time 75 % of the frame or more, or GPU use 90 % or more, or, with vsync off,
  a wait for the window's image of 40 % of the frame or more (the render thread waits for the
  GPU to give back an image).
- Otherwise `unclear`: read the parts (and the hitches, the loading time) by hand.

## 4. Cost

Per frame: a few resources read, one number pushed to the stretch's list (its room kept),
three atomic takes of the split's accumulators, and the stamps and atomic adds that fill
them (`split.rs`: a few `Instant::now()` a frame across the two threads). The place is
compared by form ID; names are looked up only on a change. The usage sampler collects on its
own thread once a second for 5 s of each minute. GPU timestamps are not added for the log:
Bevy's render diagnostics can only be added when the app is built and cost GPU time and a
read-back every frame; the line uses them where the overlay's split row or a measuring aid
added them [design, #2798]. The cost against the stress baseline is to be measured before
and after (target under 0.1 ms a frame) [hypothesis: to be measured, #2798].

## 5. Test aid

`NVF_PLAY_TEST_STALL=<seconds>:<ms>[,…]`: on the world aids' clock the main thread sleeps
`ms` once (at most 10 s), a frame that long, and logs `test aid: stalling a frame <ms> ms`.
The smoke scenario `perf-log` uses it for the hitch line.
