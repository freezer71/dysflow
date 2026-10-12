#!/usr/bin/env python3
"""Filtre de charge de Sillon avec la méthode du banc : attente d'une machine
calme (load 1 min < 4), CPU de l'arbre de processus (proc_pid_rusage), GPU
`ioreg` moins le repos, empreinte physique. Les outils viennent de
.superpowers/research/eval/light_run.py (non versionné, présent sur la
machine de mesure). Rien n'est journalisé du texte lu.

  app   : Sillon.app --measure-charge (chemin MLX, critères de la spec)
  serve : l'exemple kokoro_serve (ORT, Linux) derrière light_run duty

Lancer avec l'interpréteur du banc (numpy, soundfile) :
  ~/Library/Caches/sillon-bench/light/venv-mlx/bin/python -I scripts/mesure-charge.py app \
      --app macos/build/Release/Sillon.app --out /tmp/sillon-charge.json
"""
import argparse, json, os, statistics, subprocess, sys, tempfile, time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", ".superpowers", "research", "eval"))
import light_run as LR  # noqa: E402

def parse_line(line):
    """Une ligne de l'app -> (type, contenu). PHASE nom ; COMPTEUR k=v… ;
    MESURE (nom, [valeur, unité]). Toute autre ligne : (None, None)."""
    parts = line.split()
    if not parts:
        return None, None
    if parts[0] == "PHASE" and len(parts) >= 2:
        return "PHASE", parts[1]
    if parts[0] == "COMPTEUR":
        return "COMPTEUR", {k: float(v) for k, v in (p.split("=") for p in parts[1:])}
    if parts[0] == "MESURE" and len(parts) >= 3:
        return "MESURE", (parts[1], parts[2:])
    return None, None

def derive(phases, mesures):
    """Chiffres tirés des lignes MESURE et des compteurs : sauts, empreintes,
    et coût de la copie à travers le pont UniFFI rapporté à la durée moyenne
    d'un passage MLX (phase read_1x)."""
    sauts = [float(v[0]) for n, v in mesures if n == "saut_loin_premier_son"]
    sans_voix = next((float(v[0]) for n, v in mesures if n == "empreinte_sans_voix"), None)
    repos = next((float(v[0]) for n, v in mesures if n == "empreinte_repos"), None)
    pont = {n: float(v[0]) for n, v in mesures if n.startswith("pont_copie_")}
    st = phases.get("read_1x", {})
    if st.get("appels_mlx") and st.get("forward_s") is not None:
        moyenne_ms = st["forward_s"] / st["appels_mlx"] * 1000
        st["synthese_moyenne_ms"] = round(moyenne_ms, 1)
        if "pont_copie_5s" in pont and moyenne_ms > 0:
            pont["pont_pour_cent_synthese"] = round(pont["pont_copie_5s"] / moyenne_ms * 100, 2)
    return {"sauts_ms": sauts, "sans_voix": sans_voix, "repos": repos, "pont": pont}

def make_verdicts(phases, peak, extra):
    sauts, sans_voix, repos = extra["sauts_ms"], extra["sans_voix"], extra["repos"]
    return {
        "temps occupé à 1× ≤ 10 %": phases["read_1x"].get("occupe", 1.0) <= 0.10,
        "2× sans coupure": phases["read_2x"].get("coupures", 1) == 0,
        "empreinte ≤ 2 Go": peak is not None and peak <= 2.0,
        "premier son à chaud ≤ 0,3 s (médiane)": bool(sauts) and statistics.median(sauts) <= 300,
        "pause : aucun appel à MLX": phases["pause"].get("appels_mlx", 1) == 0,
        "pause : CPU nul (< 0,01 cœur)": phases["pause"].get("cpu_cores_avg", 1.0) < 0.01,
        "repos : empreinte de l'app sans voix ± 50 Mo": sans_voix is not None and repos is not None
        and abs(repos - sans_voix) <= 50,
    }

