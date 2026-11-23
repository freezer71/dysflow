#!/usr/bin/env python3
"""Les DLL qu'importent les binaires Windows du paquet (PE32+), lues dans
leurs tables d'import, directe et différée. Aucun outil à installer.

    python3 scripts/windows-imports.py list FICHIER...
    python3 scripts/windows-imports.py deploy-crt DOSSIER_CRT DIST
    python3 scripts/windows-imports.py check [--sans-espeak] DIST

deploy-crt : copie, à côté de chaque binaire de DIST, les DLL du runtime
Visual C++ (DOSSIER_CRT : VC\\Redist\\MSVC\\<version>\\x64\\Microsoft.VC14x.CRT
de Visual Studio) qu'il importe, et celles qu'elles importent à leur tour.
Le déploiement local, à côté de dysflow.exe, est permis par la licence de
Visual Studio : un PC sans le redistribuable lance quand même dysflow.exe.

check : chaque DLL importée par un .exe ou un .dll de DIST, sous-dossiers
compris, est soit dans le dossier de ce binaire (le seul où Windows la
cherche avant System32), soit une DLL de Windows de la liste blanche
SYSTEM (ou un jeu d'API api-ms-win-*, ext-ms-win-*). Rien d'autre : une DLL
inconnue est une erreur, même présente dans le System32 de la machine qui
vérifie (elle peut manquer sur un autre PC). Aucune ne vient d'un
redistribuable (Visual C++, MinGW). dysflow-espeak.exe doit être là (sauf
--sans-espeak : la porte d'ONNX Runtime n'en a pas) ; lié statiquement, il
n'importe que KERNEL32, ADVAPI32, msvcrt, ucrtbase et api-ms-win-crt-*. Les
imports de chaque binaire sont affichés.
"""

import re
import shutil
import struct
import sys
from pathlib import Path

# Jamais une DLL de Windows : elles viennent d'un redistribuable, à joindre
# au paquet (Visual C++) ou à ne pas demander du tout (MinGW).
REDISTRIBUTABLE = re.compile(
    r"^(vcruntime|msvcp|concrt|vccorlib|vcomp|vcamp|mfc|libgcc|libstdc\+\+|libwinpthread|libgomp)",
    re.IGNORECASE,
)
# La liste blanche des DLL de Windows (en minuscules) : celles que
# Windows livre partout. Une DLL qui n'y est pas fait échouer `check`, sur
# Windows comme ailleurs : on l'ajoute ici en connaissance de cause.
SYSTEM = {
    "kernel32.dll", "kernelbase.dll", "ntdll.dll", "advapi32.dll", "user32.dll", "gdi32.dll",
    "shell32.dll", "ole32.dll", "oleaut32.dll", "ucrtbase.dll", "msvcrt.dll", "ws2_32.dll",
    "userenv.dll", "bcrypt.dll", "bcryptprimitives.dll", "crypt32.dll", "secur32.dll",
    "setupapi.dll", "dbghelp.dll", "dxgi.dll", "winmm.dll", "avrt.dll", "ksuser.dll",
    "propsys.dll", "shlwapi.dll", "comctl32.dll", "uxtheme.dll", "dwmapi.dll", "imm32.dll",
    "version.dll", "powrprof.dll", "iphlpapi.dll", "ncrypt.dll", "mmdevapi.dll",
}
# dysflow-espeak.exe : rien d'autre que le noyau et le CRT de Windows.
STATIC_ONLY = {"dysflow-espeak.exe"}
STATIC_ALLOWED = {"kernel32.dll", "advapi32.dll", "msvcrt.dll", "ucrtbase.dll"}


