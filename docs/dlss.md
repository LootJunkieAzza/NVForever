# NVIDIA DLSS in NVForever

DLSS Super Resolution is an optional upscaler beside FSR 3.1: the world is drawn at a lower
resolution and DLSS rebuilds the picture at the window's size, so the game runs faster for a
picture close to native. It is off by default, and NVForever plays the same without it.

DLSS needs:

- Windows;
- an NVIDIA RTX graphics card with a current driver;
- the Vulkan renderer, which is NVForever's default;
- four of NVIDIA's files, which you download from NVIDIA yourself (below).

NVForever ships no NVIDIA file. NVIDIA's files come under NVIDIA's own licence terms.

## Getting the files

1. From NVIDIA's Streamline repository on GitHub
   ([NVIDIA-RTX/Streamline, releases](https://github.com/NVIDIA-RTX/Streamline/releases)),
   download the release **2.14.16**, the archive `streamline-sdk-v2.14.16.zip`. Other
   versions are not tested with NVForever.
2. Open the archive's `bin\x64` folder and take these four files:
   - `sl.interposer.dll`
   - `sl.common.dll`
   - `sl.dlss.dll`
   - `nvngx_dlss.dll` (DLSS itself; the version tested is 310.9.16)
3. Make a folder named `dlss` next to `nvforever.exe` (in the folder NVForever was unpacked
   to, not the game folder) and copy the four files into it:

   ```
   NVForever\
     nvforever.exe
     dlss\
       sl.interposer.dll
       sl.common.dll
       sl.dlss.dll
       nvngx_dlss.dll
   ```

Copy only these four. **Do not copy `nvngx_update.exe`**: NVForever refuses to use a `dlss`
folder that holds a file of that name, because NVIDIA's code would start it without checking
it.

## Turning it on

In the pause menu, **Settings > Graphics**, set **Upscaler** to **DLSS**, and
**Upscaling mode** to Native AA (DLSS at full resolution, also called DLAA), Quality,
Balanced, Performance or Ultra Performance. Each mode shows the share of the screen's width and
height the world is drawn at, for example Quality (67%): Native AA for full resolution with
anti-aliasing only, Quality for the sharpest upscaled look (1440p and 4K), Balanced for more
speed with little visible loss, Performance for the highest frame rate (4K or heavy scenes),
Ultra Performance for very high resolutions only. The mode row serves FSR 3.1 the same way
(its shares differ a little: Balanced is 59% with FSR, 58% with DLSS); it is dimmed while
Upscaler is Off. (It is not the Quality preset row, Low to Very High: it only
says how much smaller the world is drawn before the upscaler rebuilds it.)

With the four files in the `dlss` folder when the game starts, every change takes effect at
once: Off, FSR 3.1 and DLSS, and every mode. Files put into the folder while the game runs
are found at the next start: until then a DLSS choice reads **DLSS (restart)** and the picture
stays as it was. That holds too when the game started with DLSS chosen and the files missing:
once all four are in the folder, DLSS is no longer greyed out and reads DLSS (restart). Loading
a save starts the game again too.

On the command line, `--upscaler dlss --upscaler-quality balanced` (or `native-aa`,
`quality`, `performance`, `ultra-performance`) chooses DLSS; as with every setting, a choice
saved in the settings wins in normal play.

## The DLSS model

Under Upscaling mode, **DLSS model** chooses which of NVIDIA's DLSS models rebuilds the
picture. NVIDIA calls these models presets and names them by letters. The row can be set while
DLSS draws (it is dimmed with "DLSS only" otherwise) and takes effect at once.

| Choice | What it is |
|---|---|
| Default (recommended) | NVIDIA picks for each mode. With NVIDIA's DLSS 310.9.16 that was K for Native AA, Quality and Balanced, M for Performance and L for Ultra Performance; a newer DLSS may pick others. |
| K | NVIDIA's transformer model with the most detail, at a higher cost in frame rate. |
| L | Sharper and steadier than J and K, with less ghosting; the dearest of the four. |
| M | Close to L in looks, closer to J and K in speed. |
| J | Like K, with a little less ghosting and a little more flicker. |

The letters are the ones NVIDIA gives these presets. NVIDIA's other presets are left out: some
are withdrawn and the rest fall back to the default. On the command line: `--dlss-model default`
(or `k`, `l`, `m`, `j`). It is saved as `dlss-model=` in `NVForever.ini` (`dlss-model=K`; the
earlier spelling `dlss-model=Preset K` still loads); a value that is not one of the five loads
as Default.

While DLSS is on:

- the anti-aliasing row reads "Off (upscaling active)": DLSS does the anti-aliasing, and the
  saved anti-aliasing comes back when Upscaler is Off;
- the Sharpening switch drives NVForever's ordinary sharpening pass;
- frame generation is not available: its row reads "Not with DLSS" (frame generation works
  with FSR 3.1 only).

Whenever the files are in the `dlss` folder, NVIDIA's driver may update DLSS in the background,
as it does for other games that use DLSS, whichever upscaler is chosen: NVForever loads NVIDIA's
Streamline at every start the files are there, so that DLSS can be switched to without a
restart. NVForever cannot switch the driver's updates off. With the files there, FSR 3.1 runs
through Streamline too; remove the folder to keep NVIDIA's code out altogether.

## When DLSS is greyed out

When DLSS cannot run, the Upscaler row's DLSS entry is greyed out with the reason. If DLSS is
the upscaler you chose, the game then draws without upscaling; an FSR 3.1 choice keeps working
as before, and FSR 3.1 or Off can still be chosen, taking effect at once. A DLSS choice already
saved stays saved, so it is tried again at every start: fix what the reason says and start the
game again. The log
(`Documents\My Games\NVForever\logs`) has the details in its lines starting `DLSS:`.

| Reason shown | What it means |
|---|---|
| DLSS needs Windows | DLSS runs on Windows only. Elsewhere, FSR 3.1 is the upscaler. |
| DLSS needs the Vulkan renderer | The game was started with another renderer (the `WGPU_BACKEND` variable, for example `dx12`). Start it with Vulkan, the default. |
| DLSS files missing: put them in the dlss folder beside nvforever.exe, then restart | The `dlss` folder is not next to `nvforever.exe`, or one of the four files is not in it. The log names the missing files. Once all four are there, DLSS reads DLSS (restart). |
| The dlss folder holds NVIDIA's updater (nvngx_update.exe): remove it, then restart | Delete `nvngx_update.exe` from the `dlss` folder. This is also shown when the folder could not be read. |
| The DLSS files are not the tested NVIDIA release (Streamline 2.14.16) | `sl.interposer.dll` did not pass NVForever's checks: it is not signed by NVIDIA, or it is not the exact file of Streamline 2.14.16 (another version, or a changed file). The log gives the file's SHA-256. Download release 2.14.16 again from NVIDIA's page and copy the files unchanged. |
| DLSS did not start (the log says why) | NVIDIA's Streamline would not load or start. The log's `DLSS:` lines say where it stopped. |
| A graphics card the game may use cannot run DLSS | A graphics card Vulkan can see lacks what DLSS needs. This is typical on laptops whose built-in graphics also show up in Vulkan beside the NVIDIA card: the game checks every card Vulkan shows, since it could end up starting on any of them. DLSS stays off rather than risk a crash at start. The log names the card. |
| Not on this GPU or driver (DLSS needs an NVIDIA RTX card) | DLSS does not run on this graphics card or driver. It needs an NVIDIA RTX card; updating the driver may help. |
| DLSS stopped working this session; it is tried again at the next start | DLSS gave no picture for a while in this session. For the rest of the session a DLSS choice draws without upscaling; FSR 3.1 and Off can still be chosen and take effect at once. DLSS is tried again at the next start. |

## Removing it

Set Upscaler to Off or to FSR 3.1, quit the game, then delete the `dlss` folder. If the folder
is deleted while DLSS is still chosen, the next start draws without upscaling and DLSS reads
"DLSS files missing".

## Settings saved by an earlier build

Earlier builds kept the upscaler in one row, `fsr-upscaling` in `NVForever.ini` (for example
`fsr-upscaling=DLSS Balanced` or `fsr-upscaling=Quality`). Such a line still loads into the two
rows; once a row is changed, the two rows are saved as `upscaler=` and `upscaler-quality=`, and
those win over the older line.

How DLSS is built into NVForever: [design/dlss-upscaling.md](design/dlss-upscaling.md).
