"""Build and test on the current desktop OS. Never install/remove the user's Grid."""
import os
from contextlib import contextmanager
from pathlib import Path
import tempfile

from release_assets import DeployError, FONTS_SHA, package_names


def prepare_fonts(deployment):
    d = deployment
    configured = d.env.get('CRAFT_FONTS_DIR')
    fonts = Path(configured).expanduser().resolve() if configured else d.common/'grid-release-fonts'/FONTS_SHA
    if not fonts.exists():
        if configured:
            raise DeployError(f'CRAFT_FONTS_DIR가 없습니다: {fonts}')
        fonts.parent.mkdir(parents=True, exist_ok=True)
        d.command('git', 'clone', '--no-checkout', 'https://github.com/storytold/craft-fonts.git', fonts, capture=False)
        d.command('git', '-C', fonts, 'checkout', '--detach', FONTS_SHA, capture=False)
    actual = d.command('git', '-C', fonts, 'rev-parse', 'HEAD').stdout.strip()
    dirty = d.command('git', '-C', fonts, 'status', '--porcelain').stdout.strip()
    if actual != FONTS_SHA or dirty:
        raise DeployError('릴리스 글꼴은 문서화된 고정 커밋의 깨끗한 체크아웃이어야 합니다.')
    d.env.update(CRAFT_FONTS_DIR=str(fonts), CRAFT_FONTS_REQUIRED='1')


@contextmanager
def committed_source(d):
    # Build only committed files. Untracked work and edits in the shared checkout
    # cannot become release input while another session uses it.
    with tempfile.TemporaryDirectory(prefix='grid-build-source-') as scratch:
        source = Path(scratch)/'source'
        d.command('git', 'clone', '--quiet', '--shared', '--no-checkout', d.root, source)
        d.command('git', '-C', source, 'checkout', '--quiet', '--detach', d.sha)
        yield source


def build(d, output):
    prepare_fonts(d)
    d.assert_source()
    with committed_source(d) as source:
        return build_source(d, output, source)


def build_source(d, output, source):
    d.env.update(DIST=str(output), GRIDCRAFT_VERSION=d.version, GRIDCRAFT_BUILD_SHA=d.sha,
                 CARGO_TARGET_DIR=str(d.root/'target'))
    d.env.pop('GRID_UPDATE_SMOKE_PACKAGE', None)
    d.env.pop('GRID_UPDATE_SMOKE_INSTALL', None)
    def run(*args, **kwargs):
        return d.command(*args, cwd=source, **kwargs)

    run(os.sys.executable, 'packaging/test_fork_contract.py', capture=False)
    run(d.cargo, 'test', '--locked', '-p', 'gridcraft-update', capture=False)
    if d.args.platform == 'macos':
        run('bash', 'packaging/macos/package.sh', '--arch', 'universal', capture=False)
        cli = d.root/'target/macos-package/bin/gridcraft-cli'
        run(d.cargo, 'test', '--locked', '-p', 'gridcraft-update', '--lib', 'native_dmg_roundtrip', '--', '--ignored',
                  capture=False, env=dict(d.env, GRID_UPDATE_SMOKE_PACKAGE=str(output/package_names(d.version, 'macos')[0])))
    else:
        run('pwsh', '-NoProfile', '-File', 'packaging/windows/package.ps1', '-Arch', 'x64', capture=False)
        cli = d.root/'target/windows-package/x64/grid-cli.exe'
        # The MSI's static policy check is part of package.ps1. Installation tests are
        # deliberately limited to a disposable machine; never touch a personal install.
        run(d.cargo, 'test', '--locked', '-p', 'gridcraft-update', '--lib', 'native_windows_portable_roundtrip', '--', '--ignored',
                  capture=False, env=dict(d.env, GRID_UPDATE_SMOKE_PACKAGE=str(output/package_names(d.version, 'windows')[1])))
    if d.version not in d.command(cli, '--version').stdout:
        raise DeployError('빌드한 CLI의 버전이 다릅니다.')
    if d.command(cli, 'eval', '=SUM(1,2,3)').stdout.strip() != '6':
        raise DeployError('CLI 계산 검증에 실패했습니다.')
    with tempfile.TemporaryDirectory(prefix='grid-cli-smoke-') as scratch:
        book = Path(scratch)/'budget.xlsx'
        d.command(cli, 'run', '--sample', 'budget', '--out', book, capture=False)
        d.command(cli, 'cat', book, '--range', 'B4:F8', capture=False)
    d.assert_source()
    return ['fork-identity', 'updater-tests', 'package-policy', 'cli-smoke']
