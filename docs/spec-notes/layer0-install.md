# Spec note: Layer 0 — the user's install and the original executable

Ground truth comes from the user's own Steam copy at
`F:\SteamLibrary\steamapps\common\Fallout New Vegas` (inventory of 465 files, 9.9 GB,
kept privately as `install-inventory.json`; hashes of the staged files in
`staged-inventory.json`). Tags as in the other notes.

## Game build

- `FalloutNV.exe` version resource: FileVersion/ProductVersion **1.4.0.525**, PE
  timestamp 2011-07-01 [observed]. This is the final patch level; all goldens are for
  this build.
- Content present: base game, Dead Money, Honest Hearts, Old World Blues, Lonesome Road,
  Gun Runners' Arsenal, and the Caravan/Classic/Mercenary/Tribal packs [observed].

## Executable protection (constraint)

- The EXE has a `.bind` section and its `.text` section has Shannon entropy 8.000
  bits/byte (encrypted/compressed), consistent with a Steam DRM wrapper [observed].
- Per spec section 2 (no DRM circumvention) the code section is **not** unpacked or
  disassembled. Behavioural ground truth will come from public documentation, data
  observation, and runtime capture through the script-extender ecosystem (xNVSE) on the
  running game. Only unencrypted data (imports, `.rdata` strings, resources) was read.

## Third-party components (identify → replace with open equivalents)

Evidence from the import table and unencrypted strings [observed]:

| Component | Evidence | Open replacement direction |
|---|---|---|
| Direct3D 9 + D3DX 9 (d3dx9_38) | imports `d3d9.dll`, `d3dx9_38.dll` (shader assembly/compilation) | Vulkan/OpenGL via an abstraction; shaders rewritten |
| Havok physics | RTTI `hk*` classes, `Havok libs built with version`, `HavokStream.cpp` path | Open physics engine (e.g. Bullet/Jolt) driven by NIF `bhk*` collision data |
| SpeedTree RT | `CSpeedTreeRT::*` strings, `BSTreeManager.cpp` | Custom tree renderer from TREE/SPT data (only 3 TREE records in FalloutNV.esm) |
| FaceGen SDK | `SDK\FaceGen\algs.cpp`, `BSFaceGen*` classes, `.egm/.egt/.tri` patterns | Original morph implementation from documented EGM/EGT/TRI formats |
| Bink video | imports `binkw32.dll` (`_BinkOpen@8` …), `Data\Video\FNVIntro.bik` | An open Bink decoder (e.g. FFmpeg's) or skip intro videos |
| Ogg Vorbis | imports `libvorbisfile.dll` (`ov_open_callbacks` …) | libvorbis (already open) |
| Audio output | imports `DSOUND.dll`, `WINMM.dll`; no FMOD/Wwise strings; two Miles-related strings (`Enumerates Miles samples and streams`, `DebugMilesPS3`, near file offset 0xC408AB) but no Miles DLL import [observed] — likely leftover debug/console code [hypothesis] | OpenAL Soft / miniaudio |
| Input | `DINPUT8.dll`, `XINPUT1_3.dll` | SDL |
| Steam | `steam_api.dll` | Not needed |
| Networking | `WSOCK32.dll` (21 ordinal imports) | purpose unknown [hypothesis: debug/remote tools]; not needed |

**UI is not Flash.** The spec's hypothesis of a Flash/Scaleform UI is not supported:
there are no Scaleform/GFx strings, and the EXE references XML menu definitions
(`Data\Menus\Main\hud_main_menu.xml`, `inventory_menu.xml`, `dialog_menu.xml`,
`Data\Menus\CharGen\*.xml`) which ship in `Fallout - Misc.bsa` under `menus\` [observed].
The UI layer must reimplement Bethesda's XML menu system (tile/layout definitions +
DDS textures + fonts).

Other observations:
- Shader packages: `Data\Shaders\*.sdp` (16 files) [observed]; precompiled D3D9 shaders,
  to be replaced, not ported.
- `Fallout_default.ini` `[Archive]` lists the base archives (see `bsa.md`).
- 247 loose MP3 files: 199 under `Data\Music\`, 48 radio songs under
  `Data\Sound\songs\` [observed].

## Tooling decisions

- Static analysis of the EXE is blocked (above). Ghidra remains useful for the
  unprotected DLLs only if ever needed; none are engine code.
- Corpus ground truth for Layer 1 is produced by two independent readers written from
  the spec notes (Rust candidate, Python oracle) plus, on the user's machine, FNVEdit
  exports. The HEDR record count provides a third, Bethesda-authored check.
