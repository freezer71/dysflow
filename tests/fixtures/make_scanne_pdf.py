#!/usr/bin/env python3
"""Écrit tests/fixtures/scanne.pdf : une page sans couche texte, dont le
contenu n'existe que sous forme d'image — comme une photocopie.

L'image est capture.png recomprimée en JPEG et embarquée telle quelle sous
/DCTDecode, ce qui est la forme la plus courante d'un scan.
"""
import pathlib
import struct
import subprocess

here = pathlib.Path(__file__).parent
jpeg = here / "scanne.jpg"
subprocess.run(
    ["sips", "-s", "format", "jpeg", "-s", "formatOptions", "70",
     str(here / "capture.png"), "--out", str(jpeg)],
    check=True, stdout=subprocess.DEVNULL,
)
data = jpeg.read_bytes()
jpeg.unlink()

# Dimensions lues dans l'en-tête JPEG, pour dimensionner l'objet image.
i, width, height = 2, 0, 0
while i < len(data):
    if data[i] != 0xFF:
        i += 1
        continue
    marker = data[i + 1]
    if marker in (0xC0, 0xC1, 0xC2):
        height, width = struct.unpack(">HH", data[i + 5:i + 9])
        break
    if marker in (0xD8, 0xD9):
        i += 2
        continue
    i += 2 + struct.unpack(">H", data[i + 2:i + 4])[0]

objs = {
    1: b"<< /Type /Catalog /Pages 2 0 R >>",
    2: b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    3: b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R "
       b"/Resources << /XObject << /Im0 5 0 R >> >> >>",
}
content = b"q 612 0 0 792 0 0 cm /Im0 Do Q"
objs[4] = b"<< /Length %d >>\nstream\n" % len(content) + content + b"\nendstream"
objs[5] = (
    "<< /Type /XObject /Subtype /Image /Width %d /Height %d "
    "/ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode "
    "/Length %d >>" % (width, height, len(data))
).encode() + b"\nstream\n" + data + b"\nendstream"

buf = bytearray(b"%PDF-1.4\n")
offsets = {}
for num in sorted(objs):
    offsets[num] = len(buf)
    buf += b"%d 0 obj\n" % num + objs[num] + b"\nendobj\n"
xref = len(buf)
buf += b"xref\n0 %d\n" % (max(objs) + 1) + b"0000000000 65535 f \n"
for num in range(1, max(objs) + 1):
    buf += b"%010d 00000 n \n" % offsets.get(num, 0)
buf += b"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (
    max(objs) + 1, xref,
)

out = here / "scanne.pdf"
out.write_bytes(bytes(buf))
print("écrit %s : %d octets (image %dx%d)" % (out, len(buf), width, height))
