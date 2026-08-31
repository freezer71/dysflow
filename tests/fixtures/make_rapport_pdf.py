#!/usr/bin/env python3
"""Écrit tests/fixtures/rapport.pdf : trois pages portant chaque défaut que le
lot B doit corriger — en-tête et pied courants, césures, titres numérotés,
liste à puces, phrase coupée par un saut de page.
"""
import pathlib


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


PAGES = [
    ["Spécification technique — Confidentiel", "",
     "1. Contexte", "",
     "Le service de paiement doit gérer les règlements diffé-",
     "rés sur trente jours. Chaque opération est enregistrée",
     "à l'état « en attente », puis confirmée par le presta-",
     "taire externe.", "",
     "Page 1 sur 3"],
    ["Spécification technique — Confidentiel", "",
     "2. Contraintes", "",
     "Les contraintes retenues sont les suivantes :", "",
     "• latence maximale de 250 ms",
     "• disponibilité de 99,9 %",
     "• journalisation intégrale", "",
     "2.1 Latence", "",
     "La mesure est prise au percentile 95. Le seuil ne",
     "s'applique pas aux opérations de",
     "Page 2 sur 3"],
    ["Spécification technique — Confidentiel", "",
     "remboursement, traitées en différé.", "",
     "3. Interfaces", "",
     "L'API expose deux routes documentées.", "",
     "Page 3 sur 3"],
]

objs = {
    1: b"<< /Type /Catalog /Pages 2 0 R >>",
    5: b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica "
       b"/Encoding /WinAnsiEncoding >>",
}
kids = " ".join("%d 0 R" % (10 + i * 2) for i in range(len(PAGES)))
objs[2] = ("<< /Type /Pages /Kids [%s] /Count %d >>" % (kids, len(PAGES))).encode()
for i, page in enumerate(PAGES):
    pid, cid = 10 + i * 2, 11 + i * 2
    objs[pid] = (
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents %d 0 R "
        "/Resources << /Font << /F1 5 0 R >> >> >>" % cid
    ).encode()
    body = stream(page)
    objs[cid] = b"<< /Length %d >>\nstream\n" % len(body) + body + b"\nendstream"

buf = bytearray(b"%PDF-1.4\n")
offsets = {}
for num in sorted(objs):
    offsets[num] = len(buf)
    buf += b"%d 0 obj\n" % num + objs[num] + b"\nendobj\n"

xref = len(buf)
buf += b"xref\n0 %d\n" % (max(objs) + 1)
buf += b"0000000000 65535 f \n"
for num in range(1, max(objs) + 1):
    buf += b"%010d 00000 n \n" % offsets.get(num, 0)
buf += b"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (
    max(objs) + 1,
    xref,
)

out = pathlib.Path(__file__).with_name("rapport.pdf")
out.write_bytes(bytes(buf))
print("écrit %s : %d octets" % (out, len(buf)))
