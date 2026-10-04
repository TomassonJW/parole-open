import importlib.util
import json
import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('mac_signing', ROOT/'scripts/check-macos-signing.py')
assert spec and spec.loader
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class MacSigningTests(unittest.TestCase):
    def test_rejects_hardened_ad_hoc_combination(self):
        with self.assertRaisesRegex(ValueError, 'no Apple Team ID'):
            module.validate_profile({'adhoc': True, 'hardened': True, 'team': None})

    def test_accepts_private_ad_hoc_package(self):
        module.validate_profile({'adhoc': True, 'hardened': False, 'team': None})

    def test_accepts_developer_signed_package(self):
        module.validate_profile({'adhoc': False, 'hardened': True, 'team': 'TEST-IDENTITY'})

    def test_rejects_missing_team_on_non_ad_hoc_signature(self):
        with self.assertRaises(ValueError):
            module.validate_profile({'adhoc': False, 'hardened': True, 'team': None})

    def test_rejects_unsigned_or_non_macho_input(self):
        with self.assertRaises(ValueError):
            module.signing_profile(ROOT/'src-tauri/tauri.macos.conf.json')

    def test_mac_trial_settings_are_explicit(self):
        config = json.loads((ROOT/'src-tauri/tauri.macos.conf.json').read_text())
        self.assertEqual(config['bundle']['macOS']['signingIdentity'], '-')
        self.assertIs(config['bundle']['macOS']['hardenedRuntime'], False)
        self.assertNotIn('entitlements', config['bundle']['macOS'])
        self.assertEqual(config['version'], '0.1.2')


if __name__ == '__main__':
    unittest.main()
