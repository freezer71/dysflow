#!/usr/bin/env python3
"""Filtre de charge de Sillage avec la méthode du banc : attente d'une machine
calme (load 1 min < 3, sans processus ML, ≥ 40 % de RAM libre), CPU de l'arbre de processus (proc_pid_rusage), GPU
`ioreg` moins le repos, empreinte physique. Les outils viennent de
.superpowers/research/eval/light_run.py (non versionné, présent sur la
machine de mesure). Rien n'est journalisé du texte lu.

  app   : Sillage.app --measure-charge (chemin MLX, critères de la spec)
  serve : l'exemple kokoro_serve (ORT, Linux) derrière light_run duty

Lancer avec l'interpréteur du banc (numpy, soundfile) :
  ~/Library/Caches/sillage-bench/light/venv-mlx/bin/python -I scripts/mesure-charge.py app \
      --app macos/build/Mesure/Sillage.app --out target/runs/sillage-charge.json
"""
import argparse, json, os, re, statistics, subprocess, sys, tempfile, threading, time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", ".superpowers", "research", "eval"))
import light_run as LR  # noqa: E402

# Codes de sortie : 0 tout OK ; 1 au moins un NON ; 2 usage ; 3 conditions non
# calmes (rien n'a été mesuré) ; 4 mesure invalide (l'app a signalé une erreur) ;
# 5 délai global dépassé ou interruption.
EXIT_NON, EXIT_CALME, EXIT_INVALIDE, EXIT_DELAI = 1, 3, 4, 5
DEADLINE_S = 720
LOAD_MAX, RAM_LIBRE_MIN = 3.0, 40
ML_PROCS = re.compile(r"xcodebuild|cargo|rustc|swift-build|swift-frontend|ollama|llama|whisper|mlx|torch|python.*(tts|bench|eval)")

def calm_ok(load1, ml_procs, free_pct):
    """La règle unique de calme (celle de calme.sh) : load 1 min < 3, aucun
    processus ML ou de compilation, au moins 40 % de RAM libre."""
    return load1 < LOAD_MAX and not ml_procs and free_pct is not None and free_pct >= RAM_LIBRE_MIN

def _ancestors():
    out = subprocess.run(["ps", "-A", "-o", "pid=,ppid="], capture_output=True, text=True).stdout
    parent = {int(a): int(b) for a, b in (l.split() for l in out.splitlines() if len(l.split()) == 2)}
    pids, p = set(), os.getpid()
    while p and p not in pids:
        pids.add(p); p = parent.get(p, 0)
    return pids

def calm_state():
    skip = _ancestors()
    out = subprocess.run(["ps", "-A", "-o", "pid=,comm=,args="], capture_output=True, text=True).stdout
    procs = set()
    for l in out.splitlines():
        f = l.split(None, 2)
        if len(f) < 3 or int(f[0]) in skip or "guard.py" in f[2]:
            continue
        if ML_PROCS.search(f[1] + " " + f[2]):
            procs.add(os.path.basename(f[1]))
    mp = subprocess.run(["memory_pressure"], capture_output=True, text=True).stdout
    m = re.search(r"free percentage: (\d+)%", mp)
    return os.getloadavg()[0], sorted(procs), int(m.group(1)) if m else None

def wait_calm_strict(max_s):
    """Attend le calme (au plus max_s). Renvoie (calme, attente_s, état) ; ne
    relâche jamais la règle : sans calme, l'appelant ne mesure pas."""
    t0 = time.time()
    while True:
        load1, procs, free = calm_state()
        if calm_ok(load1, procs, free):
            return True, round(time.time() - t0), (load1, procs, free)
        if time.time() - t0 >= max_s:
            return False, round(time.time() - t0), (load1, procs, free)
        time.sleep(10)

def refuse_if_not_calm(a):
    ok, waited, (load1, procs, free) = wait_calm_strict(a.calm_max_s)
    info = {"attente_calme_s": waited, "load1": round(load1, 2), "processus": procs, "ram_libre_pct": free}
    print(json.dumps(info, ensure_ascii=False), flush=True)
    if not ok:
        print(f"conditions non calmes : load1={load1:.2f} (< {LOAD_MAX}), processus={procs or 'aucun'}, "
              f"RAM libre={free} % (≥ {RAM_LIBRE_MIN}) : rien n'a été mesuré", file=sys.stderr)
        sys.exit(EXIT_CALME)
    return info

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
        "temps occupé à 1× ≤ 10 % (MLX a travaillé : appels > 0)":
            phases["read_1x"].get("appels_mlx", 0) > 0 and phases["read_1x"].get("occupe", 1.0) <= 0.10,
        "2× sans coupure (MLX a travaillé : appels > 0)":
            phases["read_2x"].get("appels_mlx", 0) > 0 and phases["read_2x"].get("coupures", 1) == 0,
        "empreinte ≤ 2 Go": peak is not None and peak <= 2.0,
        "premier son à chaud ≤ 0,3 s (médiane)": bool(sauts) and statistics.median(sauts) <= 300,
        "pause : aucun appel à MLX": phases["pause"].get("appels_mlx", 1) == 0,
        "pause : CPU nul (< 0,01 cœur)": phases["pause"].get("cpu_cores_avg", 1.0) < 0.01,
        # + 100 Mo : la fragmentation malloc mesurée (16,8 Mo vivants, le reste
        # des pages libres du zone SMALL ; malloc_zone_pressure_relief rend 0).
        "repos : empreinte ≤ app sans voix + 100 Mo": sans_voix is not None and repos is not None
        and repos - sans_voix <= 100,
    }