def imports(path):
    """Les DLL importées par le PE `path` (directes puis différées), dans l'ordre."""
    data = Path(path).read_bytes()
    if data[:2] != b"MZ":
        raise ValueError(f"{path} : pas un exécutable Windows")
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    if data[pe:pe + 4] != b"PE\0\0":
        raise ValueError(f"{path} : en-tête PE absent")
    sections_count = struct.unpack_from("<H", data, pe + 6)[0]
    optional_size = struct.unpack_from("<H", data, pe + 20)[0]
    optional = pe + 24
    magic = struct.unpack_from("<H", data, optional)[0]
    directories = optional + (112 if magic == 0x20B else 96)
    sections = []
    table = optional + optional_size
    for i in range(sections_count):
        s = table + 40 * i
        virtual_size, virtual_address, raw_size, raw_offset = struct.unpack_from("<IIII", data, s + 8)
        sections.append((virtual_address, max(virtual_size, raw_size), raw_offset))

    def offset(rva):
        for address, size, raw in sections:
            if address <= rva < address + size:
                return rva - address + raw
        raise ValueError(f"{path} : adresse {rva:#x} hors des sections")

    def name_at(rva):
        o = offset(rva)
        return data[o:data.index(b"\0", o)].decode("ascii")

    names = []
    # Répertoire 1 : imports (20 octets par DLL, nom au décalage 12) ;
    # répertoire 13 : imports différés (32 octets, nom au décalage 4).
    for index, size, name_field in ((1, 20, 12), (13, 32, 4)):
        rva = struct.unpack_from("<I", data, directories + 8 * index)[0]
        if rva == 0:
            continue
        o = offset(rva)
        while True:
            entry = data[o:o + size]
            if len(entry) < size or entry == bytes(size):
                break
            name_rva = struct.unpack_from("<I", entry, name_field)[0]
            if name_rva == 0:
                break
            name = name_at(name_rva)
            if name.lower() not in (n.lower() for n in names):
                names.append(name)
            o += size
    return names


def binaries(directory):
    """Les .exe et .dll de `directory`, sous-dossiers compris."""
    return sorted(p for p in Path(directory).rglob("*")
                  if p.is_file() and p.suffix.lower() in (".exe", ".dll"))


def is_system(name):
    """Une DLL de Windows de la liste blanche, ou un jeu d'API de Windows."""
    lower = name.lower()
    if REDISTRIBUTABLE.match(lower):
        return False
    return lower.startswith(("api-ms-win-", "ext-ms-win-")) or lower in SYSTEM


def deploy_crt(crt, dist):
    available = {p.name.lower(): p for p in Path(crt).iterdir() if p.suffix.lower() == ".dll"}
    if not available:
        sys.exit(f"Erreur : aucune DLL dans {crt}")
    copied = []
    pending = binaries(dist)
    while pending:
        binary = pending.pop()
        for name in imports(binary):
            source = available.get(name.lower())
            target = binary.parent / source.name if source else None
            if source and not target.exists():
                shutil.copy2(source, target)
                copied.append(target.relative_to(dist).as_posix())
                pending.append(target)
    print("Runtime Visual C++ joint : " + (", ".join(sorted(copied)) or "rien à joindre"))


def check(dist, espeak=True):
    errors = []
    found = binaries(dist)
    if espeak and not any(p.name.lower() in STATIC_ONLY for p in found):
        errors.append(f"dysflow-espeak.exe absent de {dist}")
    for binary in found:
        beside = {p.name.lower() for p in binary.parent.iterdir() if p.is_file()}
        names = imports(binary)
        shown = binary.relative_to(dist).as_posix()
        print(f"{shown} : {', '.join(names)}")
        for name in names:
            lower = name.lower()
            if binary.name.lower() in STATIC_ONLY:
                if not (lower in STATIC_ALLOWED or lower.startswith("api-ms-win-crt-")):
                    errors.append(f"{shown} importe {name} (il doit être lié statiquement)")
            elif lower not in beside and not is_system(name):
                errors.append(f"{shown} importe {name}, ni à côté de lui ni DLL de Windows de la liste blanche")
    if errors:
        sys.stdout.flush()
        for e in errors:
            print(f"Erreur : {e}", file=sys.stderr)
        sys.exit(1)
    print("Imports : tous dans le paquet ou dans Windows")


def main():
    args = sys.argv[1:]
    if len(args) >= 2 and args[0] == "list":
        for f in args[1:]:
            print(f"{Path(f).name} : {', '.join(imports(f))}")
    elif len(args) == 3 and args[0] == "deploy-crt":
        deploy_crt(args[1], args[2])
    elif len(args) == 2 and args[0] == "check":
        check(args[1])
    elif len(args) == 3 and args[0] == "check" and args[1] == "--sans-espeak":
        check(args[2], espeak=False)
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
