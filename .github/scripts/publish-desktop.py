#!/usr/bin/env python3
"""Publish one approved desktop build; never replace assets or the Mac latest release."""
import argparse
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[2]
REPOSITORY = 'Pengjingyu1992/Kaleiform'
CHECKS = {'Windows x64 packages', 'Linux packages', 'CI', 'Packaging lint'}


def load_module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


LINUX = load_module('desktop_linux_audit', ROOT / 'packaging/linux/publish.py')
WINDOWS = load_module('desktop_windows_audit', ROOT / 'packaging/windows/privacy.py')


def validate_request(request, repository):
    if repository != REPOSITORY:
        raise ValueError('Desktop publication is restricted to the requested Kaleiform repository')
    if not re.fullmatch(r'[0-9a-f]{40}', request.get('source_sha', '')):
        raise ValueError('Publication requires a full source commit')
    if not re.fullmatch(r'[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?', request.get('version', '')):
        raise ValueError('Invalid desktop version')
    checks = request.get('checks', {})
    if set(checks) != CHECKS or any(type(value) is not int or value <= 0 for value in checks.values()):
        raise ValueError('Pin all four required workflow runs')


def run_ready(run, request, name):
    if (run.get('head_sha') != request['source_sha'] or run.get('head_branch') != 'main'
            or run.get('event') != 'push' or run.get('name') != name
            or run.get('repository', {}).get('full_name') != REPOSITORY
            or run.get('head_repository', {}).get('full_name') != REPOSITORY):
        raise ValueError('A required workflow run does not match the approved source and repository')
    if run.get('status') != 'completed':
        return False
    if run.get('conclusion') != 'success':
        raise ValueError(f'Required workflow did not pass: {name}')
    return True


def release_payload(request, body):
    return {'tag_name': 'desktop-v' + request['version'], 'target_commitish': request['source_sha'],
            'name': '虹构 Windows / Linux 预览 ' + request['version'], 'body': body,
            'draft': False, 'prerelease': True, 'make_latest': 'false'}


def api(endpoint, payload=None, pages=False):
    command = ['gh', 'api', endpoint]
    if pages:
        command += ['--paginate', '--slurp']
    if payload is not None:
        command += ['--method', 'POST', '--input', '-']
    result = subprocess.run(command, input=None if payload is None else json.dumps(payload, ensure_ascii=False),
                            text=True, capture_output=True, check=True)
    return json.loads(result.stdout)


def releases():
    return [release for page in api(f'repos/{REPOSITORY}/releases?per_page=100', pages=True) for release in page]


def hydrate_digests(release, tag, expected):
    result = copy.deepcopy(release)
    for asset in result['assets']:
        if asset['name'] in expected and not asset.get('digest'):
            with tempfile.TemporaryDirectory(prefix='kaleiform-remote-check-') as directory:
                subprocess.run(['gh', 'release', 'download', tag, '--repo', REPOSITORY,
                                '--pattern', asset['name'], '--dir', directory], check=True)
                asset['digest'] = 'sha256:' + LINUX.sha256(Path(directory) / asset['name'])
    return result