def invalid_reason(mesures):
    """Une mesure n'a de sens que si l'app a annoncé une voix prête et aucune
    erreur : sinon elle n'a pas mesuré Kokoro MLX."""
    for n, v in mesures:
        if n == "erreur":
            return "l'app a signalé une erreur : " + " ".join(v)
    if not any(n == "voix" and v[:1] == ["ready"] for n, v in mesures):
        return "l'app n'a pas annoncé « voix ready »"
    return None

def stop_app(proc, grace=5):
    """Termine l'app (SIGTERM), puis la tue si elle ne part pas : jamais
    d'app qui continue de parler après le pilote."""
    if proc.poll() is not None:
        return
    proc.terminate()
    try:
        proc.wait(grace)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait()

def cmd_app(a):
    info = refuse_if_not_calm(a)
    load1 = info["load1"]
    exe = os.path.join(a.app, "Contents", "MacOS", "Sillage")
    errfile = open(a.out + ".stderr", "w")
    proc = subprocess.Popen([exe, "--measure-charge"], stdout=subprocess.PIPE, stderr=errfile, text=True)
    smp = None
    timed_out = threading.Event()
    def on_deadline():
        timed_out.set()
        stop_app(proc, grace=2)
    watchdog = threading.Timer(a.deadline_s, on_deadline)
    watchdog.daemon = True
    watchdog.start()
    counters, mesures = {}, []
    try:
        smp = LR.Sampler(LR.TreeCPU(proc.pid))
        smp.start()
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
    except BaseException:
        # Erreur Python ou Ctrl-C : l'app ne doit pas rester à parler.
        stop_app(proc)
        raise
    finally:
        watchdog.cancel()
        if smp is not None:
            smp.stop = True
        stop_app(proc)
        errfile.close()
    if timed_out.is_set():
        print(f"délai global de {a.deadline_s} s dépassé : app arrêtée, mesure abandonnée", file=sys.stderr)
        return EXIT_DELAI

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
    invalide = invalid_reason(mesures)
    result = {"baseline": base, "phases": phases, "empreinte_pic_go": peak, "sauts_ms": sauts,
              "empreinte_sans_voix_mo": sans_voix, "empreinte_repos_mo": repos, "pont": extra["pont"], "verdicts": verdicts,
              "load1_depart": load1, "calme": info, "mesure_invalide": invalide, "alerte_thermique": LR.therm_warn(), "echantillons": smp.rows}
    json.dump(result, open(a.out, "w"), indent=1, ensure_ascii=False)
    print(json.dumps({k: v for k, v in result.items() if k != "echantillons"}, ensure_ascii=False, indent=1))
    if invalide:
        print(f"MESURE INVALIDE : {invalide} (journal de l'app : {a.out}.stderr) : aucun verdict n'a de valeur", file=sys.stderr)
        return EXIT_INVALIDE
    for name, ok in verdicts.items():
        print(f"{'OK ' if ok else 'NON'} {name}")
    return 0 if all(verdicts.values()) else 1

class SillageServe:
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
    refuse_if_not_calm(a)
    LR.load_backend = lambda args: (SillageServe(args.lang, a.serve), {})
    LR.cmd_duty(argparse.Namespace(backend="sillage_serve", opt=[], lang=a.lang, speeds="1,2", minutes=2,
                                   lookahead_s=10, idle_s=20, baseline_s=20, calm=a.calm,
                                   calm_max_s=a.calm_max_s, out=a.out))
    return 0

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("cmd", choices=["app", "serve"])
    ap.add_argument("--app", default="macos/build/Mesure/Sillage.app")
    ap.add_argument("--serve", default="target/release/examples/kokoro_serve")
    ap.add_argument("--lang", default="fr")
    ap.add_argument("--calm", type=float, default=LOAD_MAX)
    ap.add_argument("--calm-max-s", type=float, default=600)
    ap.add_argument("--deadline-s", type=float, default=DEADLINE_S)
    ap.add_argument("--out", default="target/runs/sillage-charge.json")
    a = ap.parse_args()
    # Les relevés vont sous target/runs par défaut : le dossier peut manquer.
    os.makedirs(os.path.dirname(os.path.abspath(a.out)), exist_ok=True)
    sys.exit(cmd_app(a) if a.cmd == "app" else cmd_serve(a))

if __name__ == "__main__":
    main()
