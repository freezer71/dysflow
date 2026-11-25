"""Tests de scripts/windows-imports.py sur des binaires Windows fabriqués
(PE32+ minimaux, avec leurs tables d'import directe et différée) : la liste
blanche stricte, les sous-dossiers, dysflow-espeak.exe et dysflow-app.exe exigés (les DLL graphiques de GPUI
pour la seule app), le runtime
Visual C++ joint à côté de chaque binaire. Aucun outil, aucun Windows :

    python3 -I scripts/test_windows_imports.py
"""
import contextlib
import importlib.util
import io
import os
import shutil
import struct
import tempfile
import unittest
from pathlib import Path

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("windows_imports", os.path.join(HERE, "windows-imports.py"))
WI = importlib.util.module_from_spec(spec)
spec.loader.exec_module(WI)

SECTION_RVA = 0x1000
RAW_OFFSET = 0x200


def pe(direct=(), delayed=()):
    """Un PE32+ d'une section, qui importe `direct` (répertoire 1) et
    `delayed` (répertoire 13)."""
    body = bytearray()
    # Les descripteurs d'abord (et leur entrée nulle), les noms ensuite.
    direct_at = 0
    body += bytes(20 * (len(direct) + 1))
    delayed_at = len(body)
    body += bytes(32 * (len(delayed) + 1))
    for i, name in enumerate(direct):
        struct.pack_into("<I", body, direct_at + 20 * i + 12, SECTION_RVA + len(body))
        body += name.encode("ascii") + b"\0"
    for i, name in enumerate(delayed):
        struct.pack_into("<I", body, delayed_at + 32 * i + 4, SECTION_RVA + len(body))
        body += name.encode("ascii") + b"\0"
    body += bytes(-len(body) % 0x200)

    optional_size = 112 + 16 * 8
    header = bytearray(RAW_OFFSET)
    header[0:2] = b"MZ"
    struct.pack_into("<I", header, 0x3C, 0x40)
    header[0x40:0x44] = b"PE\0\0"
    struct.pack_into("<HHIIIHH", header, 0x44, 0x8664, 1, 0, 0, 0, optional_size, 0x22)
    optional = 0x44 + 20
    struct.pack_into("<H", header, optional, 0x20B)
    directories = optional + 112
    if direct:
        struct.pack_into("<II", header, directories + 8 * 1, SECTION_RVA + direct_at, 20 * (len(direct) + 1))
    if delayed:
        struct.pack_into("<II", header, directories + 8 * 13, SECTION_RVA + delayed_at, 32 * (len(delayed) + 1))
    section = optional + optional_size
    header[section:section + 8] = b".idata\0\0"
    struct.pack_into("<IIII", header, section + 8, len(body), SECTION_RVA, len(body), RAW_OFFSET)
    return bytes(header) + bytes(body)


CRT = ["vcruntime140.dll", "msvcp140.dll"]


def write(directory, name, direct=(), delayed=()):
    path = Path(directory) / name
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(pe(direct, delayed))
    return path


def package(root):
    """Un paquet Windows complet et juste."""
    write(root, "dysflow.exe", ["KERNEL32.dll", "onnxruntime.dll", "sherpa-onnx-c-api.dll", "VCRUNTIME140.dll",
                                "api-ms-win-crt-heap-l1-1-0.dll"], ["bcryptprimitives.dll"])
    write(root, "onnxruntime.dll", ["KERNEL32.dll", "MSVCP140.dll", "dxgi.dll", "SETUPAPI.dll"])
    write(root, "sherpa-onnx-c-api.dll", ["onnxruntime.dll", "KERNEL32.dll"])
    write(root, "vcruntime140.dll", ["KERNEL32.dll", "api-ms-win-crt-runtime-l1-1-0.dll"])
    write(root, "msvcp140.dll", ["VCRUNTIME140.dll", "KERNEL32.dll"])
    write(root, "dysflow-espeak.exe", ["KERNEL32.dll", "ADVAPI32.dll", "api-ms-win-crt-stdio-l1-1-0.dll"])
    # ort est en load-dynamic : l'app n'importe pas onnxruntime.dll.
    write(root, "dysflow-app.exe", ["KERNEL32.dll", "USER32.dll", "d3d11.dll", "dxgi.dll", "dcomp.dll",
                                    "dwrite.dll", "UIAutomationCore.dll", "combase.dll", "RPCRT4.dll",
                                    "VCRUNTIME140.dll", "api-ms-win-crt-heap-l1-1-0.dll"])
    (Path(root) / "espeak-ng-data").mkdir()
    (Path(root) / "espeak-ng-data" / "fr_dict").write_bytes(b"pas un binaire")
    (Path(root) / "licences").mkdir()
    for name in WI.FONT_LICENCES:
        (Path(root) / "licences" / name).write_text("SIL Open Font License 1.1")


def check(root, *options):
    """(code de sortie, sortie, erreurs) de `check`."""
    out, err = io.StringIO(), io.StringIO()
    code = 0
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
        try:
            WI.check(root, espeak="--sans-espeak" not in options)
        except SystemExit as exit:
            code = exit.code
    return code, out.getvalue(), err.getvalue()


class Base(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="windows-imports-"))
        self.addCleanup(shutil.rmtree, self.root)


class Imports(Base):
    def test_direct_and_delayed_imports_are_read(self):
        path = write(self.root, "a.exe", ["KERNEL32.dll", "onnxruntime.dll"], ["dbghelp.dll"])
        self.assertEqual(WI.imports(path), ["KERNEL32.dll", "onnxruntime.dll", "dbghelp.dll"])

    def test_a_file_that_is_not_a_pe_is_refused(self):
        path = self.root / "faux.dll"
        path.write_bytes(b"ELF")
        with self.assertRaises(ValueError):
            WI.imports(path)


