"""Tests du lecteur de traces ; ces lignes sont des fixtures, pas des mesures."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("audio_durability_check", Path(__file__).resolve().parents[1] / "check-audio-durability.py")
assert spec is not None and spec.loader is not None
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)


class DurabilityTraceTests(unittest.TestCase):
    def setUp(self):
        self.lines = [
            "1 fsync(3</fixture/tranche-00000000.wav.part-1-0>) = 0",
            '1 rename("/fixture/tranche-00000000.wav.part-1-0", "/fixture/tranche-00000000.wav") = 0',
            "1 fsync(3</fixture>) = 0",
            "1 fsync(3</fixture/tranche-00000000.audio.json.part-1-1>) = 0",
            '1 rename("/fixture/tranche-00000000.audio.json.part-1-1", "/fixture/tranche-00000000.audio.json") = 0',
            "1 fsync(3</fixture>) = 0",
            "1 fsync(3</fixture/travail.tmp>) = 0",
            '1 rename("/fixture/travail.tmp", "/fixture/travail.json") = 0',
        ]

    def test_accepte_lordre_complet(self):
        self.assertTrue(checker.verify_trace("\n".join(self.lines))["before_job_confirmation"])

    def test_chaque_etape_manquante_est_refusee(self):
        for index in range(len(self.lines)):
            with self.subTest(index=index), self.assertRaises(ValueError):
                checker.verify_trace("\n".join(self.lines[:index] + self.lines[index + 1:]))

    def test_refuse_la_confirmation_avant_la_barriere(self):
        lines = self.lines.copy()
        lines[5], lines[6] = lines[6], lines[5]
        with self.assertRaises(ValueError):
            checker.verify_trace("\n".join(lines))

    def test_refuse_une_barriere_en_erreur_ou_un_autre_dossier(self):
        for altered in [self.lines[5].replace("= 0", "= -1 EIO"), self.lines[5].replace("/fixture", "/ailleurs")]:
            lines = self.lines.copy()
            lines[5] = altered
            with self.assertRaises(ValueError):
                checker.verify_trace("\n".join(lines))


if __name__ == "__main__":
    unittest.main()
