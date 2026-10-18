#!/usr/bin/env python3
"""Écrit THIRD-PARTY-RUST.txt : le texte des licences de chaque crate Rust liée
statiquement dans ce que Sillage distribue.

    python3 scripts/third-party-rust.py            # écrit THIRD-PARTY-RUST.txt
    python3 scripts/third-party-rust.py --check    # échoue si le fichier n'est pas à jour

Deux livrables :
  (a) la bibliothèque de l'app macOS : sillage-ffi, avec les features de
      build-ffi.sh (ocr), pour aarch64-apple-darwin ;
  (b) le binaire du terminal : sillage, avec les features de package.sh
      (ocr,kokoro-onnx), pour Linux x86_64 et ARM64 et macOS.

Le graphe vient de `cargo tree -e normal,no-proc-macro` (dépendances liées
seulement : ni build, ni dev, ni macros procédurales, avec la résolution
exacte des features de chaque paquet) ; le dossier de chaque crate, de
`cargo metadata --filter-platform … --features …`. Les textes sont les
fichiers LICENSE*, LICENCE*, COPYING*, NOTICE*, COPYRIGHT*, UNLICENSE* livrés
par chaque crate. Une crate sans aucun fichier reçoit le texte standard de sa
licence, pris dans une autre crate du graphe, et elle est listée en tête.

Aucun outil à installer. Le script échoue sur une licence GPL, AGPL, LGPL ou
inconnue dans le graphe lié.
"""

import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "THIRD-PARTY-RUST.txt"

DELIVERABLES = [
    {
        "key": "app",
        "title": "bibliothèque de l'app macOS (sillage-ffi, features ocr)",
        "package": "sillage-ffi",
        "features": "ocr",
        "metadata_features": "sillage-ffi/ocr",
        "targets": ["aarch64-apple-darwin"],
    },
    {
        "key": "terminal",
        "title": "binaire du terminal (sillage, features ocr,kokoro-onnx)",
        "package": "sillage",
        "features": "ocr,kokoro-onnx",
        "metadata_features": "sillage/ocr,sillage/kokoro-onnx",
        "targets": ["x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu", "aarch64-apple-darwin"],
    },
]

# Licences acceptées telles quelles.
ACCEPTED = {"MIT", "Apache-2.0", "ISC", "Unicode-3.0", "Zlib", "MPL-2.0"}
# Licences permissives connues, acceptées et signalées dans le résumé.
ACCEPTED_NOTED = {
    "0BSD", "BSL-1.0", "CC0-1.0", "Unlicense", "Unicode-DFS-2016",
    "CDLA-Permissive-2.0", "Apache-2.0 WITH LLVM-exception",
}
REFUSED = re.compile(r"\b(A|L)?GPL", re.IGNORECASE)

LICENSE_FILE = re.compile(r"^(licen[cs]e|copying|notice|copyright|unlicense)", re.IGNORECASE)


def run(args):
    return subprocess.run(args, cwd=ROOT, check=True, capture_output=True, text=True).stdout


def linked_crates(d):
    """(nom, version) des crates liées, hors paquets du dépôt."""
    args = ["cargo", "tree", "-p", d["package"], "--features", d["features"],
            "-e", "normal,no-proc-macro", "--prefix", "none", "-f", "{p}"]
    for t in d["targets"]:
        args += ["--target", t]
    crates = set()
    for line in run(args).splitlines():
        line = line.strip().removesuffix("(*)").strip()
        if not line:
            continue
        parts = line.split(" ")
        name, version = parts[0], parts[1].lstrip("v")
        if len(parts) > 2 and parts[2].startswith("(/"):
            continue  # paquet du dépôt (sillage, sillage-ffi) : notre code
        crates.add((name, version))
    return crates


def package_info(d):
    info = {}
    for t in d["targets"]:
        meta = json.loads(run(["cargo", "metadata", "--format-version", "1",
                               "--filter-platform", t, "--features", d["metadata_features"]]))
        for p in meta["packages"]:
            info[(p["name"], p["version"])] = p
    return info


