"""Merge an MO2 profile into a game folder of its own, as hard links, for pilot runs.

The quest pilot and fnvtool read one game folder. A mod list in Mod Organizer 2 (Tale of
Two Wastelands) is only seen merged through MO2's virtual file system, which a batch of
runs cannot use. This writes `<out>/` as MO2 would show the game folder: the game's own
files, then each enabled mod of the profile over them in MO2's priority (the bottom of
`modlist.txt` lowest), every file a hard link to its source (same volume; nothing is
copied and neither the game folder nor the mods are written). The profile's
`plugins.txt` is written beside it as `plugins.txt` (one name a line) for `--plugins`.

    python tools/smoke/pilot/mo2_merge.py --instance F:/Modlists/VNV --profile NVForever \
        --out F:/nvf-ttw/game

Run it again after the mod list changes: `<out>` is emptied first.
"""

from __future__ import annotations

import argparse
import configparser
import os
import shutil
import sys
from pathlib import Path


def game_path(instance: Path) -> Path:
    """The game folder MO2's instance manages (`[General] gamePath`)."""
    ini = configparser.RawConfigParser(strict=False)
    ini.read(instance / "ModOrganizer.ini", encoding="utf-8")
    raw = ini.get("General", "gamePath")
    if raw.startswith("@ByteArray(") and raw.endswith(")"):
        raw = raw[len("@ByteArray("):-1]
    return Path(raw.replace("\\\\", "\\"))


def enabled_mods(profile: Path) -> list[str]:
    """The profile's enabled mods, lowest priority first (MO2 lists the highest first)."""
    lines = (profile / "modlist.txt").read_text(encoding="utf-8").splitlines()
    return [line[1:] for line in reversed(lines) if line.startswith("+")]


def plugins(profile: Path) -> list[str]:
    out = []
    for line in (profile / "plugins.txt").read_text(encoding="utf-8").splitlines():
        line = line.strip().lstrip("*")
        if line and not line.startswith("#"):
            out.append(line)
    return out


def merge(game: Path, mods: list[Path], out: Path) -> tuple[int, int]:
    """Hard links in `out`: the game's files, then each mod's `Data` files over them."""
    files: dict[str, Path] = {}
    for f in game.rglob("*"):
        if f.is_file():
            files[str(f.relative_to(game)).lower()] = f
    for mod in mods:
        for f in mod.rglob("*"):
            if f.is_file() and f.name.lower() != "meta.ini":
                files[str(Path("Data") / f.relative_to(mod)).lower()] = f
    linked = 0
    for rel, src in files.items():
        # The case of the source's own name, under the first-seen folder spelling.
        dst = out / Path(rel).parent / src.name
        dst.parent.mkdir(parents=True, exist_ok=True)
        os.link(src, dst)
        linked += 1
    return linked, len(mods)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--instance", required=True, help="the MO2 instance folder (holds ModOrganizer.ini)")
    ap.add_argument("--profile", required=True)
    ap.add_argument("--out", required=True, help="the merged game folder to write (emptied first)")
    args = ap.parse_args()
    instance, out = Path(args.instance), Path(args.out)
    profile = instance / "profiles" / args.profile
    game = game_path(instance)
    if out.resolve() == game.resolve() or game.resolve() in out.resolve().parents:
        print(f"refused: {out} is the game folder or inside it", file=sys.stderr)
        return 2
    mods = [instance / "mods" / m for m in enabled_mods(profile)]
    missing = [m for m in mods if not m.is_dir()]
    if missing:
        print(f"mods not found: {', '.join(str(m) for m in missing)}", file=sys.stderr)
        return 2
    if out.exists():
        shutil.rmtree(out)
    linked, n = merge(game, mods, out)
    names = plugins(profile)
    (out / "plugins.txt").write_text("\n".join(names) + "\n", encoding="utf-8")
    print(f"merged {game} and {n} mods of profile {args.profile} into {out}: {linked} files linked; {len(names)} plugins in {out / 'plugins.txt'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
