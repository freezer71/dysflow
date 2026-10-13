"""Tests du pilote de charge : analyse des lignes et verdicts, sans app, sans
mesure, sans numpy (light_run est remplacé par un double)."""
import importlib.util, os, sys, types, unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.modules.setdefault("light_run", types.ModuleType("light_run"))
spec = importlib.util.spec_from_file_location("mesure_charge", os.path.join(HERE, "mesure-charge.py"))
MC = importlib.util.module_from_spec(spec)
spec.loader.exec_module(MC)

def phases_ok():
    return {"read_1x": {"occupe": 0.07, "appels_mlx": 100, "forward_s": 6.0},
            "read_2x": {"coupures": 0, "appels_mlx": 190},
            "pause": {"appels_mlx": 0, "cpu_cores_avg": 0.0}}

class ParseLine(unittest.TestCase):
    def test_phase(self):
        self.assertEqual(MC.parse_line("PHASE read_1x\n"), ("PHASE", "read_1x"))
    def test_compteur(self):
        k, v = MC.parse_line("COMPTEUR busy_s=1.500 stalls=0 mlx_calls=12 forward_s=0.900")
        self.assertEqual(k, "COMPTEUR")
        self.assertEqual(v, {"busy_s": 1.5, "stalls": 0.0, "mlx_calls": 12.0, "forward_s": 0.9})
    def test_mesure(self):
        self.assertEqual(MC.parse_line("MESURE saut_loin_premier_son 120 ms"),
                         ("MESURE", ("saut_loin_premier_son", ["120", "ms"])))
    def test_bruit(self):
        self.assertEqual(MC.parse_line("2026 autre chose"), (None, None))
        self.assertEqual(MC.parse_line("\n"), (None, None))

class Derive(unittest.TestCase):
    def mesures(self, sauts=(100, 120, 140), sans=400, repos=420):
        m = [("saut_loin_premier_son", [str(s), "ms"]) for s in sauts]
        m += [("empreinte_sans_voix", [str(sans), "Mo"]), ("empreinte_repos", [str(repos), "Mo"]),
              ("pont_copie_5s", ["0.6", "ms"])]
        return m
    def test_bridge_share_of_synthesis(self):
        ph = phases_ok()
        extra = MC.derive(ph, self.mesures())
        self.assertEqual(ph["read_1x"]["synthese_moyenne_ms"], 60.0)
        self.assertEqual(extra["pont"]["pont_pour_cent_synthese"], 1.0)
    def test_no_calls_no_share(self):
        ph = phases_ok(); ph["read_1x"]["appels_mlx"] = 0
        self.assertNotIn("pont_pour_cent_synthese", MC.derive(ph, self.mesures())["pont"])

class Verdicts(unittest.TestCase):
    def run_verdicts(self, ph=None, peak=1.4, **kw):
        ph = ph or phases_ok()
        extra = MC.derive(ph, DeriveHelper.mesures(**kw))
        return MC.make_verdicts(ph, peak, extra)
    def test_all_pass(self):
        v = self.run_verdicts()
        self.assertEqual(len(v), 7)
        self.assertTrue(all(v.values()), v)
    def test_threshold_is_never_relaxed(self):
        ph = phases_ok(); ph["read_1x"]["occupe"] = 0.101
        self.assertFalse([ok for n, ok in self.run_verdicts(ph).items() if n.startswith("temps occupé")][0])
        self.assertFalse(self.run_verdicts(peak=2.01)["empreinte ≤ 2 Go"])
        self.assertFalse(self.run_verdicts(sauts=(310, 320, 330))["premier son à chaud ≤ 0,3 s (médiane)"])
        self.assertFalse(self.run_verdicts(repos=501)["repos : empreinte ≤ app sans voix + 100 Mo"])
        ph = phases_ok(); ph["read_2x"]["coupures"] = 1
        self.assertFalse([ok for n, ok in self.run_verdicts(ph).items() if n.startswith("2× sans coupure")][0])
        ph = phases_ok(); ph["pause"]["appels_mlx"] = 1
        self.assertFalse(self.run_verdicts(ph)["pause : aucun appel à MLX"])
        ph = phases_ok(); ph["pause"]["cpu_cores_avg"] = 0.05
        self.assertFalse(self.run_verdicts(ph)["pause : CPU nul (< 0,01 cœur)"])
    def test_rest_footprint_allows_up_to_100_mb_above_and_any_amount_below(self):
        # Fragmentation malloc mesurée : 16,8 Mo vivants, le reste en pages
        # libres du zone SMALL que macOS ne reprend pas tout de suite.
        label = "repos : empreinte ≤ app sans voix + 100 Mo"
        self.assertTrue(self.run_verdicts(sans=400, repos=500)[label])
        self.assertFalse(self.run_verdicts(sans=400, repos=501)[label])
        self.assertTrue(self.run_verdicts(sans=400, repos=300)[label], "sous l'app sans voix : rien de retenu")
    def test_a_run_that_never_touched_mlx_is_not_ok(self):
        for phase, label in (("read_1x", "temps occupé à 1×"), ("read_2x", "2× sans coupure")):
            ph = phases_ok(); ph[phase]["appels_mlx"] = 0
            v = self.run_verdicts(ph)
            self.assertFalse([ok for n, ok in v.items() if n.startswith(label)][0], phase)
            ph = phases_ok(); del ph[phase]["appels_mlx"]
            v = self.run_verdicts(ph)
            self.assertFalse([ok for n, ok in v.items() if n.startswith(label)][0], phase)
    def test_a_zero_cpu_pause_is_ok(self):
        ph = phases_ok(); ph["pause"]["cpu_cores_avg"] = 0.0
        self.assertTrue(self.run_verdicts(ph)["pause : CPU nul (< 0,01 cœur)"])
    def test_missing_data_fails(self):
        ph = {"read_1x": {}, "read_2x": {}, "pause": {}}
        extra = MC.derive(ph, [])
        v = MC.make_verdicts(ph, None, extra)
        self.assertFalse(any(v.values()), v)

class Calm(unittest.TestCase):
    def test_rule(self):
        self.assertTrue(MC.calm_ok(2.9, [], 40))
        self.assertFalse(MC.calm_ok(3.0, [], 80))
        self.assertFalse(MC.calm_ok(1.0, ["xcodebuild"], 80))
        self.assertFalse(MC.calm_ok(1.0, [], 39))
        self.assertFalse(MC.calm_ok(1.0, [], None))
    def test_not_calm_refuses_with_a_distinct_exit_code(self):
        MC.wait_calm_strict, saved = (lambda max_s: (False, 0, (5.0, ["cargo"], 50))), MC.wait_calm_strict
        try:
            a = types.SimpleNamespace(calm_max_s=0)
            with self.assertRaises(SystemExit) as cm:
                MC.refuse_if_not_calm(a)
            self.assertEqual(cm.exception.code, MC.EXIT_CALME)
            self.assertNotIn(MC.EXIT_CALME, (0, MC.EXIT_NON, MC.EXIT_INVALIDE, MC.EXIT_DELAI))
        finally:
            MC.wait_calm_strict = saved

class Invalid(unittest.TestCase):
    def test_needs_a_ready_voice_and_no_error(self):
        ok = [("voix", ["ready"])]
        self.assertIsNone(MC.invalid_reason(ok))
        self.assertIsNotNone(MC.invalid_reason([]))
        self.assertIsNotNone(MC.invalid_reason(ok + [("erreur", ["voix-non-kokoro-mlx-fallback"])]))

DeriveHelper = Derive()

if __name__ == "__main__":
    unittest.main()