def license_atoms(expr):
    """Découpe une expression SPDX en alternatives (OR) de conjonctions (AND)."""
    expr = expr.replace("/", " OR ")
    expr = re.sub(r"\s+", " ", expr).strip()
    # Les parenthèses de la forme « (A OR B) AND C » : développer.
    def parse(s):
        s = s.strip()
        # retire des parenthèses englobantes
        while s.startswith("(") and matching(s, 0) == len(s) - 1:
            s = s[1:-1].strip()
        parts = split_top(s, " OR ")
        if len(parts) > 1:
            out = []
            for part in parts:
                out += parse(part)
            return out
        parts = split_top(s, " AND ")
        if len(parts) > 1:
            combos = [[]]
            for part in parts:
                combos = [c + alt for c in combos for alt in parse(part)]
            return combos
        return [[s]]
    return parse(expr)


def matching(s, i):
    depth = 0
    for j in range(i, len(s)):
        if s[j] == "(":
            depth += 1
        elif s[j] == ")":
            depth -= 1
            if depth == 0:
                return j
    return -1


def split_top(s, sep):
    parts, depth, start, i = [], 0, 0, 0
    while i < len(s):
        c = s[i]
        if c == "(":
            depth += 1
        elif c == ")":
            depth -= 1
        elif depth == 0 and s.startswith(sep, i):
            parts.append(s[start:i])
            i += len(sep)
            start = i
            continue
        i += 1
    parts.append(s[start:])
    return [p.strip() for p in parts]


def ok_atom(atom):
    return atom in ACCEPTED or atom in ACCEPTED_NOTED or atom.startswith("BSD-")


def judge(expr):
    """(accepté, licences notées) : une alternative entièrement acceptable suffit."""
    if not expr:
        return False, set()
    alternatives = license_atoms(expr)
    good = [alt for alt in alternatives if all(ok_atom(a) for a in alt)]
    if not good:
        return False, set()
    # Préférer une alternative sans licence notée.
    plain = [alt for alt in good if not any(a in ACCEPTED_NOTED for a in alt)]
    chosen = plain[0] if plain else good[0]
    return True, {a for a in chosen if a in ACCEPTED_NOTED}


def license_files(pkg):
    directory = Path(pkg["manifest_path"]).parent
    files = sorted(p for p in directory.iterdir() if p.is_file() and LICENSE_FILE.match(p.name))
    if pkg.get("license_file"):
        extra = directory / pkg["license_file"]
        if extra.is_file() and extra not in files:
            files.append(extra)
    return files


MIT_TEMPLATE = """MIT License

Copyright (c) {authors}

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE."""


def standard_texts(crates_files):
    """Les textes standard longs (Apache-2.0, MPL-2.0), pris tels quels dans
    les fichiers des crates du graphe, reconnus à leur contenu ; MIT vient du
    modèle ci-dessus, avec les auteurs de la crate."""
    found = {}
    for files in crates_files:
        for f in files:
            head = f.read_text(errors="replace").lstrip()[:400]
            if "MPL-2.0" not in found and head.startswith("Mozilla Public License Version 2.0"):
                found["MPL-2.0"] = f.read_text(errors="replace")
            if ("Apache-2.0" not in found and head.startswith("Apache License")
                    and "Version 2.0, January 2004" in head and re.search(r"apache", f.name, re.I)):
                found["Apache-2.0"] = f.read_text(errors="replace")
    found["MIT"] = None  # rempli par crate
    return found


