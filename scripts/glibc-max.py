#!/usr/bin/env python3
"""La version de glibc la plus haute qu'un binaire ELF Linux demande
(les symboles `GLIBC_x.y` de sa table de versions requises), et un refus si
elle dépasse le plancher du paquet.

    python3 -I scripts/glibc-max.py PLANCHER FICHIER...

PLANCHER : « 2.39 », la version de glibc des notes de publication et du README
(Ubuntu 24.04). Un binaire qui demande plus (GPUI, Vulkan, xkbcommon…) fait
échouer le contrôle : le plancher ne se relève pas en silence, c'est une
décision (spec « interface GPUI », risques). ELF 64 bits petit-boutiste
(x86_64, ARM64), comme elf-version-ref.py. Aucun outil à installer.
"""

import re
import struct
import sys
from pathlib import Path

SHT_GNU_VERNEED = 0x6FFFFFFE


def cstring(data: bytes, offset: int) -> bytes:
    return data[offset:data.index(b"\0", offset)]


def glibc_versions(path):
    """Les versions `GLIBC_x.y[.z]` demandées par l'ELF `path`, en tuples."""
    data = Path(path).read_bytes()
    if data[:4] != b"\x7fELF" or data[4] != 2 or data[5] != 1:
        raise ValueError(f"{path} : pas un ELF 64 bits petit-boutiste")
    shoff, = struct.unpack_from("<Q", data, 0x28)
    shentsize, shnum = struct.unpack_from("<HH", data, 0x3A)
    sections = [struct.unpack_from("<IIQQQQIIQQ", data, shoff + i * shentsize) for i in range(shnum)]
    versions = set()
    for _name, sh_type, _flags, _addr, offset, _size, link, info, _align, _entsize in sections:
        if sh_type != SHT_GNU_VERNEED:
            continue
        strtab = sections[link][4]
        need = offset
        for _ in range(info):
            _version, count, _file, aux, nxt = struct.unpack_from("<HHIII", data, need)
            entry = need + aux
            for _ in range(count):
                _hash, _flags, _other, name, vna_next = struct.unpack_from("<IHHII", data, entry)
                found = re.fullmatch(r"GLIBC_(\d+(?:\.\d+)+)", cstring(data, strtab + name).decode())
                if found:
                    versions.add(tuple(int(n) for n in found.group(1).split(".")))
                entry += vna_next
            need += nxt
    return versions


def main(args):
    if len(args) < 2 or not re.fullmatch(r"\d+(\.\d+)+", args[0]):
        print(__doc__.strip().splitlines()[2].strip(), file=sys.stderr)
        return 2
    floor = tuple(int(n) for n in args[0].split("."))
    failed = False
    for path in args[1:]:
        try:
            versions = glibc_versions(path)
        except ValueError as error:
            print(f"Erreur : {error}", file=sys.stderr)
            return 1
        top = max(versions, default=None)
        name = Path(path).name
        if top is None:
            print(f"Erreur : {name} ne demande aucune version de glibc (pas un binaire Linux dynamique ?)", file=sys.stderr)
            failed = True
        elif top > floor:
            print(f"Erreur : {name} demande glibc {'.'.join(map(str, top))}, au-dessus du plancher {args[0]} "
                  "du paquet. Ne pas relever le plancher sans décision : voir la spec « interface GPUI ».",
                  file=sys.stderr)
            failed = True
        else:
            print(f"{name} : glibc {'.'.join(map(str, top))} au plus (plancher {args[0]})")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
