import copy
import importlib.util
import json
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location('topic_fixture', ROOT / 'scripts/check-topic-fixture.py')
assert SPEC is not None and SPEC.loader is not None
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)

class TopicFixtureTest(unittest.TestCase):
    def setUp(self):
        self.data = json.loads(CHECK.FIXTURE.read_text(encoding='utf-8'))

    def test_fixture_has_the_frozen_identity_and_counts(self):
        self.assertEqual(CHECK.validate_file(), {'cases': 30, 'development': 12, 'holdout': 18,
            'single': 20, 'multiple': 3, 'abstain': 7, 'critical': 15})

    def test_oracle_cannot_accept_an_unoffered_topic(self):
        changed = copy.deepcopy(self.data)
        changed['cases'][0]['expected']['topic_ids'] = ['unknown-project']
        with self.assertRaisesRegex(ValueError, 'hors des candidats'):
            CHECK.validate(changed)

    def test_case_identifiers_are_unique(self):
        self.data['cases'][1]['id'] = self.data['cases'][0]['id']
        with self.assertRaisesRegex(ValueError, 'Identifiant de cas'):
            CHECK.validate(self.data)

    def test_abstention_cannot_contain_a_firm_assignment(self):
        entry = next(c for c in self.data['cases'] if c['expected']['kind'] == 'abstain')
        entry['expected']['topic_ids'] = [entry['candidate_topic_ids'][0]]
        with self.assertRaisesRegex(ValueError, 'Abstention'):
            CHECK.validate(self.data)

    def test_synthetic_and_critical_flags_have_strict_types(self):
        self.data['cases'][0]['synthetic'] = 1
        with self.assertRaisesRegex(ValueError, 'fictif'):
            CHECK.validate(self.data)

    def test_no_silent_duplicate_candidates(self):
        self.data['cases'][0]['candidate_topic_ids'].append(self.data['cases'][0]['candidate_topic_ids'][0])
        with self.assertRaisesRegex(ValueError, 'Candidats'):
            CHECK.validate(self.data)

if __name__ == '__main__':
    unittest.main()
