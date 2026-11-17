#!/usr/bin/env python3
"""Prepare a Grid fork release and follow its existing GitHub Actions workflow."""
import argparse
import fcntl
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import sys
import time
import tomllib

ROOT = Path(__file__).resolve().parents[1]
REPO = 'JeongEEE/gridcraft'
BRANCH = 'my-custom'
RELEASE_BRANCH = 'my-custom-release'
WORKFLOW = 'release-custom.yml'
VERSION_FILES = {'Cargo.toml', 'Cargo.lock'}
ORIGIN_URLS = {f'https://github.com/{REPO}', f'https://github.com/{REPO}.git',
               f'git@github.com:{REPO}.git', f'ssh://git@github.com/{REPO}.git'}


class DeployError(Exception):
    pass


def version_parts(value):
    if not re.fullmatch(r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)', value):
        raise DeployError('버전은 0.19.6과 같은 X.Y.Z 형식이어야 합니다.')
    return tuple(map(int, value.split('.')))


class Deployment:
    def __init__(self, args):
        self.args = args
        self.env = dict(os.environ, GH_HOST='github.com', GH_PROMPT_DISABLED='1', GH_PAGER='cat', GIT_TERMINAL_PROMPT='0')
        xcode = '/Applications/Xcode.app/Contents/Developer'
        if sys.platform == 'darwin' and 'DEVELOPER_DIR' not in self.env and Path(xcode).is_dir():
            self.env['DEVELOPER_DIR'] = xcode
        self.cargo = shutil.which('cargo') or str(Path.home() / '.cargo/bin/cargo')
        self.version = ''
        self.sha = ''
        self.run_url = ''

    def command(self, *args, capture=True, optional=False):
        result = subprocess.run(args, cwd=ROOT, env=self.env, text=True,
                                stdout=subprocess.PIPE if capture else None,
                                stderr=subprocess.PIPE if capture else None)
        if result.returncode and not optional:
            detail = (result.stderr or result.stdout or '').strip()
            raise DeployError(f'명령 실패: {shlex.join(args)}\n{detail}')
        return result

    def git(self, *args):
        return self.command('git', *args).stdout.strip()

    def gh(self, *args, capture=True):
        return self.command('gh', *args, '--repo', REPO, capture=capture)

    def gh_json(self, *args):
        return json.loads(self.gh(*args).stdout)

    def step(self, number, message):
        prefix = f'[{number}/5]'
        if sys.stdout.isatty():
            prefix = f'\033[1;32m{prefix}\033[0m'
        print(f'\n{prefix} {message}', flush=True)

    def clean(self):
        if self.git('status', '--porcelain', '--untracked-files=no'):
            raise DeployError('커밋되지 않은 추적 파일 또는 스테이징 변경이 있습니다. 먼저 작업을 정리하세요. 미추적 파일은 배포에 포함하지 않습니다.')

    def local_preflight(self):
        if self.git('branch', '--show-current') != BRANCH:
            raise DeployError(f'{BRANCH} 브랜치에서 실행하세요.')
        self.clean()
        for key in ['remote.origin.url', 'remote.origin.pushurl']:
            result = self.command('git', 'config', '--get-all', key, optional=True)
            urls = result.stdout.splitlines()
            if key.endswith('pushurl') and not urls:
                continue
            if len(urls) != 1 or urls[0] not in ORIGIN_URLS:
                raise DeployError(f'origin은 {REPO} 포크 한 곳만 가리켜야 합니다.')
        current = tomllib.loads((ROOT / 'Cargo.toml').read_text())['workspace']['package']['version']
        major, minor, patch = version_parts(current)
        self.version = self.args.version or f'{major}.{minor}.{patch + 1}'
        if version_parts(self.version) < version_parts(current):
            raise DeployError('현재 소스보다 낮은 버전으로 배포할 수 없습니다.')
        return current

    def ancestor(self, older, newer):
        result = self.command('git', 'merge-base', '--is-ancestor', older, newer, optional=True)
        if result.returncode:
            raise DeployError(f'{older}의 변경을 현재 소스에 먼저 반영하세요. 배포는 fast-forward만 허용합니다.')

    def releases_preflight(self):
        # Listing releases avoids treating authentication or transport failures as a missing tag.
        response = self.command('gh', 'api', f'repos/{REPO}/releases?per_page=100', '--paginate', '--slurp')
        pages = json.loads(response.stdout)
        for page in pages:
            for release in page:
                tag = release['tag_name']
                if release['draft']:
                    continue
                if tag == f'gridcraft-v{self.version}':
                    raise DeployError('이미 공개된 버전입니다. 새 버전을 지정하세요.')
                match = re.fullmatch(r'gridcraft-v((?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*))', tag)
                if match and not release['prerelease'] and version_parts(match[1]) >= version_parts(self.version):
                    raise DeployError('공개된 안정 버전보다 높은 버전을 지정하세요.')

    def prepare(self, current):
        self.step(1, f'Grid {current} → {self.version} · 사전 확인')
        initial = self.git('rev-parse', 'HEAD')
        self.command('gh', 'auth', 'status', '--hostname', 'github.com')
        self.releases_preflight()
        self.command('git', 'fetch', '--no-tags', 'origin',
                     f'refs/heads/{BRANCH}:refs/remotes/origin/{BRANCH}',
                     f'refs/heads/{RELEASE_BRANCH}:refs/remotes/origin/{RELEASE_BRANCH}')
        for branch in [BRANCH, RELEASE_BRANCH]:
            self.ancestor(f'origin/{branch}', initial)
        # Authentication and fetch can take time while this checkout is shared with other work.
        # Reject changes before touching version files instead of silently including new source.
        self.clean()
        if self.git('branch', '--show-current') != BRANCH or self.git('rev-parse', 'HEAD') != initial:
            raise DeployError('사전 확인 중 브랜치 또는 소스 커밋이 변경되었습니다. 다시 실행하세요.')
        self.step(2, '버전 준비 및 개발 브랜치 푸시')
        self.sha = initial
        if self.version != current:
            self.command(self.cargo, 'run', '--quiet', '-p', 'xtask', '--no-default-features', '--', 'version', 'set', self.version, capture=False)
            if self.git('branch', '--show-current') != BRANCH or self.git('rev-parse', 'HEAD') != initial:
                raise DeployError('버전 준비 중 HEAD가 변경되었습니다. 현재 변경을 확인한 뒤 다시 실행하세요.')
            changed = set(self.git('diff', '--name-only').splitlines())
            if not changed or not changed <= VERSION_FILES or self.git('diff', '--cached', '--name-only'):
                raise DeployError('버전 준비 중 다른 변경이 발견되었습니다. 자동 스테이징을 중단했습니다.')
            self.command('git', 'add', '--', 'Cargo.toml', 'Cargo.lock')
            self.command('git', 'commit', '--only', '-m', f'chore: Grid {self.version} 릴리스 버전 준비', '--', 'Cargo.toml', 'Cargo.lock', capture=False)
            self.sha = self.git('rev-parse', 'HEAD')
            if self.git('rev-parse', f'{self.sha}^') != initial:
                raise DeployError('버전 커밋 전후로 다른 소스 커밋이 추가되었습니다. 배포 소스를 확인한 뒤 다시 실행하세요.')
        self.clean()
        if self.git('branch', '--show-current') != BRANCH or self.git('rev-parse', 'HEAD') != self.sha:
            raise DeployError('배포 소스를 확정하는 동안 브랜치 또는 HEAD가 변경되었습니다. 다시 실행하세요.')
        self.command('git', 'push', 'origin', f'{self.sha}:refs/heads/{BRANCH}', capture=False)
        previous = self.git('rev-parse', f'origin/{RELEASE_BRANCH}')
        self.step(3, f'배포 브랜치 반영 · {self.sha[:8]}')
        # Push the frozen SHA directly; do not switch a shared working tree's branch.
        self.command('git', 'push', 'origin', f'{self.sha}:refs/heads/{RELEASE_BRANCH}', capture=False)
        return previous != self.sha

    def runs(self):
        rows = self.gh_json('run', 'list', '--workflow', WORKFLOW, '--branch', RELEASE_BRANCH,
                            '--commit', self.sha, '--limit', '20', '--json', 'databaseId,headSha,headBranch,status,conclusion,url')
        return sorted((row for row in rows if row['headSha'] == self.sha and row['headBranch'] == RELEASE_BRANCH),
                      key=lambda row: row['databaseId'], reverse=True)

    def follow_build(self, pushed):
        self.step(4, 'GitHub Actions · macOS / Windows 빌드 및 설치 검증')
        ignored = set()
        if not pushed:
            existing = self.runs()
            if not existing or (existing[0]['status'] == 'completed' and existing[0]['conclusion'] != 'success'):
                ignored = {row['databaseId'] for row in existing}
                self.gh('workflow', 'run', WORKFLOW, '--ref', RELEASE_BRANCH, capture=False)
        deadline = time.monotonic() + 120
        while True:
            candidates = [row for row in self.runs() if row['databaseId'] not in ignored]
            if candidates:
                selected = candidates[0]
                break
            if time.monotonic() >= deadline:
                raise DeployError('정확한 소스 커밋의 Actions 실행을 찾지 못했습니다. GitHub Actions를 확인하고 같은 버전으로 다시 실행하세요.')
            time.sleep(3)
        run_id = str(selected['databaseId'])
        self.run_url = selected['url']
        print(self.run_url, flush=True)
        try:
            self.gh('run', 'watch', run_id, '--exit-status', '--interval', '10', capture=False)
        except DeployError as error:
            raise DeployError(f'{error}\n빌드 로그: gh run view {run_id} --repo {REPO} --log-failed') from error
        actual = self.gh_json('run', 'view', run_id, '--json', 'headSha,headBranch,status,conclusion,jobs')
        jobs = {job['name']: job['conclusion'] for job in actual['jobs']}
        if (actual['headSha'] != self.sha or actual['headBranch'] != RELEASE_BRANCH
                or actual['status'] != 'completed' or actual['conclusion'] != 'success'
                or any(jobs.get(name) != 'success' for name in ['version', 'macos', 'windows', 'draft'])):
            raise DeployError('소스 커밋 또는 필수 빌드 작업의 성공 상태가 일치하지 않습니다.')

    def validate_draft(self):
        tag = f'gridcraft-v{self.version}'
        release = self.gh_json('release', 'view', tag, '--json', 'tagName,isDraft,isPrerelease,targetCommitish,body,assets,url')
        expected = {f'grid-{self.version}-macos-universal.dmg', f'grid-cli-{self.version}-macos-universal.zip',
                    f'grid-{self.version}-windows-x64.msi', f'grid-{self.version}-windows-x64-portable.zip',
                    'SHA256SUMS.txt', 'GridUpdate.json', 'GridUpdate.sig'}
        assets = release['assets']
        if (release['tagName'] != tag or not release['isDraft'] or release['isPrerelease']
                or release['targetCommitish'] != self.sha or f'Source commit: `{self.sha}`' not in release['body']):
            raise DeployError('현재 소스 커밋의 릴리스 초안이 아닙니다. 공개를 중단합니다.')
        if (len(assets) != len(expected) or {asset['name'] for asset in assets} != expected
                or any(asset['size'] <= 0 or asset['state'] != 'uploaded' for asset in assets)):
            raise DeployError('릴리스 필수 자산 7개가 모두 업로드되지 않았습니다.')
        refs = self.git('ls-remote', '--tags', 'origin', f'refs/tags/{tag}', f'refs/tags/{tag}^{{}}').splitlines()
        if refs:
            peeled = [row for row in refs if row.endswith('^{}')]
            tag_sha = (peeled or refs)[0].split()[0]
            if tag_sha != self.sha:
                raise DeployError('기존 릴리스 태그가 다른 소스를 가리킵니다. 태그를 덮어쓰지 않습니다.')
        return release['url']

    def run(self):
        current = self.local_preflight()
        if self.args.dry_run:
            print(f'[dry-run] Grid {current} → {self.version}\n'
                  f'버전 커밋 → {BRANCH} 푸시 → {RELEASE_BRANCH} fast-forward → Actions 대기 → 자산 7개 확인 → 공개 확인\n'
                  '파일·커밋·원격은 변경하지 않았습니다. 인증과 원격 상태는 실제 실행 때 확인합니다.')
            return
        common = Path(self.git('rev-parse', '--git-common-dir'))
        if not common.is_absolute():
            common = ROOT / common
        with (common / 'grid-deploy.lock').open('a') as lock:
            try:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError as error:
                raise DeployError('같은 저장소에서 다른 배포가 진행 중입니다.') from error
            self.clean()
            pushed = self.prepare(current)
            self.follow_build(pushed)
            self.step(5, '릴리스 초안 확인')
            url = self.validate_draft()
            print(f'Grid {self.version} · 소스 {self.sha}\n필수 빌드 및 자산 7개 확인 완료\n{url}', flush=True)
            if not self.args.draft:
                try:
                    answer = input('이 버전을 공개하여 자동 업데이트를 배포할까요? [y/N] ').strip().lower()
                except EOFError:
                    answer = ''
                if answer in {'y', 'yes', '예'}:
                    self.validate_draft()
                    self.gh('release', 'edit', f'gridcraft-v{self.version}', '--draft=false', '--latest', capture=False)
                    print(f'\n배포 완료 · Grid {self.version}\n{url}')
                    return
            print(f'\n초안을 유지했습니다. 이어서 공개하려면 ./deploy.sh {self.version}을 실행하세요.')