def cmd_app(a):
    waited, load1 = LR.wait_calm(a.calm, a.calm_max_s)
    print(json.dumps({"attente_calme_s": waited, "load1": load1}), flush=True)
    exe = os.path.join(a.app, "Contents", "MacOS", "Sillon")
    proc = subprocess.Popen([exe, "--measure-charge"], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
    smp = LR.Sampler(LR.TreeCPU(proc.pid))
    smp.start()
    counters, mesures = {}, []
    for line in proc.stdout:
        kind, payload = parse_line(line)
        if kind == "PHASE":
            smp.phase = payload
        elif kind == "COMPTEUR":
            payload["t"] = time.time()
            counters.setdefault(smp.phase, []).append(payload)
        elif kind == "MESURE":
            mesures.append(payload)
            print(line.strip(), flush=True)
    proc.wait()
    smp.stop = True

    base = LR.phase_stats(smp.rows, "baseline")
    phases = {}
    for name in ("read_1x", "read_2x", "pause", "rest"):
        st = LR.phase_stats(smp.rows, name)
        c = counters.get(name, [])
        if len(c) >= 2:
            wall = c[-1]["t"] - c[0]["t"]
            st["occupe"] = round((c[-1]["busy_s"] - c[0]["busy_s"]) / wall, 3) if wall > 0 else None
            st["coupures"] = int(c[-1]["stalls"] - c[0]["stalls"])
            st["appels_mlx"] = int(c[-1]["mlx_calls"] - c[0]["mlx_calls"])
            if "forward_s" in c[0]:
                st["forward_s"] = round(c[-1]["forward_s"] - c[0]["forward_s"], 3)
        if st.get("gpu_sys_avg") is not None and base.get("gpu_sys_avg") is not None:
            st["gpu_sur_repos"] = round(st["gpu_sys_avg"] - base["gpu_sys_avg"], 1)
        phases[name] = st
    peak = max((r["fp_gb"] for r in smp.rows), default=None)
    extra = derive(phases, mesures)
    sauts, sans_voix, repos = extra["sauts_ms"], extra["sans_voix"], extra["repos"]
    verdicts = make_verdicts(phases, peak, extra)
    result = {"baseline": base, "phases": phases, "empreinte_pic_go": peak, "sauts_ms": sauts,
              "empreinte_sans_voix_mo": sans_voix, "empreinte_repos_mo": repos, "pont": extra["pont"], "verdicts": verdicts,
              "load1_depart": load1, "alerte_thermique": LR.therm_warn(), "echantillons": smp.rows}
    json.dump(result, open(a.out, "w"), indent=1, ensure_ascii=False)
    print(json.dumps({k: v for k, v in result.items() if k != "echantillons"}, ensure_ascii=False, indent=1))
    for name, ok in verdicts.items():
        print(f"{'OK ' if ok else 'NON'} {name}")
    return 0 if all(verdicts.values()) else 1

class SillonServe:
    """L'exemple kokoro_serve, vu comme un moteur de light_run."""
    def __init__(self, lang, cmd):
        import numpy as np
        self.np, self.sr, self.lang, self.desc = np, 24000, lang, "kokoro_serve (ORT int8)"
        self.p = subprocess.Popen(cmd, shell=True, stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
        json.loads(self.p.stdout.readline())
        self.tmp = tempfile.mktemp(suffix=".f32")
    def stream(self, text):
        self.p.stdin.write(json.dumps({"text": text, "lang": self.lang, "out": self.tmp}) + "\n")
        self.p.stdin.flush()
        json.loads(self.p.stdout.readline())
        yield self.np.fromfile(self.tmp, dtype=self.np.float32)
    def close(self):
        self.p.stdin.close()
        self.p.wait(10)

def cmd_serve(a):
    LR.load_backend = lambda args: (SillonServe(args.lang, a.serve), {})
    LR.cmd_duty(argparse.Namespace(backend="sillon_serve", opt=[], lang=a.lang, speeds="1,2", minutes=2,
                                   lookahead_s=10, idle_s=20, baseline_s=20, calm=a.calm,
                                   calm_max_s=a.calm_max_s, out=a.out))
    return 0

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("cmd", choices=["app", "serve"])
    ap.add_argument("--app", default="macos/build/Release/Sillon.app")
    ap.add_argument("--serve", default="target/release/examples/kokoro_serve")
    ap.add_argument("--lang", default="fr")
    ap.add_argument("--calm", type=float, default=4.0)
    ap.add_argument("--calm-max-s", type=float, default=1800)
    ap.add_argument("--out", required=True)
    a = ap.parse_args()
    sys.exit(cmd_app(a) if a.cmd == "app" else cmd_serve(a))

if __name__ == "__main__":
    main()
