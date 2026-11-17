#!/usr/bin/env python3
"""Native desktop release orchestration. GitHub stores files; no Actions runs are requested."""
import argparse
from contextlib import contextmanager
import json
import os
from pathlib import Path
import platform
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import tomllib

import local_build
import release_assets as assets
from release_assets import DeployError

ROOT = Path(__file__).resolve().parents[1]
REPO = 'JeongEEE/gridcraft'
BRANCH = 'my-custom'
ORIGIN_URLS = {f'https://github.com/{REPO}', f'https://github.com/{REPO}.git',
               f'git@github.com:{REPO}.git', f'ssh://git@github.com/{REPO}.git'}
FIELDS = 'tagName,isDraft,isPrerelease,targetCommitish,body,assets,url'


def version_parts(value):
    if not re.fullmatch(r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)', value):
        raise DeployError('버전은 0.19.7과 같은 X.Y.Z 형식이어야 합니다.')
    return tuple(map(int, value.split('.')))


@contextmanager
def release_lock(path):
    with path.open('a+b') as handle:
        handle.seek(0, os.SEEK_END)
        if handle.tell() == 0:
            handle.write(b'\0'); handle.flush()
        handle.seek(0)
        try:
            if os.name == 'nt':
                import msvcrt
                msvcrt.locking(handle.fileno(), msvcrt.LK_NBLCK, 1)
            else:
                import fcntl
                fcntl.flock(handle.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        except OSError as error:
            raise DeployError('같은 저장소에서 다른 배포가 진행 중입니다.') from error
        try:
            yield
        finally:
            handle.seek(0)
            if os.name == 'nt':
                msvcrt.locking(handle.fileno(), msvcrt.LK_UNLCK, 1)
            else:
                fcntl.flock(handle.fileno(), fcntl.LOCK_UN)


class Deployment:
    def __init__(self, args):
        self.args, self.root = args, ROOT
        self.env = dict(os.environ, GH_HOST='github.com', GH_PROMPT_DISABLED='1', GH_PAGER='cat', GIT_TERMINAL_PROMPT='0')
        self.env.pop('GRID_UPDATE_PRIVATE_KEY', None)
        xcode = '/Applications/Xcode.app/Contents/Developer'
        if platform.system() == 'Darwin' and 'DEVELOPER_DIR' not in self.env and Path(xcode).is_dir():
            self.env['DEVELOPER_DIR'] = xcode
        self.cargo = shutil.which('cargo') or str(Path.home()/'.cargo/bin'/('cargo.exe' if os.name == 'nt' else 'cargo'))
        self.version, self.sha, self.common = '', '', None
        self.tag = ''

    def command(self, *args, capture=True, optional=False, env=None, cwd=None):
        args = [str(a) for a in args]
        result = subprocess.run(args, cwd=cwd or self.root, env=env or self.env, text=True, encoding='utf-8', errors='replace',
                                stdout=subprocess.PIPE if capture else None, stderr=subprocess.PIPE if capture else None)
        if result.returncode and not optional:
            raise DeployError(f'명령 실패: {shlex.join(args)}\n{(result.stderr or result.stdout or "").strip()}')
        return result

    def git(self, *args):
        return self.command('git', *args).stdout.strip()

    def gh(self, *args, capture=True):
        return self.command('gh', *args, '--repo', REPO, capture=capture)

    def clean(self):
        if self.git('status', '--porcelain', '--untracked-files=no'):
            raise DeployError('커밋되지 않은 추적 파일 또는 스테이징 변경이 있습니다. 먼저 정리하세요.')

    def assert_source(self):
        self.clean()
        if self.git('branch', '--show-current') != BRANCH or self.git('rev-parse', 'HEAD') != self.sha:
            raise DeployError('배포 중 브랜치 또는 소스가 변경되었습니다. 파일을 업로드하지 않습니다.')

    def local_preflight(self):
        if self.git('branch', '--show-current') != BRANCH:
            raise DeployError(f'{BRANCH} 브랜치에서 실행하세요.')
        self.clean()
        for key in ['remote.origin.url', 'remote.origin.pushurl']:
            rows = self.command('git', 'config', '--get-all', key, optional=True).stdout.splitlines()
            if key.endswith('pushurl') and not rows:
                continue
            if len(rows) != 1 or rows[0] not in ORIGIN_URLS:
                raise DeployError(f'origin은 {REPO} 포크 한 곳만 가리켜야 합니다.')
        current = tomllib.loads((self.root/'Cargo.toml').read_text(encoding='utf-8'))['workspace']['package']['version']
        version_parts(current)
        self.version = self.args.version or current
        if version_parts(self.version) < version_parts(current):
            raise DeployError('현재 소스보다 낮은 버전으로 배포할 수 없습니다.')
        if not self.args.dry_run and not self.args.finalize:
            expected = 'Darwin' if self.args.platform == 'macos' else 'Windows'
            if platform.system() != expected:
                raise DeployError(f'{self.args.platform} 빌드는 해당 OS에서 실행하세요.')
        self.sha = self.git('rev-parse', 'HEAD')
        return current

    def prepare(self, current):
        self.command('gh', 'auth', 'status', '--hostname', 'github.com')
        pages = json.loads(self.command('gh', 'api', f'repos/{REPO}/releases?per_page=100', '--paginate', '--slurp').stdout)
        releases = [r for page in pages for r in page]
        published = [r['tag_name'][11:] for r in releases
                     if not r['draft'] and not r['prerelease'] and re.fullmatch(r'gridcraft-v\d+\.\d+\.\d+', r['tag_name'])]
        if not self.args.version and not self.args.finalize and current in published:
            major, minor, patch = version_parts(current)
            self.version = f'{major}.{minor}.{patch+1}'
        if any(version_parts(v) >= version_parts(self.version) for v in published):
            raise DeployError('공개된 버전보다 높은 버전을 사용하세요. 공개본은 덮어쓰지 않습니다.')
        self.tag = f'gridcraft-v{self.version}'
        self.command('git', 'fetch', '--no-tags', 'origin', f'refs/heads/{BRANCH}:refs/remotes/origin/{BRANCH}')
        if self.command('git', 'merge-base', '--is-ancestor', f'origin/{BRANCH}', self.sha, optional=True).returncode:
            raise DeployError('원격 변경을 먼저 pull하세요. 배포 스크립트는 자동 병합하지 않습니다.')
        self.assert_source()
        if self.version != current:
            if self.args.finalize:
                raise DeployError('마무리는 빌드했던 버전의 소스를 pull한 뒤 실행하세요.')
            initial = self.sha
            self.command(self.cargo, 'run', '--quiet', '-p', 'xtask', '--no-default-features', '--', 'version', 'set', self.version, capture=False)
            if self.git('rev-parse', 'HEAD') != initial or self.git('branch', '--show-current') != BRANCH:
                raise DeployError('버전 준비 중 소스가 변경되었습니다.')
            changed = set(self.git('diff', '--name-only').splitlines())
            if not changed or not changed <= {'Cargo.toml', 'Cargo.lock'} or self.git('diff', '--cached', '--name-only'):
                raise DeployError('버전 파일 외 변경이 있어 커밋하지 않습니다.')
            self.command('git', 'add', '--', 'Cargo.toml', 'Cargo.lock')
            self.command('git', 'commit', '--only', '-m', f'chore: Grid {self.version} 릴리스 버전 준비', '--', 'Cargo.toml', 'Cargo.lock', capture=False)
            self.sha = self.git('rev-parse', 'HEAD')
            if self.git('rev-parse', f'{self.sha}^') != initial:
                raise DeployError('버전 커밋 중 다른 커밋이 추가되었습니다.')
        self.assert_source()
        self.command('git', 'push', 'origin', f'{self.sha}:refs/heads/{BRANCH}', capture=False)
        self.check_tag()
        found = any(r['tag_name'] == self.tag for r in releases)
        if found:
            self.view()
        elif self.args.finalize:
            raise DeployError('릴리스 초안이 없습니다. 각 OS 빌드를 먼저 실행하세요.')
        else:
            with tempfile.TemporaryDirectory(prefix='grid-release-notes-') as scratch:
                notes = Path(scratch)/'notes.md'
                notes.write_text(f'Grid {self.version} · local macOS / Windows builds.\n\nSource commit: `{self.sha}`\n'
                                 'Both platform packages and signed update files are required before publication.\n', encoding='utf-8')
                self.gh('release', 'create', self.tag, '--target', self.sha, '--title', f'Grid v{self.version}',
                        '--draft', '--notes-file', notes)
            self.view()

    def check_tag(self):
        rows = self.git('ls-remote', '--tags', 'origin', f'refs/tags/{self.tag}', f'refs/tags/{self.tag}^{{}}').splitlines()
        if rows and ([r for r in rows if r.endswith('^{}')] or rows)[0].split()[0] != self.sha:
            raise DeployError('기존 태그가 다른 소스를 가리킵니다. 덮어쓰지 않습니다.')

    def view(self):
        release = json.loads(self.gh('release', 'view', self.tag, '--json', FIELDS).stdout)
        if (release['tagName'] != self.tag or not release['isDraft'] or release['isPrerelease']
                or release['targetCommitish'] != self.sha or f'Source commit: `{self.sha}`' not in release['body']):
            raise DeployError('같은 소스 커밋의 미공개 초안이 아닙니다. 두 PC에서 같은 버전을 pull하세요.')
        allowed = set(assets.payload_names(self.version) + ['GridUpdate.json', 'GridUpdate.sig', 'SHA256SUMS.txt'])
        rows = release['assets']
        if len({r['name'] for r in rows}) != len(rows) or any(r['name'] not in allowed or r['size'] <= 0 or r['state'] != 'uploaded' for r in rows):
            raise DeployError('초안에 예상하지 못했거나 불완전한 자산이 있습니다.')
        return release

    @staticmethod
    def fingerprint(release):
        return sorted((r['name'], r['id'], r['size'], r['updatedAt'], r.get('digest')) for r in release['assets'])

    def upload_platform(self):
        print(f'\n로컬 {self.args.platform} 빌드 · Grid {self.version} · {self.sha}', flush=True)
        with tempfile.TemporaryDirectory(prefix='grid-native-build-') as scratch:
            output = Path(scratch)
            checks = local_build.build(self, output)
            self.assert_source()
            receipt = assets.write_receipt(output, self.version, self.sha, self.args.platform, checks)
            self.view(); self.check_tag()
            files = [output/n for n in assets.package_names(self.version, self.args.platform)] + [receipt]
            self.gh('release', 'upload', self.tag, *files, '--clobber', capture=False)
        print('이 OS의 패키지와 빌드 증빙을 초안에 업로드했습니다.', flush=True)

    def key_file(self):
        value = self.args.key_file or self.env.get('GRID_UPDATE_KEY_FILE')
        return Path(value).expanduser() if value else Path.home()/'Dev/keystores/grid/update-private-key.hex'

    def finalize(self):
        before = self.view()
        missing = set(assets.payload_names(self.version)) - {r['name'] for r in before['assets']}
        if missing:
            if self.args.finalize:
                raise DeployError('아직 없는 빌드 파일: '+', '.join(sorted(missing)))
            print('다른 OS 빌드를 기다리는 초안입니다. 같은 버전·소스를 pull한 뒤 그 OS의 배포 스크립트를 실행하세요.')
            return
        key = self.key_file()
        if not key.is_file():
            if self.args.finalize:
                raise DeployError('업데이트 서명 키 파일이 없습니다. --key-file 또는 GRID_UPDATE_KEY_FILE을 지정하세요.')
            print(f'두 OS 업로드 완료. 키가 있는 PC에서 마무리하세요: ./deploy-macos.sh {self.version} --finalize')
            return
        with tempfile.TemporaryDirectory(prefix='grid-release-finalize-') as scratch:
            folder = Path(scratch)
            patterns = [part for name in assets.payload_names(self.version) for part in ['--pattern', name]]
            self.gh('release', 'download', self.tag, '--dir', folder, *patterns)
            assets.validate_payloads(folder, self.version, self.sha)
            secret = key.read_text(encoding='utf-8').strip()
            if not re.fullmatch(r'[0-9a-fA-F]{64}', secret):
                raise DeployError('업데이트 키는 기존 32바이트 seed의 16진수 파일이어야 합니다.')
            # Only the signer subprocess receives the key. Never include it in arguments or logs.
            with local_build.committed_source(self) as source:
                self.command(self.cargo, 'run', '--quiet', '--locked', '-p', 'gridcraft-update', '--bin', 'grid-update-sign', '--',
                             'sign', folder, self.version, source/'crates/update/public-key.hex', cwd=source,
                             env=dict(self.env, CARGO_TARGET_DIR=str(self.root/'target'), GRID_UPDATE_PRIVATE_KEY=secret))
            assets.validate_signed_files(folder, self.version)
            assets.write_checksums(folder, self.version)
            self.assert_source(); self.check_tag()
            if self.fingerprint(self.view()) != self.fingerprint(before):
                raise DeployError('검증 중 초안의 파일이 바뀌었습니다. 두 PC에서 동시에 배포하지 말고 다시 실행하세요.')
            self.gh('release', 'upload', self.tag, folder/'GridUpdate.json', folder/'GridUpdate.sig', folder/'SHA256SUMS.txt', '--clobber', capture=False)
            ready = self.view()
            if len(ready['assets']) != 9:
                raise DeployError('최종 자산 9개가 모두 있어야 공개할 수 있습니다.')
            # Verify the uploaded bytes too; a successful CLI call alone does not prove their content.
            download = folder/'uploaded'; download.mkdir()
            self.gh('release', 'download', self.tag, '--dir', download)
            for name in assets.payload_names(self.version) + ['GridUpdate.json', 'GridUpdate.sig', 'SHA256SUMS.txt']:
                if assets.file_hash(download/name) != assets.file_hash(folder/name):
                    raise DeployError(f'업로드된 파일이 다릅니다: {name}')
            print(f'\nGrid {self.version} · 두 OS·해시·서명·자산 9개 확인 완료\n{ready["url"]}', flush=True)
            if not self.args.draft:
                try:
                    answer = input('이 버전을 공개하여 자동 업데이트를 배포할까요? [y/N] ').strip().lower()
                except EOFError:
                    answer = ''
                if answer in {'y', 'yes', '예'}:
                    self.assert_source(); self.check_tag()
                    if self.fingerprint(self.view()) != self.fingerprint(ready):
                        raise DeployError('확인 중 릴리스 자산이 바뀌어 공개하지 않습니다.')
                    self.gh('release', 'edit', self.tag, '--draft=false', '--latest', capture=False)
                    print(f'배포 완료 · Grid {self.version}')
                    return
            print('초안을 유지했습니다. 나중에 --finalize로 공개 확인을 이어갈 수 있습니다.')

    def run(self):
        current = self.local_preflight()
        if self.args.dry_run:
            print(f'[dry-run] Grid {self.version} · {self.args.platform} 로컬 '+('마무리' if self.args.finalize else '빌드'))
            print('미공개 버전은 재사용하고, 현재 버전이 공개됐다면 다음 패치를 준비합니다. Actions 호출·배포 브랜치 푸시 없음.')
            print('파일·커밋·원격은 변경하지 않았고 네트워크를 호출하지 않았습니다.')
            return
        common = Path(self.git('rev-parse', '--git-common-dir'))
        self.common = common if common.is_absolute() else self.root/common
        with release_lock(self.common/'grid-deploy.lock'):
            self.prepare(current)
            if not self.args.finalize:
                self.upload_platform()
            self.finalize()


def main():
    parser = argparse.ArgumentParser(description='Grid OS별 로컬 빌드 → GitHub 릴리스 초안 업로드 → 서명·공개 확인')
    parser.add_argument('version', nargs='?', help='X.Y.Z. 생략하면 현재 미공개 버전 재사용, 공개됐으면 다음 패치')
    parser.add_argument('--platform', choices=['macos', 'windows'], required=True)
    parser.add_argument('--dry-run', action='store_true', help='변경·네트워크 요청 없이 절차만 확인')
    parser.add_argument('--draft', action='store_true', help='공개 질문 없이 초안 유지')
    parser.add_argument('--finalize', action='store_true', help='빌드 없이 두 OS 업로드 결과 검증·서명·공개 확인')
    parser.add_argument('--key-file', help='기존 업데이트 개인키 파일 경로 (기본 GRID_UPDATE_KEY_FILE 또는 ~/Dev/keystores/grid/update-private-key.hex)')
    args = parser.parse_args()
    deployment = Deployment(args)
    try:
        deployment.run()
    except (DeployError, OSError, ValueError, KeyError, TypeError) as error:
        print(f'\n배포 중단: {error}', file=sys.stderr)
        command = f'./deploy-macos.sh {deployment.version}' if args.platform == 'macos' else f'.\\deploy-windows.ps1 -Version {deployment.version}'
        print('같은 버전으로 재시도: '+command+(' --finalize' if args.finalize and args.platform == 'macos' else ' -Finalize' if args.finalize else ''), file=sys.stderr)
        return 1
    except KeyboardInterrupt:
        print('\n로컬 배포를 중단했습니다. 초안은 유지되며 같은 버전으로 다시 실행할 수 있습니다.', file=sys.stderr)
        return 130
    return 0


if __name__ == '__main__':
    sys.exit(main())