def audit_windows(directory, version):
    base = f'vectorcraft-{version}-windows-x64'
    expected = {base + '.msi', base + '-portable.zip'}
    manifest = directory / (base + '.sha256')
    hashes = {}
    for line in manifest.read_text().splitlines():
        match = re.fullmatch(r'([0-9a-f]{64})  ([^/\\:]+)', line)
        if not match or match[2] not in expected or match[2] in hashes:
            raise ValueError('Invalid Windows release checksum manifest')
        hashes[match[2]] = match[1]
    if set(hashes) != expected or any(LINUX.sha256(directory / name) != digest for name, digest in hashes.items()):
        raise ValueError('Windows release payload checksum mismatch')
    WINDOWS.check(directory / (base + '-portable.zip'))
    hashes[manifest.name] = LINUX.sha256(manifest)
    return hashes


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check-request', action='store_true', help='Validate the pinned local request without network or writes')
    args = parser.parse_args()
    request = json.loads((ROOT / '.github/desktop-release.json').read_text())
    validate_request(request, os.environ.get('GITHUB_REPOSITORY', REPOSITORY))
    source = request['source_sha']
    cargo = subprocess.check_output(['git', 'show', source + ':Cargo.toml'], cwd=ROOT, text=True)
    if tomllib.loads(cargo)['workspace']['package']['version'] != request['version']:
        raise ValueError('Package version does not match the approved source')
    if args.check_request:
        print('Desktop release request matches the source version; no network or publication performed')
        return
    if os.environ.get('GITHUB_ACTIONS') != 'true':
        raise ValueError('Publish through the approved GitHub workflow; use --check-request locally')
    trigger = os.environ.get('DESKTOP_TRIGGER_RUN_ID', '')
    if trigger and int(trigger) not in request['checks'].values():
        print('This workflow completion is outside the pinned publication request')
        return
    runs = {name: api(f'repos/{REPOSITORY}/actions/runs/{run_id}') for name, run_id in request['checks'].items()}
    readiness = [run_ready(run, request, name) for name, run in runs.items()]
    if not all(readiness):
        print('::notice::Desktop publication is waiting for all four approved workflow runs')
        return
    with tempfile.TemporaryDirectory(prefix='kaleiform-desktop-release-') as temporary:
        work = Path(temporary)
        assets = work / 'assets'
        assets.mkdir()
        hashes = {}
        for name, artifact, folder in [('Windows x64 packages', 'kaleiform-windows-x64', 'windows'),
                                       ('Linux packages', 'kaleiform-linux-x86_64', 'linux')]:
            directory = work / folder
            subprocess.run(['gh', 'run', 'download', str(request['checks'][name]), '--repo', REPOSITORY,
                            '--name', artifact, '--dir', str(directory)], check=True)
            verified = audit_windows(directory, request['version']) if folder == 'windows' else LINUX.verify_artifacts(directory, request['version'])
            if {path.name for path in directory.iterdir()} != set(verified):
                raise ValueError('Unexpected file in a desktop distribution artifact')
            for filename, digest in verified.items():
                if filename in hashes:
                    raise ValueError('Duplicate desktop release filename')
                shutil.copy2(directory / filename, assets / filename)
                hashes[filename] = digest
        prefix = 'kaleiform-' + request['version'] + '-desktop'
        archive = assets / (prefix + '-source.tar.gz')
        subprocess.run(['git', 'archive', '--format=tar.gz', '--prefix=Kaleiform/', '--output', str(archive), source], cwd=ROOT, check=True)
        hashes[archive.name] = LINUX.sha256(archive)
        build = assets / (prefix + '-BUILD-INFO.json')
        info = {'source_sha': source, 'version': request['version'], 'windows_authenticode_signed': False,
                'workflows': {name: {'id': request['checks'][name], 'url': run['html_url'], 'conclusion': run['conclusion']} for name, run in runs.items()},
                'payload_sha256': hashes}
        build.write_text(json.dumps(info, ensure_ascii=False, indent=2, sort_keys=True) + '\n')
        hashes[build.name] = LINUX.sha256(build)
        manifest = assets / (prefix + '.sha256')
        manifest.write_text(''.join(f'{digest}  {filename}\n' for filename, digest in sorted(hashes.items())))
        hashes[manifest.name] = LINUX.sha256(manifest)
        body = ('Windows 与 Linux 对齐仓库 Mac 工作区的简洁层级，优化中英文字体、文档标签和紧凑布局。\n\n'
                '- Windows 10/11 x64：选择 MSI 或 portable.zip；当前为未签名预览包。\n'
                '- Linux x86_64：选择 DEB、RPM、AppImage 或 tar.gz；最低 glibc 2.35。RPM 安装未在本轮实测。\n'
                '- 原生 Windows 安装/便携 CLI 与桌面验收、Linux 最小安装运行及隐私检查均通过。\n'
                '- 所有包附 SHA-256、许可证及来源记录；没有新增遥测或作品上传。\n'
                '- 性能预算保留原阈值与失败记录；云端大型画布渲染仍有超标，不能视作全面性能通过。\n'
                '- 真机显卡、输入法、数位板与多显示器仍有验收边界；本轮未重新实测 Mac 硬件。\n'
                '- 此独立预览不替换任何原 Mac 附件，也不更改 latest 标记。\n\n'
                f'源码：[{source}](https://github.com/{REPOSITORY}/commit/{source})\n\n'
                + '\n'.join(f'- [{name}]({run["html_url"]})' for name, run in runs.items()))
        payload = release_payload(request, body)
        tag = payload['tag_name']
        refs = subprocess.check_output(['git', 'ls-remote', 'origin', 'refs/tags/' + tag, 'refs/tags/' + tag + '^{}'], cwd=ROOT, text=True).splitlines()
        if refs:
            peeled = next((line.split()[0] for line in refs if line.endswith('^{}')), refs[0].split()[0])
            if peeled != source:
                raise ValueError('Refusing to publish against a tag for another source commit')
        before = releases()
        release = next((item for item in before if item['tag_name'] == tag), None)
        if release is None:
            release = api(f'repos/{REPOSITORY}/releases', payload)
        if release['draft'] or not release['prerelease']:
            raise ValueError('The desktop target must be a published prerelease')
        if api(f'repos/{REPOSITORY}/commits/{tag}')['sha'] != source:
            raise ValueError('Desktop release tag does not resolve to the approved source')
        pending = LINUX.pending_assets(hydrate_digests(release, tag, hashes), assets, hashes)
        if pending:
            subprocess.run(['gh', 'release', 'upload', tag, '--repo', REPOSITORY, '--', *(str(path) for path in pending)], check=True)
        after = releases()
        by_id = {item['id']: item for item in after}
        for original in before:
            current = by_id.get(original['id'])
            if current is None or current['tag_name'] != original['tag_name']:
                raise ValueError('An existing release changed during publication')
            LINUX.verify_retained(original, current)
        published = next(item for item in after if item['tag_name'] == tag)
        if LINUX.pending_assets(hydrate_digests(published, tag, hashes), assets, hashes):
            raise ValueError('The desktop release did not retain every verified upload')
        url = published['html_url']
        print('::notice::Published verified Windows/Linux preview: ' + url)
        summary = os.environ.get('GITHUB_STEP_SUMMARY')
        if summary:
            with open(summary, 'a', encoding='utf-8') as stream:
                stream.write(f'Windows/Linux preview: {url}\n\nSource: `{source}`. Existing release assets retained.\n')


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        raise SystemExit('Desktop publication stopped: ' + str(error)) from error
