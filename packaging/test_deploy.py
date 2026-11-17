"""Deployment integration tests: real temporary git remotes, no GitHub writes or builds."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

SOURCE = Path(__file__).resolve().parent

GH = r'''#!/usr/bin/env python3
import json, os, pathlib, subprocess, sys, tomllib
args = sys.argv[1:]
state = json.loads(pathlib.Path(os.environ['TEST_STATE']).read_text())
with open(os.environ['TEST_CALLS'], 'a') as f:
    f.write(json.dumps(args) + '\n')
sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
version = tomllib.loads(pathlib.Path('Cargo.toml').read_text())['workspace']['package']['version']
names = [f'grid-{version}-macos-universal.dmg', f'grid-cli-{version}-macos-universal.zip',
         f'grid-{version}-windows-x64.msi', f'grid-{version}-windows-x64-portable.zip',
         'SHA256SUMS.txt', 'GridUpdate.json', 'GridUpdate.sig']
if args[:2] == ['auth', 'status']:
    sys.exit(1 if state.get('auth_failure') else 0)
if args[0] == 'api':
    if state.get('api_failure'):
        print('network failure', file=sys.stderr); sys.exit(1)
    if state.get('edit_during_preflight'):
        pathlib.Path('source.rs').write_text('// concurrent unfinished edit\n')
    print(json.dumps([state.get('releases', [])])); sys.exit(0)
run = dict(databaseId=123, headSha=sha, headBranch='my-custom-release', status='completed',
           conclusion='success', url='https://github.com/JeongEEE/gridcraft/actions/runs/123')
if args[:2] == ['run', 'list']:
    if pathlib.Path(os.environ['TEST_DISPATCHED']).exists():
        run['databaseId'] = 124
    elif state.get('failed_run'):
        run['conclusion'] = 'failure'
    print(json.dumps([run])); sys.exit(0)
if args[:2] == ['run', 'watch']:
    sys.exit(1 if state.get('build_failure') else 0)
if args[:2] == ['workflow', 'run']:
    pathlib.Path(os.environ['TEST_DISPATCHED']).touch()
    sys.exit(0)
if args[:2] == ['run', 'view']:
    run['headSha'] = state.get('run_sha', sha)
    run['jobs'] = [dict(name=n, conclusion='success') for n in ['version', 'macos', 'windows', 'draft']]
    print(json.dumps(run)); sys.exit(0)
if args[:2] == ['release', 'view']:
    if state.get('missing_asset'): names.remove('GridUpdate.sig')
    release = dict(tagName='gridcraft-v'+version, isDraft=True, isPrerelease=False,
                   targetCommitish=state.get('draft_sha', sha), body=f'Source commit: `{sha}`',
                   assets=[dict(name=n, size=100, state='uploaded') for n in names],
                   url='https://github.com/JeongEEE/gridcraft/releases/tag/gridcraft-v'+version)
    print(json.dumps(release)); sys.exit(0)
if args[:2] == ['release', 'edit']:
    pathlib.Path(os.environ['TEST_PUBLISHED']).write_text(args[2]); sys.exit(0)
print('unexpected gh command: '+str(args), file=sys.stderr); sys.exit(2)
'''

CARGO = r'''#!/usr/bin/env python3
import pathlib, sys, tomllib
new = sys.argv[-1]
manifest = pathlib.Path('Cargo.toml')
old = tomllib.loads(manifest.read_text())['workspace']['package']['version']
for name in ['Cargo.toml', 'Cargo.lock']:
    p = pathlib.Path(name)
    p.write_text(p.read_text().replace('version = "'+old+'"', 'version = "'+new+'"'))
'''


class DeployTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='grid-deploy-test-')
        self.addCleanup(self.tmp.cleanup)
        self.base = Path(self.tmp.name)
        self.repo = self.base / 'repo'
        self.repo.mkdir()
        self.remote = self.base / 'remote.git'
        self.env = os.environ.copy()
        self.env.update(GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_NOSYSTEM='1', GIT_TERMINAL_PROMPT='0')
        self.git('init', '-b', 'my-custom')
        self.git('config', 'user.name', 'Deployment Test')
        self.git('config', 'user.email', 'deploy@example.invalid')
        self.git('init', '--bare', str(self.remote))
        self.git('remote', 'add', 'origin', 'https://github.com/JeongEEE/gridcraft.git')
        self.git('config', f'url.{self.remote}.insteadOf', 'https://github.com/JeongEEE/gridcraft.git')
        (self.repo / 'packaging').mkdir()
        for name in ['deploy.py']:
            if (SOURCE / name).exists():
                shutil.copy(SOURCE / name, self.repo / 'packaging' / name)
        if (SOURCE.parent / 'deploy.sh').exists():
            shutil.copy(SOURCE.parent / 'deploy.sh', self.repo / 'deploy.sh')
        (self.repo / 'Cargo.toml').write_text('[workspace.package]\nversion = "0.19.5"\n')
        (self.repo / 'Cargo.lock').write_text('version = "0.19.5"\n')
        (self.repo / 'source.rs').write_text('// committed source\n')
        self.git('add', 'Cargo.toml', 'Cargo.lock', 'source.rs', 'packaging')
        self.git('commit', '-m', 'initial')
        self.initial = self.git('rev-parse', 'HEAD')
        self.git('push', 'origin', 'HEAD:refs/heads/my-custom', 'HEAD:refs/heads/my-custom-release')
        self.bin = self.base / 'bin'
        self.bin.mkdir()
        for name, source in [('gh', GH), ('cargo', CARGO)]:
            p = self.bin / name
            p.write_text(source.replace('#!/usr/bin/env python3', '#!' + sys.executable, 1))
            p.chmod(0o755)
        self.state = self.base / 'state.json'
        self.state.write_text('{}')
        self.published = self.base / 'published'
        self.calls = self.base / 'calls.jsonl'
        self.env.update(PATH=str(self.bin)+os.pathsep+self.env['PATH'], TEST_STATE=str(self.state),
                        TEST_PUBLISHED=str(self.published), TEST_CALLS=str(self.calls), TEST_DISPATCHED=str(self.base / 'dispatched'))

    def git(self, *args):
        return subprocess.check_output(['git', *args], cwd=self.repo, env=self.env, stderr=subprocess.DEVNULL, text=True).strip()

    def settings(self, **values):
        self.state.write_text(json.dumps(values))

    def deploy(self, *args, answer='n\n'):
        return subprocess.run([sys.executable, str(self.repo / 'packaging/deploy.py'), *args], cwd=self.repo,
                              env=self.env, input=answer, text=True, capture_output=True, timeout=30)

    def assert_ok(self, result):
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_default_bumps_only_version_and_keeps_untracked_work_out_of_release(self):
        (self.repo / 'scratch.txt').write_text('not part of release')
        self.assert_ok(self.deploy())
        self.assertIn('0.19.6', (self.repo / 'Cargo.toml').read_text())
        self.assertEqual(self.git('diff-tree', '--no-commit-id', '--name-only', '-r', 'HEAD').splitlines(), ['Cargo.lock', 'Cargo.toml'])
        sha = self.git('rev-parse', 'HEAD')
        self.assertEqual(self.git('ls-remote', 'origin', 'refs/heads/my-custom-release').split()[0], sha)
        self.assertEqual(self.git('branch', '--show-current'), 'my-custom')
        self.assertFalse(self.published.exists())
        self.assertTrue((self.repo / 'scratch.txt').exists())

    def test_explicit_version_and_final_confirmation_publish(self):
        self.assert_ok(self.deploy('0.20.0', answer='y\n'))
        self.assertEqual(self.published.read_text(), 'gridcraft-v0.20.0')

    def test_draft_flag_never_publishes_even_with_yes_input(self):
        self.assert_ok(self.deploy('--draft', answer='y\n'))
        self.assertFalse(self.published.exists())

    def test_dry_run_does_not_write_or_contact_github(self):
        before = self.git('status', '--porcelain')
        self.assert_ok(self.deploy('--dry-run'))
        self.assertEqual(self.git('rev-parse', 'HEAD'), self.initial)
        self.assertEqual(self.git('status', '--porcelain'), before)
        self.assertFalse(self.calls.exists())
        self.assertIn('0.19.5', (self.repo / 'Cargo.toml').read_text())

    def test_dirty_source_and_staged_changes_stop_before_version_update(self):
        for staged in [False, True]:
            with self.subTest(staged=staged):
                (self.repo / 'source.rs').write_text('// unfinished work\n')
                if staged:
                    self.git('add', 'source.rs')
                self.assertNotEqual(self.deploy().returncode, 0)
                self.assertEqual(self.git('rev-parse', 'HEAD'), self.initial)
                self.assertIn('0.19.5', (self.repo / 'Cargo.toml').read_text())

    def test_wrong_branch_and_remote_are_rejected(self):
        self.git('switch', '-c', 'main')
        self.assertNotEqual(self.deploy().returncode, 0)
        self.git('switch', 'my-custom')
        self.git('remote', 'set-url', 'origin', 'https://github.com/storytold/gridcraft.git')
        self.assertNotEqual(self.deploy().returncode, 0)
        self.assertEqual(self.git('rev-parse', 'HEAD'), self.initial)

    def test_published_version_network_error_and_older_version_do_not_mutate(self):
        for state, args in [({'releases': [{'tag_name': 'gridcraft-v0.19.6', 'draft': False, 'prerelease': False}]}, []),
                            ({'api_failure': True}, []), ({}, ['0.19.4'])]:
            with self.subTest(state=state, args=args):
                self.settings(**state)
                self.assertNotEqual(self.deploy(*args).returncode, 0)
                self.assertEqual(self.git('rev-parse', 'HEAD'), self.initial)

    def test_failed_build_incorrect_source_and_missing_assets_cannot_publish(self):
        for state in [{'build_failure': True}, {'run_sha': 'a'*40}, {'draft_sha': 'b'*40}, {'missing_asset': True}]:
            with self.subTest(state=state):
                self.settings(**state)
                self.assertNotEqual(self.deploy('0.19.6', answer='y\n').returncode, 0)
                self.assertFalse(self.published.exists())

    def test_retry_same_version_does_not_create_another_version_commit(self):
        self.settings(build_failure=True)
        self.assertNotEqual(self.deploy('0.19.6').returncode, 0)
        committed = self.git('rev-parse', 'HEAD')
        self.settings()
        self.assert_ok(self.deploy('0.19.6', '--draft'))
        self.assertEqual(self.git('rev-parse', 'HEAD'), committed)

    def test_existing_tag_pointing_elsewhere_is_not_published(self):
        self.git('tag', 'gridcraft-v0.19.6', self.initial)
        self.git('push', 'origin', 'refs/tags/gridcraft-v0.19.6')
        self.assertNotEqual(self.deploy('0.19.6', answer='y\n').returncode, 0)
        self.assertFalse(self.published.exists())

    def test_retry_after_a_failed_run_tracks_a_new_manual_run(self):
        self.assert_ok(self.deploy('0.19.6', '--draft'))
        self.settings(failed_run=True)
        self.assert_ok(self.deploy('0.19.6', '--draft'))
        calls = [json.loads(line) for line in self.calls.read_text().splitlines()]
        self.assertTrue(any(call[:2] == ['workflow', 'run'] for call in calls))
        self.assertTrue(any(call[:3] == ['run', 'watch', '124'] for call in calls))

    def test_remote_release_ahead_of_source_stops_before_version_mutation(self):
        self.git('switch', '-c', 'release-ahead')
        (self.repo / 'source.rs').write_text('// release-only change\n')
        self.git('add', 'source.rs')
        self.git('commit', '-m', 'release ahead')
        self.git('push', 'origin', 'HEAD:refs/heads/my-custom-release')
        self.git('switch', 'my-custom')
        self.assertNotEqual(self.deploy().returncode, 0)
        self.assertEqual(self.git('rev-parse', 'HEAD'), self.initial)
        self.assertIn('0.19.5', (self.repo / 'Cargo.toml').read_text())

    def test_noninteractive_eof_keeps_the_validated_draft(self):
        self.assert_ok(self.deploy(answer=''))
        self.assertFalse(self.published.exists())

    def test_source_edit_during_network_preflight_stops_before_version_mutation(self):
        self.settings(edit_during_preflight=True)
        self.assertNotEqual(self.deploy().returncode, 0)
        self.assertEqual(self.git('rev-parse', 'HEAD'), self.initial)
        self.assertIn('0.19.5', (self.repo / 'Cargo.toml').read_text())
        self.assertIn('concurrent unfinished edit', (self.repo / 'source.rs').read_text())

    def test_concurrent_commit_after_version_commit_is_not_silently_released(self):
        real_git = shutil.which('git')
        wrapper = self.bin / 'git'
        wrapper.write_text('#!' + sys.executable + '\n' + f'''
import pathlib, subprocess, sys
args = sys.argv[1:]
real = {real_git!r}
result = subprocess.run([real, *args])
if result.returncode == 0 and args and args[0] == 'commit' and '-m' in args and args[args.index('-m')+1].startswith('chore: Grid'):
    pathlib.Path('source.rs').write_text('// concurrent commit after version preparation\\n')
    subprocess.run([real, 'add', 'source.rs'], check=True)
    subprocess.run([real, 'commit', '-m', 'concurrent source commit'], check=True)
sys.exit(result.returncode)
''')
        wrapper.chmod(0o755)
        self.assertNotEqual(self.deploy().returncode, 0)
        self.assertEqual(self.git('ls-remote', 'origin', 'refs/heads/my-custom-release').split()[0], self.initial)
        self.assertFalse(self.published.exists())


if __name__ == '__main__':
    unittest.main()
