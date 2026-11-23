#!/usr/bin/env python3
"""Install this local-test payload for one Linux user; preserve user data."""
import argparse
import hashlib
import json
import os
import shutil
import sys
from pathlib import Path


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def desktop_quote(value):
    quoted = '"' + value.replace('\\', '\\\\').replace('"', '\\"').replace('`', '\\`').replace('$', '\\$').replace('%', '%%') + '"'
    return quoted.replace('\\', '\\\\')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--uninstall', action='store_true')
    args = parser.parse_args()
    if sys.platform != 'linux' or os.geteuid() == 0:
        parser.error('Run as your ordinary Linux desktop user, without sudo.')
    here = Path(__file__).resolve().parent
    manifest = json.loads((here/'payload-manifest.json').read_text())
    slug = manifest['install_id']
    if not slug.startswith('s33d-alpha-preview-test-') or '/' in slug or '..' in slug:
        raise ValueError('Invalid test installation identity')
    home = Path.home().resolve()
    if '\n' in str(home) or '\r' in str(home):
        raise ValueError('Home paths containing newlines are unsupported')
    install = home/'.local/opt'/slug
    shortcut = home/'.local/share/applications'/(slug+'.desktop')
    launcher = home/'.local/bin'/slug
    if not install.resolve().is_relative_to(home):
        raise ValueError('Install path must stay within the current home directory')
    expected = {x['path']: x for x in manifest['files']}
    for name in expected:
        p = Path(name)
        if p.is_absolute() or '..' in p.parts or name == 'payload-manifest.json':
            raise ValueError('Unsafe payload entry')
    if args.uninstall:
        if here != install.resolve():
            parser.error('Run --uninstall using the installed copy of this script.')
        # Remove only recorded unmodified files; never recurse into user data.
        retained = []
        for name, entry in expected.items():
            p = install/name
            if p.is_symlink() or not p.resolve().is_relative_to(install.resolve()):
                retained.append(name)
            elif p.is_file() and digest(p) == entry['sha256']:
                p.unlink()
            elif p.exists():
                retained.append(name)
        record = install/'install-receipt.json'
        if record.is_file():
            receipt = json.loads(record.read_text())
            if shortcut.is_file() and digest(shortcut) == receipt['shortcut_sha256']:
                shortcut.unlink()
            if launcher.is_symlink() and launcher.resolve() == install/'s33d-app':
                launcher.unlink()
            record.unlink()
        (install/'payload-manifest.json').unlink(missing_ok=True)
        for p in sorted(install.rglob('*'), key=lambda x:len(x.parts), reverse=True):
            if p.is_dir() and not p.is_symlink():
                try:
                    p.rmdir()
                except OSError:
                    pass
        try:
            install.rmdir()
        except OSError:
            pass
        print('Uninstalled test application. User settings/projects are preserved.')
        if retained:
            print('Modified/unknown files remain in:', install)
        return
    if install.exists() or install.is_symlink() or launcher.exists() or launcher.is_symlink() or shortcut.exists():
        parser.error('This dated test installation already exists; uninstall it first. No files changed.')
    actual = {p.relative_to(here).as_posix() for p in here.rglob('*') if p.is_file()}
    if actual != set(expected) | {'payload-manifest.json'}:
        raise ValueError('Unexpected or missing installer payload files')
    for name, entry in expected.items():
        p = here/name
        if p.is_symlink() or digest(p) != entry['sha256']:
            raise ValueError('Payload checksum failed: '+name)
    shutil.copytree(here, install, symlinks=False)
    (install/'s33d-app').chmod(0o755)
    (install/'s33d-agent').chmod(0o755)
    launcher.parent.mkdir(parents=True, exist_ok=True)
    shortcut.parent.mkdir(parents=True, exist_ok=True)
    launcher.symlink_to(install/'s33d-app')
    shortcut.write_text('[Desktop Entry]\nType=Application\nVersion=1.0\nName=S33D Alpha Preview Test\n'
                        'Comment=Local test build; settings are shared with S33D\n'
                        'Exec='+desktop_quote(str(install/'s33d-app'))+'\n'
                        'Icon='+str(install/'assets/icons/s33d-sprouted.png').replace('\\','\\\\')+'\n'
                        'Terminal=false\nCategories=Graphics;3DGraphics;Engineering;\n', encoding='utf-8')
    (install/'install-receipt.json').write_text(json.dumps({'shortcut_sha256':digest(shortcut)},indent=2)+'\n')
    print('Installed:', install)
    print('Launch:', launcher)
    print('Uninstall: python3', str(install/'install.py'), '--uninstall')
    print('No application was launched. No MCP registration or user settings were changed.')


if __name__ == '__main__':
    main()
