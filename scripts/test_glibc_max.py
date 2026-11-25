"""Tests de scripts/glibc-max.py sur des ELF fabriqués (une table de versions
requises minimale) : aucun outil, aucun Linux :

    python3 -I scripts/test_glibc_max.py
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
spec = importlib.util.spec_from_file_location("glibc_max", os.path.join(HERE, "glibc-max.py"))
GM = importlib.util.module_from_spec(spec)
spec.loader.exec_module(GM)


def elf(*versions):
    """Un ELF64 petit-boutiste dont la table de versions requises demande
    `versions` (« GLIBC_2.17 »…) à libc.so.6."""
    strtab = b"\0libc.so.6\0" + b"".join(v.encode() + b"\0" for v in versions)
    file_at = 1
    names, at = [], 1 + len("libc.so.6") + 1
    for v in versions:
        names.append(at)
        at += len(v) + 1
    aux = b""
    for i, name in enumerate(names):
        nxt = 16 if i < len(names) - 1 else 0
        aux += struct.pack("<IHHII", 0, 0, 2 + i, name, nxt)
    verneed = struct.pack("<HHIII", 1, len(versions), file_at, 16, 0) + aux
    body = verneed + strtab
    header = bytearray(64)
    header[0:4] = b"\x7fELF"
    header[4], header[5] = 2, 1
    shoff = 64 + len(body)
    struct.pack_into("<Q", header, 0x28, shoff)
    struct.pack_into("<HH", header, 0x3A, 64, 3)
    null = bytes(64)
    # Section 1 : .gnu.version_r (lien : section 2, info : 1 requête) ; 2 : .dynstr.
    need = struct.pack("<IIQQQQIIQQ", 0, GM.SHT_GNU_VERNEED, 0, 0, 64, len(verneed), 2, 1, 8, 0)
    dynstr = struct.pack("<IIQQQQIIQQ", 0, 3, 0, 0, 64 + len(verneed), len(strtab), 0, 0, 1, 0)
    return bytes(header) + body + null + need + dynstr


class GlibcMax(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="glibc-max-"))
        self.addCleanup(shutil.rmtree, self.root)

    def run_main(self, floor, *files):
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = GM.main([floor, *map(str, files)])
        return code, out.getvalue(), err.getvalue()

    def write(self, name, *versions):
        path = self.root / name
        path.write_bytes(elf(*versions))
        return path

    def test_the_highest_version_is_read(self):
        path = self.write("a", "GLIBC_2.17", "GLIBC_2.38", "GLIBC_2.5")
        self.assertEqual(max(GM.glibc_versions(path)), (2, 38))

    def test_a_binary_at_or_below_the_floor_passes(self):
        path = self.write("a", "GLIBC_2.17", "GLIBC_2.39")
        code, out, err = self.run_main("2.39", path)
        self.assertEqual(code, 0, err)
        self.assertIn("glibc 2.39 au plus", out)

    def test_a_binary_above_the_floor_fails_and_says_which(self):
        path = self.write("dysflow-app", "GLIBC_2.17", "GLIBC_2.41")
        code, _, err = self.run_main("2.39", path)
        self.assertEqual(code, 1)
        self.assertIn("dysflow-app demande glibc 2.41", err)

    def test_versions_compare_as_numbers_not_text(self):
        path = self.write("a", "GLIBC_2.9", "GLIBC_2.10")
        self.assertEqual(max(GM.glibc_versions(path)), (2, 10))
        code, _, _ = self.run_main("2.9", path)
        self.assertEqual(code, 1)

    def test_one_failing_file_fails_the_run(self):
        good = self.write("good", "GLIBC_2.17")
        bad = self.write("bad", "GLIBC_2.40")
        code, out, err = self.run_main("2.39", good, bad)
        self.assertEqual(code, 1)
        self.assertIn("good : glibc 2.17", out)
        self.assertIn("bad demande glibc 2.40", err)

    def test_a_file_without_glibc_symbols_does_not_pass_silently(self):
        path = self.write("a")
        code, _, err = self.run_main("2.39", path)
        self.assertEqual(code, 1)
        self.assertIn("ne demande aucune version de glibc", err)

    def test_not_an_elf_fails(self):
        path = self.root / "faux"
        path.write_bytes(b"MZ")
        code, _, err = self.run_main("2.39", path)
        self.assertEqual(code, 1)
        self.assertIn("pas un ELF", err)


if __name__ == "__main__":
    unittest.main()