def main():
    parser = argparse.ArgumentParser(prog='./deploy.sh', description='Grid 버전 준비 → GitHub Actions 빌드 → 마지막 공개 확인 (macOS/Linux)')
    parser.add_argument('version', nargs='?', help='배포 버전 X.Y.Z. 생략하면 현재 버전의 다음 패치 버전')
    parser.add_argument('--dry-run', action='store_true', help='변경·네트워크 요청 없이 예정된 절차만 출력')
    parser.add_argument('--draft', action='store_true', help='빌드 및 초안 확인까지만 진행하고 공개하지 않음')
    args = parser.parse_args()
    deployment = Deployment(args)
    try:
        deployment.run()
    except (DeployError, OSError, ValueError, KeyError, TypeError) as error:
        print(f'\n배포 중단: {error}', file=sys.stderr)
        if deployment.version:
            print(f'상태 확인 후 재시도: ./deploy.sh {deployment.version}', file=sys.stderr)
        if deployment.run_url:
            print(deployment.run_url, file=sys.stderr)
        return 1
    except KeyboardInterrupt:
        print(f'\n로컬 대기를 중단했습니다. 이미 시작한 Actions는 계속 실행됩니다.\n재시도: ./deploy.sh {deployment.version}', file=sys.stderr)
        return 130
    return 0


if __name__ == '__main__':
    sys.exit(main())
