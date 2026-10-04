"""Exercise the public-tree checker through its real CLI, never with secrets."""
import pathlib
import subprocess
import sys
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parents[1] / 'check-publication.py'


class PublicationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='parole-publication-')
        self.addCleanup(self.temp.cleanup)
        self.root = pathlib.Path(self.temp.name)
        (self.root / 'README.md').write_text('# Example\n', encoding='utf-8')

    def check(self):
        return subprocess.run(
            [sys.executable, str(SCRIPT), '--root', str(self.root)],
            capture_output=True, text=True, check=False,
        )

    def test_clean_tree_passes(self):
        result = self.check()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn('PASS', result.stdout)

    def test_forbidden_names_block_without_opening_contents(self):
        import os
        path = self.root / '.env.local'
        if os.name == 'posix':
            os.mkfifo(path)
        else:
            path.touch()
        result = subprocess.run(
            [sys.executable, str(SCRIPT), '--root', str(self.root)],
            capture_output=True, text=True, timeout=5, check=False,
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn('forbidden name', result.stdout)

    def test_token_detection_never_displays_the_value(self):
        synthetic = 'ghp_' + 'x' * 36
        (self.root / 'example.txt').write_text(synthetic, encoding='utf-8')
        result = self.check()
        self.assertEqual(result.returncode, 1)
        self.assertIn('credential pattern', result.stdout)
        self.assertNotIn(synthetic, result.stdout + result.stderr)

    def test_missing_local_link_blocks(self):
        (self.root / 'README.md').write_text('# Example\n[Guide](missing.md)\n', encoding='utf-8')
        result = self.check()
        self.assertEqual(result.returncode, 1)
        self.assertIn('missing local link', result.stdout)

    def test_symlink_blocks_without_following_it(self):
        try:
            (self.root / 'linked.md').symlink_to(self.root / 'README.md')
        except OSError:
            self.skipTest('Symlink creation unavailable')
        result = self.check()
        self.assertEqual(result.returncode, 1)
        self.assertIn('symbolic link', result.stdout)

    def test_private_server_reference_blocks(self):
        private_path = '/srv/' + 'hermes-data/users/example'
        (self.root / 'example.md').write_text(private_path, encoding='utf-8')
        result = self.check()
        self.assertEqual(result.returncode, 1)
        self.assertIn('private reference', result.stdout)
        self.assertNotIn(private_path, result.stdout + result.stderr)

    def test_oversized_file_blocks(self):
        with (self.root / 'large.txt').open('wb') as stream:
            stream.truncate(10_000_000)
        result = self.check()
        self.assertEqual(result.returncode, 1)
        self.assertIn('oversized file', result.stdout)

    def test_existing_links_and_code_examples_pass(self):
        (self.root / 'guide.md').write_text('# Guide\n', encoding='utf-8')
        (self.root / 'README.md').write_text(
            '# Example\n[Guide](guide.md#guide)\n'
            '```markdown\n[Not a real link](example-only.md)\n```\n',
            encoding='utf-8',
        )
        self.assertEqual(self.check().returncode, 0)

    def test_tracked_file_in_generated_directory_is_checked(self):
        generated = self.root / 'dist'
        generated.mkdir()
        (generated / 'weights.onnx').touch()  # Empty synthetic input, not a model.
        subprocess.run(['git', '-C', str(self.root), 'init', '-q'], check=True)
        subprocess.run(['git', '-C', str(self.root), 'add', '-f', 'dist/weights.onnx'], check=True)
        result = self.check()
        self.assertEqual(result.returncode, 1)
        self.assertIn('forbidden name', result.stdout)

    def test_generated_directory_symlink_is_not_ignored(self):
        try:
            (self.root / 'target').symlink_to(self.root, target_is_directory=True)
        except OSError:
            self.skipTest('Symlink creation unavailable')
        result = self.check()
        self.assertEqual(result.returncode, 1)
        self.assertIn('symbolic link', result.stdout)

    def test_untracked_generated_directory_is_ignored(self):
        generated = self.root / 'target'
        generated.mkdir()
        (generated / 'weights.onnx').touch()
        self.assertEqual(self.check().returncode, 0)

    def test_unreadable_directory_blocks(self):
        import os
        if os.name != 'posix' or os.geteuid() == 0:
            self.skipTest('Requires POSIX permissions under a non-root identity')
        blocked = self.root / 'blocked'
        blocked.mkdir()
        (blocked / 'example.txt').write_text('Synthetic input', encoding='utf-8')
        blocked.chmod(0)
        try:
            result = self.check()
        finally:
            blocked.chmod(0o700)
        self.assertEqual(result.returncode, 1)
        self.assertIn('unreadable directory', result.stdout)


if __name__ == '__main__':
    unittest.main()
