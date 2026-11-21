#!/usr/bin/env python3
"""Isolated renderer/publication contract checks; no public assets are touched."""
import os
import json
import re
from pathlib import Path
import stat
import shlex
import shutil
import subprocess
import tempfile
import xml.etree.ElementTree as ET

REPO = Path(__file__).resolve().parent.parent
RENDER = str(REPO / 'scripts/ci-coverage-badges.sh')
PUBLISH = str(REPO / 'scripts/publish-badges.sh')


def run(*args, ok=True):
    result = subprocess.run(args, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    assert (result.returncode == 0) == ok, (args, result.stderr.decode())
    return result


with tempfile.TemporaryDirectory() as directory:
    os.chdir(directory)
    os.umask(0o077)
    for kind in ('version', 'test', 'coverage'):
        run(RENDER, kind)
    for kind, value, color in [
        ('version', '1.2.3', '#007ec6'),
        ('version', '1.2.3-very-long-prerelease.123456789', '#007ec6'),
        ('test', 'passing', '#4c1'), ('test', 'failing', '#e05d44'),
        ('test', 'pending', '#9f9f9f'), ('coverage', '0', '#e05d44'),
        ('coverage', '69', '#e05d44'), ('coverage', '70', '#dfb317'),
        ('coverage', '87.4', '#dfb317'), ('coverage', '90', '#4c1'),
        ('coverage', '100', '#4c1'), ('coverage', '-', '#9f9f9f'),
    ]:
        run(RENDER, kind, '--value', value)
        root = ET.parse(f'coverage/{kind}.svg').getroot()
        assert color in Path(f'coverage/{kind}.svg').read_text()
        if value == '1.2.3':
            normal_width = int(root.attrib['width'])
        if value == '1.2.3-very-long-prerelease.123456789':
            assert int(root.attrib['width']) > normal_width
        if value == '87.4':
            assert root.attrib['aria-label'] == 'coverage: 87%'
    run(RENDER, 'version', '--text', 'A<&"', '--value', 'B>&"', '--suffix', '&',
        '--size', '14', '--height', '30', '--label-width', '80',
        '--value-width', '120', '--color', '#abcdef', '--output', 'custom/badge.svg')
    root = ET.parse('custom/badge.svg').getroot()
    assert root.attrib['width'] == '200' and root.attrib['height'] == '30'
    assert root.attrib['aria-label'] == 'A<&": B>&"&'
    Path('directory-output').mkdir()
    run(RENDER, 'version', '--output', 'directory-output', ok=False)
    original = Path('coverage/coverage.svg').read_bytes()
    for args in [
        ['unknown'], ['coverage', '--bogus'], ['coverage', '--value'],
        *[['coverage', '--value', v] for v in ('-1', '101', 'nan', '1e2', 'x', '')],
        *[['coverage', flag, v] for flag, v in (
            ('--size', '5'), ('--size', '25'), ('--height', '13'), ('--height', '65'),
            ('--label-width', '19'), ('--value-width', '1001'),
            ('--height', '999999999999999999999999999'), ('--color', 'red'),
            ('--color', '#1234567'), ('--text', 'bad\ntext'), ('--value', '\t'))],
    ]:
        run(RENDER, *args, ok=False)
        assert Path('coverage/coverage.svg').read_bytes() == original
    assert not list(Path('.').rglob('*.tmp.*'))
    for svg in Path('.').rglob('*.svg'):
        ET.parse(svg)
    root = Path(directory) / 'assets'
    root.mkdir()
    args = [PUBLISH, str(root), 'idle-org', 'tiny-wb', 'main']
    names = ['version', 'test', 'coverage']
    run(*args, *names)
    destination = root / 'idle-org/tiny-wb/main'
    for path in (root/'idle-org', root/'idle-org/tiny-wb', destination):
        assert stat.S_IMODE(path.stat().st_mode) == 0o755
    old_inode = (destination/'test.svg').stat().st_ino
    run(*args, *names)
    assert (destination/'test.svg').stat().st_ino != old_inode
    for name in names:
        target = destination/f'{name}.svg'
        assert target.read_bytes() == Path(f'coverage/{name}.svg').read_bytes()
        assert stat.S_IMODE(target.stat().st_mode) == 0o644
    run(*args, 'test', 'test', ok=False)
    run(*args, 'invalid', ok=False)
    run(PUBLISH, 'relative', 'o', 'p', 'main', 'test', ok=False)
    run(PUBLISH, str(root/'missing'), 'o', 'p', 'main', 'test', ok=False)
    run(PUBLISH, str(root), '..', 'p', 'main', 'test', ok=False)
    saved = Path('coverage/test.svg').read_bytes()
    Path('coverage/test.svg').unlink()
    Path('coverage/test.svg').symlink_to('version.svg')
    run(*args, 'test', ok=False)
    Path('coverage/test.svg').unlink()
    Path('coverage/test.svg').touch()
    run(*args, 'test', ok=False)
    Path('coverage/test.svg').write_bytes(saved)
    (root/'linked').symlink_to(root/'idle-org', target_is_directory=True)
    run(PUBLISH, str(root), 'linked', 'tiny-wb', 'main', 'test', ok=False)
    (root/'idle-org/linked').symlink_to(root/'idle-org/tiny-wb', target_is_directory=True)
    run(PUBLISH, str(root), 'idle-org', 'linked', 'main', 'test', ok=False)
    (root/'file').touch()
    run(PUBLISH, str(root), 'file', 'p', 'main', 'test', ok=False)
    # Migration unlinks the old branch link, preserving its target and contents.
    old = destination.with_name('old')
    destination.rename(old)
    destination.symlink_to(old, target_is_directory=True)
    run(*args, *names)
    assert destination.is_dir() and not destination.is_symlink()
    assert (old/'test.svg').read_bytes() == saved
    # Failed copy, chmod and rename must preserve the published badge and
    # remove the unfinished temporary file.
    tools = Path(directory) / 'fake-bin'
    tools.mkdir()
    old_path = os.environ['PATH']
    for command in ('cp', 'chmod', 'mv'):
        stub = tools / command
        stub.write_text('#!/bin/sh\nexit 1\n')
        stub.chmod(0o755)
        os.environ['PATH'] = str(tools) + ':' + old_path
        run(*args, 'test', ok=False)
        os.environ['PATH'] = old_path
        stub.unlink()
        assert (destination/'test.svg').read_bytes() == saved
        assert not list(destination.glob('.*.svg.*'))
    stub = tools / 'mv'
    stub.write_text('#!/bin/sh\nkill -TERM "$PPID"\nexit 1\n')
    stub.chmod(0o755)
    os.environ['PATH'] = str(tools) + ':' + old_path
    run(*args, 'test', ok=False)
    run(RENDER, 'coverage', '--value', '50', ok=False)
    os.environ['PATH'] = old_path
    stub.unlink()
    assert Path('coverage/coverage.svg').read_bytes() == original
    assert not list(Path('coverage').glob('*.tmp.*'))
    assert not list(destination.glob('.*.svg.*'))
    # Exercise cleanup with real Cargo metadata from this repository while
    # keeping all generated files in the isolated directory.
    Path('scripts').symlink_to(REPO/'scripts', target_is_directory=True)
    cargo = tools/'cargo'
    cargo.write_text('#!/bin/sh\nexec ' + shlex.quote(shutil.which('cargo')) +
                     ' metadata --manifest-path ' + shlex.quote(str(REPO/'Cargo.toml')) +
                     ' --no-deps --locked --format-version 1\n')
    cargo.chmod(0o755)
    os.environ['PATH'] = str(tools) + ':' + old_path
    os.environ['BUILD_NUMBER'] = '123'
    for marker in ('tests-passed', 'coverage-ready'):
        Path(f'coverage/.{marker}-122').touch()
    run(str(REPO/'scripts/finalize-badges.sh'))
    assert ET.parse('coverage/test.svg').getroot().attrib['aria-label'] == 'tests: failing'
    assert ET.parse('coverage/coverage.svg').getroot().attrib['aria-label'] == 'coverage: -%'
    run(RENDER, 'coverage', '--value', '87.4')
    for marker in ('tests-passed', 'coverage-ready'):
        Path(f'coverage/.{marker}-123').touch()
    run(str(REPO/'scripts/finalize-badges.sh'))
    assert ET.parse('coverage/test.svg').getroot().attrib['aria-label'] == 'tests: passing'
    assert ET.parse('coverage/coverage.svg').getroot().attrib['aria-label'] == 'coverage: 87%'
    assert not list(Path('coverage').glob('.*-123'))
    os.environ['PATH'] = old_path
    cargo.unlink()
    # A coverage-floor failure can write a valid report and exit silently.
    # Exercise the collector with real jq and rendering, but no Rust rebuild.
    cargo.write_text('''#!/bin/sh
[ "$*" = coverage ] || exit 99
echo "fake compiler output"
echo "fake test output" >&2
if [ -n "$FAKE_COVERAGE_REPORT" ]; then
    printf '%s\\n' "$FAKE_COVERAGE_REPORT" > coverage/summary.json
fi
exit "$FAKE_COVERAGE_STATUS"
''')
    cargo.chmod(0o755)
    os.environ['PATH'] = str(tools) + ':' + old_path
    for percentage, status in ((91.49, 0), (85, 0), (84.99, 1), (91.49, 7)):
        os.environ['FAKE_COVERAGE_REPORT'] = json.dumps({'data': [{
            'totals': {'lines': {'percent': percentage}},
            'files': [{'filename': 'src/example.rs', 'summary': {
                'lines': {'percent': 80, 'count': 100, 'covered': 80}}}],
        }]})
        os.environ['FAKE_COVERAGE_STATUS'] = str(status)
        before = Path('coverage/coverage.svg').read_bytes()
        result = run(str(REPO/'scripts/collect-coverage.sh'), ok=status == 0)
        assert result.returncode == status
        assert result.stdout.decode() == f'Final line coverage: {percentage:.2f}%\n'
        assert 'fake compiler output' in Path('coverage/coverage.log').read_text()
        assert 'fake test output' in Path('coverage/coverage.log').read_text()
        if status:
            assert '--fail-under-lines' in result.stderr.decode()
            assert 'src/example.rs' not in result.stderr.decode()
            assert 'coverage/coverage.log' in result.stderr.decode()
            assert Path('coverage/coverage.svg').read_bytes() == before
        else:
            assert ET.parse('coverage/coverage.svg').getroot().attrib['aria-label'] == f'coverage: {percentage:.0f}%'
        assert not Path('coverage/summary.json').exists()
    for report in ('', '{broken', '{"data": []}', '{"data": [{}, {}]}',
                   *[json.dumps({'data': [{'totals': {'lines': {'percent': p}}}]})
                     for p in (None, '90', -1, 101)]):
        for status in (0, 7):
            # An old successful report must not hide a missing current report.
            Path('coverage/summary.json').write_text('{"data":[{"totals":{"lines":{"percent":99}}}]}')
            os.environ['FAKE_COVERAGE_REPORT'] = report
            os.environ['FAKE_COVERAGE_STATUS'] = str(status)
            result = run(str(REPO/'scripts/collect-coverage.sh'), ok=False)
            assert result.returncode == (status or 1)
            assert 'ERROR: cargo coverage produced' in result.stderr.decode()
            assert not Path('coverage/summary.json').exists()
    os.environ['PATH'] = old_path
    del os.environ['FAKE_COVERAGE_REPORT'], os.environ['FAKE_COVERAGE_STATUS']
    cargo.unlink()
    root.chmod(0o555)
    if os.geteuid() != 0:
        run(*args, 'test', ok=False)
    root.chmod(0o700)
    (destination/'test.svg').unlink()
    (destination/'test.svg').mkdir()
    run(*args, 'test', ok=False)
    assert not list(destination.glob('.*.svg.*'))

# Keep the alias's dependency filter portable across cache roots without
# excluding any production module (including low-coverage startup/scan code).
alias = next(line for line in (REPO/'.cargo/config.toml').read_text().splitlines()
             if line.startswith('coverage = '))
arguments = shlex.split(alias.split('"', 1)[1].rsplit('"', 1)[0])
pattern = arguments[arguments.index('--ignore-filename-regex') + 1]
assert arguments[arguments.index('--fail-under-lines') + 1] == '85'
for cache in ('/root/.cargo', '/home/user/.cargo', '/custom/cargo-cache'):
    assert re.search(pattern, cache + '/registry/src/index.crates.io-123/serde-1.0/src/lib.rs')
for source in (REPO/'src').rglob('*.rs'):
    assert not re.search(pattern, str(source))

readme = (REPO/'README.md').read_text()
for name in ('version', 'test', 'coverage'):
    assert readme.count(f'https://assets.dandaedre.com/coverage/idle-org/tiny-wb/main/{name}.svg') == 1
pipeline = (REPO/'Jenkinsfile').read_text()
assert "CARGO_LLVM_COV_VERSION = '0.9.1'" in pipeline
assert "artifacts: 'coverage/version.svg,coverage/test.svg,coverage/coverage.svg'" in pipeline
assert 'fingerprint: true, allowEmptyArchive: false' in pipeline
print('Badge rendering, XML, validation, coverage diagnostics, publication, README and archive checks passed.')
