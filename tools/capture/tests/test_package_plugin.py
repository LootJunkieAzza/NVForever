"""Tests for tools/capture/package_plugin.py (release archives of the capture plugin).
Nothing is built: the DLL checks run on a small synthetic PE file."""
import json
import os
import struct
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

import package_plugin as pp  # noqa: E402


def fake_dll(machine=0x14C, exports=pp.EXPORTS, imports=("KERNEL32.dll", "USER32.dll"), extra=b""):
    """A PE32 image with one section that holds an export and an import table."""
    base, raw = 0x1000, 0x200
    sec = bytearray(0x400)
    strings = 0x100

    def put(text):
        nonlocal strings
        at = strings
        sec[at:at + len(text) + 1] = text.encode("ascii") + b"\0"
        strings += len(text) + 1
        return base + at

    struct.pack_into("<II", sec, 24, len(exports), 0)       # NumberOfNames
    struct.pack_into("<I", sec, 32, base + 0x40)            # AddressOfNames
    for i, name in enumerate(exports):
        struct.pack_into("<I", sec, 0x40 + 4 * i, put(name))
    for i, name in enumerate(imports):
        struct.pack_into("<I", sec, 0x60 + 20 * i + 12, put(name))
    head = bytearray(raw)
    head[:2] = b"MZ"
    struct.pack_into("<I", head, 0x3C, 0x40)
    head[0x40:0x44] = b"PE\0\0"
    struct.pack_into("<HH", head, 0x44, machine, 1)
    struct.pack_into("<H", head, 0x40 + 20, 224)
    opt = 0x40 + 24
    struct.pack_into("<H", head, opt, 0x10B)
    struct.pack_into("<IIII", head, opt + 96, base, 40, base + 0x60, 20 * len(imports))
    struct.pack_into("<IIII", head, opt + 224 + 8, len(sec), base, len(sec), raw)
    return bytes(head) + bytes(sec) + extra


class DllChecks(unittest.TestCase):
    def test_reads_exports_and_imports(self):
        pe = pp.pe_summary(fake_dll())
        self.assertEqual(pe, {"machine": 0x14C, "exports": list(pp.EXPORTS), "imports": ["KERNEL32.dll", "USER32.dll"]})
        self.assertEqual(pp.check_dll(fake_dll(), [r"C:\Users\someone"]), pe)

    def test_refuses_what_xnvse_could_not_load(self):
        for dll, word in [(fake_dll(machine=0x8664), "32-bit"),
                          (fake_dll(exports=("NVSEPlugin_Query",)), "NVSEPlugin_Load"),
                          (fake_dll(imports=("KERNEL32.dll", "VCRUNTIME140.dll")), "C runtime"),
                          (b"not a dll", "PE file")]:
            with self.assertRaises(pp.PackageError) as e:
                pp.check_dll(dll, [])
            self.assertIn(word, str(e.exception))

    def test_finds_machine_paths_in_either_encoding_and_slash(self):
        home = r"C:\Users\Someone"
        self.assertEqual(pp.find_leaks(b"..c:\\users\\someone\\.cargo..", [home]), [home])
        self.assertEqual(pp.find_leaks("x C:/Users/Someone/src".encode("utf-16-le"), [home]), [home])
        self.assertEqual(pp.find_leaks(b"cargo-home\\registry\\src", [home]), [])
        with self.assertRaises(pp.PackageError):
            pp.check_dll(fake_dll(extra=b"C:\\Users\\Someone\\x.pdb"), [home])


