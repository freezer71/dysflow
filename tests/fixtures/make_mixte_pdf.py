#!/usr/bin/env python3
"""Écrit tests/fixtures/mixte.pdf : une page de texte, puis une page qui n'est
qu'une image — la couverture photographiée d'un rapport par ailleurs textuel.

L'image est celle de scanne.pdf, reprise telle quelle : le cas à vérifier est
qu'une page image ne fasse pas refuser tout le document.
"""
import pathlib
import re

here = pathlib.Path(__file__).parent
scanned = (here / "scanne.pdf").read_bytes()
m = re.search(rb"/Width (\d+) /Height (\d+) .*?/Length (\d+) >>\nstream\n", scanned, re.S)
width, height, length = int(m.group(1)), int(m.group(2)), int(m.group(3))
jpeg = scanned[m.end():m.end() + length]


def esc(s):
    return s.replace("\\", "\\\\").replace("(", "\\(").replace(")", "\\)")


def stream(lines):
    out = ["BT", "/F1 11 Tf", "60 740 Td", "13 TL"]
    for i, line in enumerate(lines):
        if i:
            out.append("T*")
        out.append("(%s) Tj" % esc(line))
    out.append("ET")
    return "\n".join(out).encode("cp1252")


text = stream([
    "1. Contexte", "",
    "Le service de paiement doit gérer les règlements différés.",
    "Chaque opération est enregistrée puis confirmée.",
])
draw = b"q 612 0 0 792 0 0 cm /Im0 Do Q"

objs = {
    1: b"<< /Type /Catalog /Pages 2 0 R >>",
    2: b"<< /Type /Pages /Kids [3 0 R 6 0 R] /Count 2 >>",
    3: b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R "
       b"/Resources << /Font << /F1 5 0 R >> >> >>",
    4: b"<< /Length %d >>\nstream\n" % len(text) + text + b"\nendstream",
    5: b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
    6: b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 7 0 R "
       b"/Resources << /XObject << /Im0 8 0 R >> >> >>",
    7: b"<< /Length %d >>\nstream\n" % len(draw) + draw + b"\nendstream",
    8: ("<< /Type /XObject /Subtype /Image /Width %d /Height %d "
        "/ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode "
        "/Length %d >>" % (width, height, len(jpeg))).encode()
       + b"\nstream\n" + jpeg + b"\nendstream",
}

buf = bytearray(b"%PDF-1.4\n")
offsets = {}
for num in sorted(objs):
    offsets[num] = len(buf)
    buf += b"%d 0 obj\n" % num + objs[num] + b"\nendobj\n"
xref = len(buf)
buf += b"xref\n0 %d\n" % (max(objs) + 1) + b"0000000000 65535 f \n"
for num in range(1, max(objs) + 1):
    buf += b"%010d 00000 n \n" % offsets.get(num, 0)
buf += b"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (max(objs) + 1, xref)

out = here / "mixte.pdf"
out.write_bytes(bytes(buf))
print("écrit %s : %d octets" % (out, len(buf)))