def main():
    check = "--check" in sys.argv[1:]
    crates = {}  # (nom, version) -> ensemble des livrables
    info = {}
    for d in DELIVERABLES:
        info.update(package_info(d))
        for c in linked_crates(d):
            crates.setdefault(c, set()).add(d["key"])

    refused, noted, without_file = [], {}, []
    entries = []
    for key in sorted(crates, key=lambda c: (c[0].lower(), c[1])):
        pkg = info.get(key)
        if pkg is None:
            sys.exit(f"Erreur : {key[0]} {key[1]} absente de cargo metadata")
        expr = pkg.get("license") or ""
        accepted, notes = judge(expr)
        # Refusée : aucune alternative entièrement permissive (GPL, AGPL, LGPL
        # seules, licence inconnue ou non déclarée). Une alternative GPL est
        # tolérée quand une autre, permissive, est retenue.
        if not accepted:
            kind = "copyleft" if REFUSED.search(expr) else "inconnue"
            refused.append(f"{key[0]} {key[1]} : « {expr or 'licence non déclarée'} » ({kind})")
            continue
        for n in notes:
            noted.setdefault(n, []).append(f"{key[0]} {key[1]}")
        files = license_files(pkg)
        if not files:
            without_file.append(key)
        entries.append((key, pkg, expr, files))

    if refused:
        print("Erreur : licence refusée ou inconnue dans le graphe lié :", file=sys.stderr)
        for r in refused:
            print(f"  - {r}", file=sys.stderr)
        sys.exit(1)

    standards = standard_texts([e[3] for e in entries])
    out = []
    out.append("Sillage — licences des crates Rust liées statiquement")
    out.append("=" * 52)
    out.append("")
    out.append("Généré par scripts/third-party-rust.py à partir de Cargo.lock. Livrables :")
    for d in DELIVERABLES:
        out.append(f"  [{d['key']}] {d['title']}, cibles : {', '.join(d['targets'])}")
    out.append("")
    out.append(f"{len(entries)} crates. Chaque crate est marquée du ou des livrables qui la")
    out.append("contiennent. Les crates sous MPL-2.0 sont utilisées sans modification ;")
    out.append("leurs sources sont celles publiées sur https://crates.io à la version citée.")
    out.append("")
    if noted:
        out.append("Licences permissives signalées (acceptées) :")
        for n in sorted(noted):
            out.append(f"  {n} : {', '.join(noted[n])}")
        out.append("")
    if without_file:
        out.append("Crates sans fichier de licence (texte standard de leur licence joint) :")
        for name, version in without_file:
            out.append(f"  {name} {version}")
        out.append("")
    out.append("Résumé")
    out.append("------")
    for (name, version), pkg, expr, files in entries:
        tags = ",".join(sorted(crates[(name, version)]))
        out.append(f"  {name} {version} — {expr} [{tags}]")
    out.append("")
    for (name, version), pkg, expr, files in entries:
        out.append("")
        out.append("=" * 78)
        out.append(f"{name} {version} — {expr}")
        if pkg.get("repository"):
            out.append(f"Source : {pkg['repository']}")
        out.append("=" * 78)
        if files:
            for f in files:
                out.append("")
                out.append(f"--- {f.name} ---")
                out.append(f.read_text(errors="replace").rstrip())
        else:
            # L'alternative retenue : la première dont chaque licence a un
            # texte standard (MIT, puis Apache-2.0).
            good = [alt for alt in license_atoms(expr) if all(ok_atom(a) for a in alt)]
            good.sort(key=lambda alt: 0 if alt == ["MIT"] else 1)
            chosen = next((alt for alt in good if all(a in standards for a in alt)), None)
            if chosen is None:
                sys.exit(f"Erreur : {name} {version} sans fichier, et aucun texte standard pour « {expr} »")
            for spdx in chosen:
                text = standards[spdx]
                if spdx == "MIT":
                    authors = ", ".join(pkg.get("authors") or []) or f"les auteurs de {name}"
                    text = MIT_TEMPLATE.format(authors=authors)
                out.append("")
                out.append(f"--- texte standard {spdx} (la crate n'en livre aucun) ---")
                out.append(text.rstrip())
    text = "\n".join(out) + "\n"

    if check:
        if not OUT.is_file() or OUT.read_text() != text:
            sys.exit("Erreur : THIRD-PARTY-RUST.txt n'est pas à jour — python3 scripts/third-party-rust.py")
        print(f"THIRD-PARTY-RUST.txt à jour ({len(entries)} crates)")
        return
    OUT.write_text(text)
    print(f"Écrit : {OUT.name} ({len(entries)} crates, {len(without_file)} sans fichier de licence)")
    for n in sorted(noted):
        print(f"  signalée : {n} ({len(noted[n])})")


if __name__ == "__main__":
    main()
