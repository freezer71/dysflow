#!/usr/bin/env python3
"""Écrit tests/fixtures/spec.pdf : deux pages accentuées, sans dépendance.

Le PDF est écrit à la main plutôt que produit par un outil : la fixture doit
rester reproductible à l'octet près sur les trois plateformes cibles.
"""
import pathlib


def esc(s):
    return s.replace("\\", "\\\\").replace("(", "\\(").replace(")", "\\)")


def page_stream(lines):
    out = ["BT", "/F1 12 Tf", "72 720 Td", "14 TL"]
    for i, line in enumerate(lines):
        if i:
            out.append("T*")
        out.append("(%s) Tj" % esc(line))
    out.append("ET")
    return "\n".join(out).encode("cp1252")  # WinAnsiEncoding


PAGE1 = [
    "Spécification — Passerelle de paiement",
    "",
    "1. Contexte",
    "",
    "Le système doit gérer les règlements différés, puis",
    "confirmer chaque opération.",
]
PAGE2 = [
    "2. Contraintes",
    "",
    "La latence maximale est fixée à 250 ms.",
    "Les montants sont exprimés en euros.",
]

objs = {
    1: b"<< /Type /Catalog /Pages 2 0 R >>",
    2: b"<< /Type /Pages /Kids [3 0 R 6 0 R] /Count 2 >>",
    3: b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R "
       b"/Resources << /Font << /F1 5 0 R >> >> >>",
    5: b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica "
       b"/Encoding /WinAnsiEncoding >>",
    6: b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 7 0 R "
       b"/Resources << /Font << /F1 5 0 R >> >> >>",
}
for num, page in ((4, PAGE1), (7, PAGE2)):
    body = page_stream(page)
    objs[num] = b"<< /Length %d >>\nstream\n" % len(body) + body + b"\nendstream"

buf = bytearray(b"%PDF-1.4\n")
offsets = {}
for num in sorted(objs):
    offsets[num] = len(buf)
    buf += b"%d 0 obj\n" % num + objs[num] + b"\nendobj\n"

xref = len(buf)
buf += b"xref\n0 %d\n" % (len(objs) + 1)
buf += b"0000000000 65535 f \n"
for num in sorted(objs):
    buf += b"%010d 00000 n \n" % offsets[num]
buf += b"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (
    len(objs) + 1,
    xref,
)

out = pathlib.Path(__file__).with_name("spec.pdf")
out.write_bytes(bytes(buf))
print("écrit %s : %d octets" % (out, len(buf)))
