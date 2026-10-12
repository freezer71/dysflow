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
            "read_2x": {"coupures": 0},
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
        self.assertFalse(self.run_verdicts(ph)["temps occupé à 1× ≤ 10 %"])
        self.assertFalse(self.run_verdicts(peak=2.01)["empreinte ≤ 2 Go"])
        self.assertFalse(self.run_verdicts(sauts=(310, 320, 330))["premier son à chaud ≤ 0,3 s (médiane)"])
        self.assertFalse(self.run_verdicts(repos=451)["repos : empreinte de l'app sans voix ± 50 Mo"])
        ph = phases_ok(); ph["read_2x"]["coupures"] = 1
        self.assertFalse(self.run_verdicts(ph)["2× sans coupure"])
        ph = phases_ok(); ph["pause"]["appels_mlx"] = 1
        self.assertFalse(self.run_verdicts(ph)["pause : aucun appel à MLX"])
        ph = phases_ok(); ph["pause"]["cpu_cores_avg"] = 0.05
        self.assertFalse(self.run_verdicts(ph)["pause : CPU nul (< 0,01 cœur)"])
    def test_missing_data_fails(self):
        ph = {"read_1x": {}, "read_2x": {}, "pause": {}}
        extra = MC.derive(ph, [])
        v = MC.make_verdicts(ph, None, extra)
        self.assertFalse(any(v.values()), v)

DeriveHelper = Derive()

if __name__ == "__main__":
    unittest.main()
