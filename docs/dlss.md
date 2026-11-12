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

1. In the pause menu, **Settings > Graphics**, set **Upscaling** to one of the `DLSS:`
   entries: Native AA (DLSS at full resolution, also called DLAA), Quality, Balanced,
   Performance or Ultra Performance.
2. Start NVForever again. DLSS is loaded only when the game starts, so choosing it, or
   leaving it for FSR or Off, takes effect at the next start; until then the entry reads
   "(from the next start)" and the picture stays as it was. Loading a save starts the game
   again too. Changing between DLSS's own qualities takes effect at once.

On the command line, `--upscaler dlss --upscaler-quality balanced` (or `native-aa`,
`quality`, `performance`, `ultra-performance`) chooses DLSS; as with every setting, a choice
saved in the settings wins in normal play.

While DLSS is on:

- the anti-aliasing row reads "Off (upscaling active)": DLSS does the anti-aliasing, and the
  saved anti-aliasing comes back when Upscaling is Off;
- the Sharpening switch drives NVForever's ordinary sharpening pass;
- frame generation is not available: its row reads "Not with DLSS" (frame generation works
  with FSR 3.1 only).

Like other games that use DLSS, NVIDIA's driver may update its DLSS files in the background
while DLSS is on. NVForever cannot switch that off.

## When the DLSS entries are greyed out

When DLSS cannot run, its entries are greyed out with the reason, and the game draws without
upscaling. A DLSS choice already saved stays saved, so it is tried again at every start: fix
what the reason says and start the game again. The log
(`Documents\My Games\NVForever\logs`) has the details in its lines starting `DLSS:`.

| Reason shown | What it means |
|---|---|
| DLSS needs Windows | DLSS runs on Windows only. Elsewhere, FSR 3.1 is the upscaler. |
| DLSS needs the Vulkan renderer | The game was started with another renderer (the `WGPU_BACKEND` variable, for example `dx12`). Start it with Vulkan, the default. |
| DLSS files missing: put them in the dlss folder beside nvforever.exe, then restart | The `dlss` folder is not next to `nvforever.exe`, or one of the four files is not in it. The log names the missing files. |
| The dlss folder holds NVIDIA's updater (nvngx_update.exe): remove it, then restart | Delete `nvngx_update.exe` from the `dlss` folder. This is also shown when the folder could not be read. |
| The DLSS files are not signed by NVIDIA | `sl.interposer.dll` did not pass the signature check. Download the release again from NVIDIA's page and copy the files unchanged. |
| DLSS did not start (the log says why) | NVIDIA's Streamline would not load or start. The log's `DLSS:` lines say where it stopped. |
| A graphics card the game may use cannot run DLSS | A graphics card Vulkan can see lacks what DLSS needs. This is typical on laptops whose built-in graphics also show up in Vulkan beside the NVIDIA card: the game checks every card Vulkan shows, since it could end up starting on any of them. DLSS stays off rather than risk a crash at start. The log names the card. |
| Not on this GPU or driver (DLSS needs an NVIDIA RTX card) | DLSS does not run on this graphics card or driver. It needs an NVIDIA RTX card; updating the driver may help. |
| DLSS stopped working this session; it is tried again at the next start | DLSS gave no picture for a while in this session, so the game draws without it until the next start. |

## Removing it

Set Upscaling to Off or to an FSR 3.1 entry and start the game again, then delete the `dlss`
folder. If the folder is deleted while DLSS is still chosen, the next start draws without
upscaling and the DLSS entries read "DLSS files missing".

How DLSS is built into NVForever: [design/dlss-upscaling.md](design/dlss-upscaling.md).