class Check(Base):
    def test_a_complete_package_passes(self):
        package(self.root)
        code, out, err = check(self.root)
        self.assertEqual(code, 0, err)
        self.assertIn("dysflow.exe : KERNEL32.dll, onnxruntime.dll", out)

    def test_a_dll_outside_the_whitelist_fails_even_if_windows_might_have_it(self):
        package(self.root)
        write(self.root, "dysflow.exe", ["KERNEL32.dll", "d3d12.dll"])
        code, _, err = check(self.root)
        self.assertEqual(code, 1)
        self.assertIn("d3d12.dll", err)

    def test_a_missing_package_dll_fails(self):
        package(self.root)
        (self.root / "sherpa-onnx-c-api.dll").unlink()
        code, _, err = check(self.root)
        self.assertEqual(code, 1)
        self.assertIn("sherpa-onnx-c-api.dll", err)

    def test_a_redistributable_is_never_taken_for_windows(self):
        package(self.root)
        (self.root / "vcruntime140.dll").unlink()
        code, _, err = check(self.root)
        self.assertEqual(code, 1)
        self.assertIn("VCRUNTIME140.dll", err)

    def test_without_dysflow_espeak_the_package_fails(self):
        package(self.root)
        (self.root / "dysflow-espeak.exe").unlink()
        code, _, err = check(self.root)
        self.assertEqual(code, 1)
        self.assertIn("dysflow-espeak.exe absent", err)

    def test_without_dysflow_app_the_package_fails(self):
        package(self.root)
        (self.root / "dysflow-app.exe").unlink()
        code, _, err = check(self.root)
        self.assertEqual(code, 1)
        self.assertIn("dysflow-app.exe absent", err)

    def test_without_the_font_licences_the_app_fails(self):
        package(self.root)
        (self.root / "licences" / "OFL-Literata.txt").unlink()
        code, _, err = check(self.root)
        self.assertEqual(code, 1)
        self.assertIn("licences/OFL-Literata.txt absent", err)

    def test_the_font_licences_are_those_of_the_app(self):
        fonts = Path(HERE).parent / "app" / "assets" / "fonts"
        self.assertEqual(sorted(p.name for p in fonts.glob("OFL-*.txt")), sorted(WI.FONT_LICENCES))

    def test_the_graphics_dlls_are_for_the_app_only(self):
        package(self.root)
        write(self.root, "dysflow.exe", ["KERNEL32.dll", "d3d11.dll"])
        code, _, err = check(self.root)
        self.assertEqual(code, 1)
        self.assertIn("dysflow.exe importe d3d11.dll", err)

    def test_the_app_still_has_no_redistributable_or_unknown_dll(self):
        for dll in ["d3d12.dll", "libwinpthread-1.dll", "d3dcompiler_47.dll"]:
            root = self.root / dll
            root.mkdir()
            package(root)
            write(root, "dysflow-app.exe", ["KERNEL32.dll", dll])
            code, _, err = check(root)
            self.assertEqual(code, 1, dll)
            self.assertIn(f"dysflow-app.exe importe {dll}", err)

    def test_the_gate_has_no_dysflow_espeak_and_says_so(self):
        package(self.root)
        (self.root / "dysflow-espeak.exe").unlink()
        (self.root / "dysflow-app.exe").unlink()
        code, _, err = check(self.root, "--sans-espeak")
        self.assertEqual(code, 0, err)

    def test_dysflow_espeak_importing_mingw_fails(self):
        package(self.root)
        write(self.root, "dysflow-espeak.exe", ["KERNEL32.dll", "libwinpthread-1.dll"])
        code, _, err = check(self.root)
        self.assertEqual(code, 1)
        self.assertIn("libwinpthread-1.dll", err)

    def test_binaries_in_subfolders_are_checked(self):
        package(self.root)
        write(self.root, "outils/extra.exe", ["KERNEL32.dll", "libgcc_s_seh-1.dll"])
        code, out, err = check(self.root)
        self.assertEqual(code, 1)
        self.assertIn("outils/extra.exe : ", out)
        self.assertIn("outils/extra.exe importe libgcc_s_seh-1.dll", err)

    def test_a_dll_of_the_package_must_sit_beside_the_binary_that_imports_it(self):
        # Windows ne cherche pas dans le dossier d'à côté : onnxruntime.dll à
        # la racine ne sert pas à un binaire d'un sous-dossier.
        package(self.root)
        write(self.root, "outils/extra.exe", ["KERNEL32.dll", "onnxruntime.dll"])
        code, _, err = check(self.root)
        self.assertEqual(code, 1)
        self.assertIn("outils/extra.exe importe onnxruntime.dll", err)


class DeployCrt(Base):
    def test_the_crt_lands_beside_each_binary_that_imports_it(self):
        crt = self.root / "crt"
        dist = self.root / "dist"
        write(crt, "vcruntime140.dll", ["KERNEL32.dll"])
        write(crt, "vcruntime140_1.dll", ["VCRUNTIME140.dll"])
        write(crt, "msvcp140.dll", ["VCRUNTIME140_1.dll"])
        write(dist, "dysflow.exe", ["KERNEL32.dll", "VCRUNTIME140.dll"])
        write(dist, "outils/extra.exe", ["MSVCP140.dll"])
        with contextlib.redirect_stdout(io.StringIO()):
            WI.deploy_crt(crt, dist)
        self.assertTrue((dist / "vcruntime140.dll").is_file())
        self.assertFalse((dist / "msvcp140.dll").exists(), "rien d'inutile à la racine")
        for name in ["msvcp140.dll", "vcruntime140_1.dll", "vcruntime140.dll"]:
            self.assertTrue((dist / "outils" / name).is_file(), name)


if __name__ == "__main__":
    unittest.main()