class Licences(unittest.TestCase):
    def test_lists_what_is_linked_and_groups_equal_texts(self):
        with tempfile.TemporaryDirectory() as tmp:
            def pkg(name, licence, files):
                d = Path(tmp) / name
                d.mkdir()
                for f, text in files.items():
                    (d / f).write_text(text, encoding="utf-8")
                return {"id": name, "name": name, "version": "1.0.0", "license": licence,
                        "repository": f"https://example.org/{name}", "manifest_path": str(d / "Cargo.toml")}

            apache = "Apache text\r\n"
            meta = {
                "packages": [pkg("root", "GPL-3.0-or-later", {}), pkg("a", "MIT OR Apache-2.0", {"LICENSE-MIT": "MIT a", "LICENSE-APACHE": apache}),
                             pkg("b", "Apache-2.0", {"LICENSE": "Apache text\n", "README.md": "no"}), pkg("devonly", "MIT", {"LICENSE": "x"}),
                             pkg("bare", "MIT", {})],
                "resolve": {"root": "root", "nodes": [
                    {"id": "root", "deps": [{"pkg": "a", "dep_kinds": [{"kind": None}]}, {"pkg": "devonly", "dep_kinds": [{"kind": "dev"}]}]},
                    {"id": "a", "deps": [{"pkg": "b", "dep_kinds": [{"kind": "build"}]}, {"pkg": "bare", "dep_kinds": [{"kind": None}]}]},
                    {"id": "b", "deps": []}, {"id": "devonly", "deps": []}, {"id": "bare", "deps": []}]},
            }
            pkgs = pp.linked_packages(meta)
            self.assertEqual([p["name"] for p in pkgs], ["a", "b", "bare"])
            text, missing = pp.render_licenses(pkgs, "rustc 1.0")
            self.assertEqual(missing, ["bare 1.0.0"])
            self.assertIn("- a 1.0.0: MIT OR Apache-2.0 (https://example.org/a)", text)
            self.assertIn("From: a 1.0.0 (LICENSE-APACHE), b 1.0.0 (LICENSE)", text)
            self.assertEqual(text.count("Apache text"), 1)
            self.assertNotIn("devonly", text)


class Archive(unittest.TestCase):
    def test_layout_is_relative_to_data_and_repeatable(self):
        files = pp.plugin_files(b"dll bytes", "licences", "build info")
        self.assertEqual(files["NVSE/Plugins/nvf_capture.dll"], b"dll bytes")
        for name in ["NVSE/Plugins/nvf_capture.ini", "Docs/nvf_capture/README.md", "Docs/nvf_capture/LICENSE.txt",
                     "Docs/nvf_capture/THIRD-PARTY-LICENSES.txt", "Docs/nvf_capture/BUILD-INFO.txt",
                     "Docs/nvf_capture/examples/example_read_only.steps.json"]:
            self.assertIn(name, files)
        self.assertTrue(all(n.startswith(("NVSE/Plugins/", "Docs/nvf_capture/")) for n in files))
        with tempfile.TemporaryDirectory() as tmp:
            a, b = Path(tmp) / "a.zip", Path(tmp) / "b.zip"
            pp.write_zip(a, files, 1_800_000_000)
            pp.write_zip(b, dict(reversed(list(files.items()))), 1_800_000_000)
            self.assertEqual(a.read_bytes(), b.read_bytes())
            with zipfile.ZipFile(a) as z:
                self.assertEqual(z.namelist(), sorted(files))
                self.assertEqual(z.read("NVSE/Plugins/nvf_capture.dll"), b"dll bytes")

    def test_shipped_files_are_what_the_plugin_reads(self):
        pkg = pp.PLUGIN / "package"
        for f in sorted((pkg / "examples").glob("*.steps.json")):
            job = json.loads(f.read_text(encoding="utf-8"))
            self.assertEqual(job["format"], "fnv.capture.steps/1")
            self.assertEqual(job["name"] + ".steps.json", f.name, "a finished job is moved by its name")
        # The shipped ini must leave the default directory alone: no active dir= line.
        ini = (pkg / "nvf_capture.ini").read_text(encoding="utf-8")
        self.assertFalse([line for line in ini.splitlines() if line.strip().startswith("dir=")])
        self.assertGreaterEqual(pp.plugin_version((pp.PLUGIN / "src" / "lib.rs").read_text(encoding="utf-8")), 7)


if __name__ == "__main__":
    unittest.main()
