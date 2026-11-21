#!/usr/bin/env python3
"""Renomme une version de symbole qu'une bibliothèque ELF demande à une autre.

    python3 -I scripts/elf-version-ref.py FICHIER BIBLIOTHÈQUE ANCIENNE NOUVELLE

Exemple (scripts/sherpa-onnx.sh) : la bibliothèque C de sherpa-onnx 2.0.2,
publiée pour ONNX Runtime 1.28.2, demande `OrtGetApiBase@VERS_1.28.2` à
libonnxruntime.so. ONNX Runtime marque ses symboles d'une seule version, la
sienne : ld.so refuse de la charger avec le 1.31.0 (« version `VERS_1.28.2'
not found »). La requête est renommée en VERS_1.31.0, rien d'autre : même
longueur, même place dans .dynstr, l'empreinte (vna_hash) recalculée.

C'est sûr parce que la bibliothèque ne prend à ONNX Runtime que
`OrtGetApiBase` ; elle demande ensuite l'API par son numéro, qu'ONNX Runtime
garde pour les versions suivantes. Sans effet si la requête porte déjà la
nouvelle version. ELF 64 bits petit-boutiste seulement (x86_64, ARM64).
"""

import struct
import sys
from pathlib import Path

SHT_GNU_VERNEED = 0x6FFFFFFE


def elf_hash(name: bytes) -> int:
    h = 0
    for c in name:
        h = ((h << 4) + c) & 0xFFFFFFFF
        g = h & 0xF0000000
        if g:
            h ^= g >> 24
        h &= ~g & 0xFFFFFFFF
    return h


def cstring(data: bytes, offset: int) -> bytes:
    return data[offset:data.index(b"\0", offset)]


def main() -> int:
    if len(sys.argv) != 5:
        print(__doc__.strip().splitlines()[2].strip(), file=sys.stderr)
        return 2
    path = Path(sys.argv[1])
    library, old, new = (a.encode() for a in sys.argv[2:])
    if len(old) != len(new):
        print(f"Erreur : {old!r} et {new!r} n'ont pas la même longueur", file=sys.stderr)
        return 1
    data = bytearray(path.read_bytes())
    if data[:4] != b"\x7fELF" or data[4] != 2 or data[5] != 1:
        print(f"Erreur : {path} n'est pas un ELF 64 bits petit-boutiste", file=sys.stderr)
        return 1
    shoff, = struct.unpack_from("<Q", data, 0x28)
    shentsize, shnum = struct.unpack_from("<HH", data, 0x3A)
    sections = [struct.unpack_from("<IIQQQQIIQQ", data, shoff + i * shentsize) for i in range(shnum)]

    found = 0
    for _name, sh_type, _flags, _addr, offset, _size, link, info, _align, _entsize in sections:
        if sh_type != SHT_GNU_VERNEED:
            continue
        strtab = sections[link][4]
        need = offset
        for _ in range(info):
            _version, count, file, aux, nxt = struct.unpack_from("<HHIII", data, need)
            if cstring(data, strtab + file) == library:
                entry = need + aux
                for _ in range(count):
                    vna_hash, flags, other, name, vna_next = struct.unpack_from("<IHHII", data, entry)
                    current = cstring(data, strtab + name)
                    if current == new:
                        found += 1
                    elif current == old:
                        if vna_hash != elf_hash(old):
                            print(f"Erreur : empreinte inattendue pour {old.decode()}", file=sys.stderr)
                            return 1
                        data[strtab + name:strtab + name + len(new)] = new
                        struct.pack_into("<IHHII", data, entry, elf_hash(new), flags, other, name, vna_next)
                        found += 1
                    entry += vna_next
            need += nxt
    if found == 0:
        print(f"Erreur : {path} ne demande ni {old.decode()} ni {new.decode()} à {library.decode()}", file=sys.stderr)
        return 1
    path.write_bytes(data)
    print(f"{path.name} : {library.decode()} {new.decode()}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
