#!/usr/bin/env python3
"""Exercise release.sh in disposable repositories with an inert publisher."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import tomllib
import unittest

REPO = Path(__file__).resolve().parent.parent


class ReleaseWrapperTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='tiny-wb release ')
        self.root = Path(self.temporary.name)
        self.project = self.root / 'project with spaces'
        self.project.mkdir()
        for name in ('release.sh', 'Cargo.toml', 'rust-release.toml'):
            shutil.copy2(REPO / name, self.project / name)
        (self.project / 'release').mkdir()
        shutil.copy2(REPO / 'release' / 'Cross.toml', self.project / 'release' / 'Cross.toml')
        self.tools = self.root / 'tools'
        self.tools.mkdir()
        # An isolated PATH tests missing publishers without relying on the host.
        for tool in ('awk', 'dirname'):
            (self.tools / tool).symlink_to(shutil.which(tool))
        self.capture = self.root / 'captured.json'
        self.fake = self.tools / 'rust_realease'
        self.fake.write_text('#!' + sys.executable + '\n' + '''import json, os, pathlib, sys
repository = pathlib.Path(sys.argv[1])
cross_config = pathlib.Path(os.environ["CROSS_CONFIG"])
record = {"args": sys.argv[1:], "token": os.environ.get("GITEA_TOKEN"),
          "fallback": (repository / "rust-release.toml").read_text(),
          "cross_config": str(cross_config),
          "cross_config_contents": cross_config.read_text()}
pathlib.Path(os.environ["CAPTURE"]).write_text(json.dumps(record))
sys.exit(int(os.environ.get("PUBLISHER_EXIT", "0")))
''')
        self.fake.chmod(0o755)
        self.token = self.root / 'token with spaces'
        self.token.write_text('inert-test-token-123\nignored-second-line\n')
        self.token.chmod(0o600)
        self.env = {**os.environ, 'PATH': str(self.tools), 'CAPTURE': str(self.capture)}
        self.env.pop('GITEA_TOKEN', None)
        self.env.pop('PUBLISHER_EXIT', None)
        subprocess.run(['git', 'init', '-q', str(self.project)], check=True)
        subprocess.run(['git', '-C', str(self.project), 'remote', 'add', 'origin',
                        'git@git.dandaedre.com:idle-org/tiny-wb.git'], check=True)

    def tearDown(self):
        self.temporary.cleanup()

    def run_wrapper(self, arguments, expected=0):
        result = subprocess.run(['/bin/sh', str(self.project / 'release.sh'), *arguments],
                                cwd=self.root, env=self.env, capture_output=True, text=True)
        self.assertEqual(result.returncode, expected, result.stderr)
        self.assertNotIn('inert-test-token-123', result.stdout + result.stderr)
        return result

    def set_version(self, version):
        manifest = self.project / 'Cargo.toml'
        text = manifest.read_text()
        old = tomllib.loads(text)['package']['version']
        manifest.write_text(text.replace(f'version = "{old}"', f'version = "{version}"', 1))

    def test_exact_arguments_token_environment_and_fallback(self):
        before = subprocess.check_output(['git', '-C', str(self.project), 'remote', 'get-url', 'origin'])
        extra = ['--dry-run', '--message-file', 'notes with spaces; $(not-a-command) *.md']
        self.run_wrapper([self.token.name, *extra])
        record = json.loads(self.capture.read_text())
        version = tomllib.loads((self.project / 'Cargo.toml').read_text())['package']['version']
        prerelease = ['--prerelease'] if '-' in version.split('+', 1)[0] else []
        self.assertEqual(record['args'], [str(self.project), '--package', 'tiny-wb',
                                         '--artifact-format', 'both', *extra, *prerelease])
        self.assertEqual(record['token'], 'inert-test-token-123')
        self.assertNotIn(record['token'], '\n'.join(record['args']))
        self.assertEqual(tomllib.loads(record['cross_config_contents']), {
            'target': {'x86_64-unknown-linux-gnu': {
                'pre-build': [
                    'apt-get update && apt-get install --assume-yes '
                    '--no-install-recommends libudev-dev pkg-config']}}})
        self.assertEqual(Path(record['cross_config']), self.project / 'release' / 'Cross.toml')
        self.assertTrue(Path(record['cross_config']).is_file())
        config = tomllib.loads(record['fallback'])
        self.assertEqual(config['repository']['fallbacks'], [{
            'origin': before.decode().strip(),
            'fallback-url': 'https://proxy.dandaedre.com/gitea/idle-org/tiny-wb'}])
        after = subprocess.check_output(['git', '-C', str(self.project), 'remote', 'get-url', 'origin'])
        self.assertEqual(before, after)

    def test_prerelease_and_build_metadata(self):
        for version, prerelease in [('1.2.3', False), ('1.2.3-rc.1', True),
                                    ('1.2.3+build-with-hyphen', False), ('1.2.3-rc.1+build', True)]:
            with self.subTest(version=version):
                self.set_version(version)
                self.run_wrapper([str(self.token), '--dry-run'])
                self.assertEqual('--prerelease' in json.loads(self.capture.read_text())['args'], prerelease)

    def test_publisher_exit_status_and_nonterminated_token(self):
        self.env['PUBLISHER_EXIT'] = '37'
        self.token.write_text('inert-test-token-123')
        self.run_wrapper([str(self.token), '--compile-only'], expected=37)
        record = json.loads(self.capture.read_text())
        self.assertEqual(record['token'], 'inert-test-token-123')

    def test_compile_only_needs_no_token(self):
        self.run_wrapper(['--compile-only', '--recompile'])
        record = json.loads(self.capture.read_text())
        version = tomllib.loads((self.project / 'Cargo.toml').read_text())['package']['version']
        prerelease = ['--prerelease'] if '-' in version.split('+', 1)[0] else []
        self.assertEqual(record['args'], [str(self.project), '--package', 'tiny-wb',
                                         '--artifact-format', 'both', '--compile-only',
                                         '--recompile', *prerelease])
        self.assertIsNone(record['token'])

    def test_missing_release_cross_config_never_invokes_publisher(self):
        (self.project / 'release' / 'Cross.toml').unlink()
        result = self.run_wrapper(['--compile-only'], expected=2)
        self.assertIn('release Cross configuration is missing or unreadable', result.stderr)
        self.assertFalse(self.capture.exists())

    def test_invalid_token_inputs_never_invoke_publisher(self):
        self.run_wrapper([], expected=2)
        self.run_wrapper([str(self.root / 'missing')], expected=2)
        self.run_wrapper([str(self.root)], expected=2)
        for contents in ('', '\nsecond-line-is-not-a-token\n'):
            self.token.write_text(contents)
            self.run_wrapper([str(self.token)], expected=2)
        self.assertFalse(self.capture.exists())

    @unittest.skipIf(os.geteuid() == 0, 'root bypasses ordinary readability checks')
    def test_unreadable_token(self):
        self.token.chmod(0)
        try:
            self.run_wrapper([str(self.token)], expected=2)
            self.assertFalse(self.capture.exists())
        finally:
            self.token.chmod(0o600)

    def test_missing_publisher_and_unresolved_version(self):
        self.fake.unlink()
        self.run_wrapper([str(self.token)], expected=2)
        self.fake.write_text('#!/bin/sh\nexit 99\n')
        self.fake.chmod(0o755)
        for manifest in ('[package]\nname="tiny-wb"\n',
                         '[package]\nversion.workspace=true\n[dependencies.example]\nversion="1.2.3"\n',
                         '[package]\nversion="not-semver"\n'):
            (self.project / 'Cargo.toml').write_text(manifest)
            self.run_wrapper([str(self.token)], expected=2)
        (self.project / 'Cargo.toml').unlink()
        self.run_wrapper([str(self.token)], expected=2)

    def test_xtrace_does_not_expose_token(self):
        result = subprocess.run(['/bin/sh', '-x', str(self.project / 'release.sh'), str(self.token), '--dry-run'],
                                cwd=self.root, env=self.env, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn('inert-test-token-123', result.stdout + result.stderr)

    def test_metadata_agrees_with_tag_and_archive_policy(self):
        manifest = tomllib.loads((REPO / 'Cargo.toml').read_text())['package']
        config = tomllib.loads((REPO / 'rust-release.toml').read_text())
        self.assertEqual(manifest['name'], 'tiny-wb')
        self.assertEqual(config['release']['tag-prefix'], '')
        self.assertEqual(config['build']['profile'], 'release')
        self.assertTrue(config['build']['locked'])
        self.assertEqual(config['build']['targets'], [
            'x86_64-unknown-linux-gnu', 'x86_64-pc-windows-gnu',
            'aarch64-apple-darwin', 'x86_64-apple-darwin'])
        self.assertEqual(config['build']['apple'], {
            'image': 'ghcr.io/rust-cross/cargo-zigbuild@sha256:'
                     '9b4f6b3eb9e8f9fefb3960fe288ddba576627d66d9eff851ce7f62d2455564a5',
            'deployment-target': '13.0',
            'sdk-root': '/opt/MacOSX11.3.sdk'})
        self.assertEqual(manifest['metadata']['binstall'], {
            'pkg-url': '{ repo }/releases/download/{ version }/{ name }-{ target }-v{ version }.tgz',
            'pkg-fmt': 'tgz',
            'bin-dir': '{ name }-{ target }-v{ version }/{ bin }{ binary-ext }'})

    def test_apple_target_overrides_are_forwarded_without_a_token(self):
        self.env['GITEA_TOKEN'] = 'inherited-token-must-not-reach-publisher'
        targets = ['--target', 'aarch64-apple-darwin',
                   '--target', 'x86_64-apple-darwin']
        self.run_wrapper(['--compile-only', *targets])
        record = json.loads(self.capture.read_text())
        self.assertEqual(record['args'][6:10], targets)
        self.assertIsNone(record['token'])
        config = tomllib.loads(record['fallback'])
        self.assertEqual(config['build']['apple']['deployment-target'], '13.0')
        self.assertEqual(Path(record['cross_config']), self.project / 'release' / 'Cross.toml')


if __name__ == '__main__':
    unittest.main()
