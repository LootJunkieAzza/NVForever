#!/usr/bin/env python3
"""Builds the release archives of the capture plugin (tools/capture/nvf_capture).

    python tools/capture/package_plugin.py [--out DIR] [--allow-dirty]

Writes into DIR (default: tools/capture/nvf_capture/target/package, which git ignores):

    nvf_capture-v<N>.zip           the plugin, laid out relative to the game's Data folder
      NVSE/Plugins/nvf_capture.dll, nvf_capture.ini
      Docs/nvf_capture/README.md, LICENSE.txt, THIRD-PARTY-LICENSES.txt, BUILD-INFO.txt,
      Docs/nvf_capture/examples/
    nvf_capture-v<N>-source.zip    the plugin's source at the commit it was built from
    nexus-description.bbcode       text for the mod page
    SHA256SUMS.txt

<N> is PLUGIN_VERSION in src/lib.rs. The DLL is built here, in its own target directory,
with the paths of this machine (home folder, checkout) replaced in the binary, and is
checked before it is packed: 32-bit, exports the two functions xNVSE calls, needs no
C runtime DLL, and holds no path of this machine.

The archives are release files, not repository content: never commit them. Nothing is
copied into a game folder; installing a build is done by hand.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import struct
import subprocess
import sys
import time
import zipfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
PLUGIN = REPO / "tools" / "capture" / "nvf_capture"
REPO_URL = "https://github.com/LootJunkieAzza/NVForever"
TARGET = "i686-pc-windows-msvc"
DLL_NAME = "nvf_capture.dll"
EXPORTS = ("NVSEPlugin_Query", "NVSEPlugin_Load")
# What the source archive holds: enough to build the DLL and run its tests, the licence
# and the two notes the code refers to.
SOURCE_PATHS = (
    "LICENSE",
    "tools/capture/nvf_capture",
    "tools/capture/jobs/aa_observe_diag.steps.json",
    "docs/spec-notes/xnvse-capture.md",
    "docs/spec-notes/ai-observer.md",
)
LICENSE_FILE = re.compile(r"^(licen[sc]e|copying|unlicense|notice)", re.IGNORECASE)


class PackageError(Exception):
    pass


def run(cmd: list[str], cwd: Path) -> str:
    r = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, encoding="utf-8", errors="replace")
    if r.returncode != 0:
        raise PackageError(f"{' '.join(cmd[:3])} failed ({r.returncode}):\n{r.stderr.strip()[-2000:]}")
    return r.stdout


def plugin_version(lib_rs: str) -> int:
    m = re.search(r"^pub const PLUGIN_VERSION: u32 = (\d+);", lib_rs, re.MULTILINE)
    if not m:
        raise PackageError("PLUGIN_VERSION not found in src/lib.rs")
    return int(m.group(1))


# ---- the DLL --------------------------------------------------------------------------

def build_dll(target_dir: Path) -> Path:
    """Release build with this machine's paths replaced in the binary."""
    cargo_home = Path(os.environ.get("CARGO_HOME") or Path.home() / ".cargo")
    flags = [
        f"--remap-path-prefix={cargo_home}=cargo-home",
        f"--remap-path-prefix={REPO}=NVForever",
        # The linker stores the PDB's full path in the DLL; keep its file name only.
        "-Clink-arg=/PDBALTPATH:nvf_capture.pdb",
    ]
    # Added to the rustflags of .cargo/config.toml (cargo joins the two lists); the
    # RUSTFLAGS variable would replace them and lose the static C runtime.
    config = f"target.{TARGET}.rustflags=[" + ", ".join(json.dumps(f) for f in flags) + "]"
    env_flags = [v for v in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS") if os.environ.get(v)]
    if env_flags:
        raise PackageError(f"unset {', '.join(env_flags)}: it would replace the plugin's build flags")
    run(["cargo", "build", "--release", "--locked", "--target-dir", str(target_dir), "--config", config], PLUGIN)
    dll = target_dir / TARGET / "release" / DLL_NAME
    if not dll.is_file():
        raise PackageError(f"build did not produce {dll}")
    return dll


def pe_summary(data: bytes) -> dict:
    """Machine, exported names and imported DLL names of a 32-bit PE file."""
    if data[:2] != b"MZ":
        raise PackageError("not a PE file (no MZ header)")
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    if data[pe:pe + 4] != b"PE\0\0":
        raise PackageError("not a PE file (no PE signature)")
    machine, nsec = struct.unpack_from("<HH", data, pe + 4)
    opt_size = struct.unpack_from("<H", data, pe + 20)[0]
    opt = pe + 24
    if struct.unpack_from("<H", data, opt)[0] != 0x10B:
        raise PackageError("not a 32-bit (PE32) image")
    sections = []
    for i in range(nsec):
        vsize, vaddr, rsize, raw = struct.unpack_from("<IIII", data, opt + opt_size + 40 * i + 8)
        sections.append((vaddr, max(vsize, rsize), raw))

    def off(rva: int) -> int:
        for vaddr, size, raw in sections:
            if vaddr <= rva < vaddr + size:
                return raw + rva - vaddr
        raise PackageError(f"RVA 0x{rva:X} is in no section")

    def text(rva: int) -> str:
        o = off(rva)
        return data[o:data.index(b"\0", o)].decode("ascii", "replace")

    export_rva, _, import_rva, _ = struct.unpack_from("<IIII", data, opt + 96)
    exports = []
    if export_rva:
        e = off(export_rva)
        count, names = struct.unpack_from("<I", data, e + 24)[0], struct.unpack_from("<I", data, e + 32)[0]
        exports = [text(struct.unpack_from("<I", data, off(names) + 4 * i)[0]) for i in range(count)]
    imports = []
    if import_rva:
        e = off(import_rva)
        while any(data[e:e + 20]):
            imports.append(text(struct.unpack_from("<I", data, e + 12)[0]))
            e += 20
    return {"machine": machine, "exports": exports, "imports": imports}


def find_leaks(data: bytes, needles: list[str]) -> list[str]:
    """Needles found in the binary as ASCII or UTF-16 text, any case, either slash."""
    low = data.lower()
    found = []
    for n in needles:
        forms = {n.lower(), n.lower().replace("\\", "/"), n.lower().replace("/", "\\")}
        if any(f.encode("utf-8") in low or f.encode("utf-16-le") in low for f in forms):
            found.append(n)
    return found


def check_dll(data: bytes, needles: list[str]) -> dict:
    pe = pe_summary(data)
    if pe["machine"] != 0x14C:
        raise PackageError(f"the DLL is not 32-bit x86 (machine 0x{pe['machine']:X}); the game is")
    missing = [e for e in EXPORTS if e not in pe["exports"]]
    if missing:
        raise PackageError(f"the DLL does not export {', '.join(missing)}")
    runtime = [d for d in pe["imports"] if re.match(r"(vcruntime|msvcp|ucrtbase|api-ms-win-crt)", d, re.IGNORECASE)]
    if runtime:
        raise PackageError(f"the DLL needs a C runtime DLL ({', '.join(runtime)}); it must be linked statically")
    leaks = find_leaks(data, needles)
    if leaks:
        raise PackageError(f"the DLL contains paths of this machine: {', '.join(leaks)}")
    return pe


# ---- third-party licences -------------------------------------------------------------

def linked_packages(meta: dict) -> list[dict]:
    """The packages the plugin is built from (normal and build dependencies, not the
    plugin itself), from `cargo metadata` output."""
    root = meta["resolve"]["root"]
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    seen: set[str] = set()
    todo = [root]
    while todo:
        i = todo.pop()
        if i in seen:
            continue
        seen.add(i)
        for d in nodes[i]["deps"]:
            if any(k.get("kind") != "dev" for k in d["dep_kinds"]):
                todo.append(d["pkg"])
    by_id = {p["id"]: p for p in meta["packages"]}
    return sorted((by_id[i] for i in seen if i != root), key=lambda p: (p["name"], p["version"]))


def license_texts(pkg: dict) -> list[tuple[str, str]]:
    folder = Path(pkg["manifest_path"]).parent
    out = []
    for f in sorted(folder.iterdir()):
        if f.is_file() and LICENSE_FILE.match(f.name):
            out.append((f.name, f.read_text(encoding="utf-8", errors="replace").replace("\r\n", "\n").strip()))
    return out


def render_licenses(pkgs: list[dict], rustc: str) -> tuple[str, list[str]]:
    """The THIRD-PARTY-LICENSES text and the names of packages with no licence file."""
    lines = [
        "nvf_capture.dll is built from the following third-party Rust packages. Each is used",
        "under the licence it names; where a package offers a choice, either applies. The",
        "licence texts found in the packages follow the list.",
        "",
        f"The DLL also holds parts of the Rust standard library ({rustc}; MIT OR",
        "Apache-2.0, https://github.com/rust-lang/rust) and of the Microsoft C runtime,",
        "linked statically.",
        "",
    ]
    texts: dict[str, list[str]] = {}
    missing = []
    for p in pkgs:
        name = f"{p['name']} {p['version']}"
        lines.append(f"- {name}: {p.get('license') or 'see its files'}" + (f" ({p['repository']})" if p.get("repository") else ""))
        found = license_texts(p)
        if not found:
            missing.append(name)
        for file, text in found:
            texts.setdefault(text, []).append(f"{name} ({file})")
    for text, users in texts.items():
        lines += ["", "=" * 78, "From: " + ", ".join(users), "=" * 78, "", text]
    return "\n".join(lines) + "\n", missing


# ---- archives ---------------------------------------------------------------------------

def write_zip(path: Path, files: dict[str, bytes], when: int) -> None:
    """A zip with the entries in name order and one fixed time, so that the same input
    gives the same archive."""
    stamp = time.gmtime(max(when, 315532800))[:6]  # zip times start in 1980
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        for name in sorted(files):
            info = zipfile.ZipInfo(name, date_time=stamp)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o644 << 16
            z.writestr(info, files[name])


def plugin_files(dll: bytes, licenses: str, build_info: str) -> dict[str, bytes]:
    pkg = PLUGIN / "package"
    docs = "Docs/nvf_capture/"
    files = {
        "NVSE/Plugins/" + DLL_NAME: dll,
        "NVSE/Plugins/nvf_capture.ini": (pkg / "nvf_capture.ini").read_bytes(),
        docs + "README.md": (PLUGIN / "README.md").read_bytes(),
        docs + "LICENSE.txt": (REPO / "LICENSE").read_bytes(),
        docs + "THIRD-PARTY-LICENSES.txt": licenses.encode("utf-8"),
        docs + "BUILD-INFO.txt": build_info.encode("utf-8"),
    }
    for f in sorted((pkg / "examples").iterdir()):
        files[docs + "examples/" + f.name] = f.read_bytes()
    return files


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--out", type=Path, default=PLUGIN / "target" / "package", help="output folder (default: %(default)s)")
    ap.add_argument("--allow-dirty", action="store_true",
                    help="build although the plugin's files have uncommitted changes (the archives are then named -dirty: not for release)")
    args = ap.parse_args(argv)
    try:
        version = plugin_version((PLUGIN / "src" / "lib.rs").read_text(encoding="utf-8"))
        commit = run(["git", "rev-parse", "HEAD"], REPO).strip()
        when = int(run(["git", "show", "-s", "--format=%ct", "HEAD"], REPO).strip())
        dirty = run(["git", "status", "--porcelain", "--", *SOURCE_PATHS], REPO).strip()
        if dirty and not args.allow_dirty:
            raise PackageError("uncommitted changes in the plugin's files (commit them, or pass --allow-dirty for a trial build):\n" + dirty)
        stem = f"nvf_capture-v{version}" + ("-dirty" if dirty else "")
        rustc = run(["rustc", "--version"], REPO).strip()

        print(f"building {DLL_NAME} (plugin version {version}, commit {commit[:12]}) ...")
        dll = build_dll(PLUGIN / "target" / "package-build").read_bytes()
        pe = check_dll(dll, [str(Path.home()), str(REPO)])
        meta = json.loads(run(["cargo", "metadata", "--format-version", "1", "--locked", "--filter-platform", TARGET], PLUGIN))
        licenses, missing = render_licenses(linked_packages(meta), rustc)
        if missing:
            raise PackageError("no licence file found in: " + ", ".join(missing))

        build_info = "\n".join([
            f"nvf_capture plugin version {version}",
            f"built from commit {commit}" + (" plus uncommitted changes (not a release build)" if dirty else ""),
            f"source: {REPO_URL}/tree/{commit}/tools/capture/nvf_capture",
            f"compiler: {rustc}, target {TARGET}, C runtime linked statically",
            f"{DLL_NAME} sha256: {sha256(dll)}",
            "",
        ])
        args.out.mkdir(parents=True, exist_ok=True)
        main_zip, source_zip = args.out / f"{stem}.zip", args.out / f"{stem}-source.zip"
        write_zip(main_zip, plugin_files(dll, licenses, build_info), when)
        run(["git", "archive", "--format=zip", f"--prefix={stem}-source/", "-o", str(source_zip), "HEAD", "--", *SOURCE_PATHS], REPO)
        page = args.out / "nexus-description.bbcode"
        page.write_bytes((PLUGIN / "package" / "nexus-description.bbcode").read_bytes())
        sums = "".join(f"{sha256(p.read_bytes())}  {p.name}\n" for p in (main_zip, source_zip))
        (args.out / "SHA256SUMS.txt").write_text(sums, encoding="utf-8", newline="\n")

        print(f"  DLL: {len(dll):,} bytes, imports {', '.join(sorted(pe['imports']))}")
        for p in (main_zip, source_zip, page):
            print(f"  wrote {p} ({p.stat().st_size:,} bytes)")
        if dirty:
            print("  NOT A RELEASE BUILD: uncommitted changes.")
        elif not run(["git", "branch", "-r", "--contains", "HEAD"], REPO).strip():
            print(f"  note: commit {commit[:12]} is on no remote branch yet; push it before publishing (BUILD-INFO links to it).")
        return 0
    except PackageError as e:
        print(f"package_plugin: {e}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
