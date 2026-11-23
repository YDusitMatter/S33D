#!/usr/bin/env python3
"""Assemble a native x86_64 Linux local-test installer from verified build outputs."""
import argparse
import hashlib
import json
import os
import platform
import re
import shutil
import struct
import subprocess
import tarfile
import zipfile
from pathlib import Path


def digest(p):
    with p.open('rb') as stream:
        return hashlib.file_digest(stream,'sha256').hexdigest()


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--release-directory',type=Path,required=True)
    parser.add_argument('--source-archive',type=Path,required=True)
    parser.add_argument('--output-directory',type=Path,required=True)
    parser.add_argument('--date',required=True)
    args=parser.parse_args()
    if platform.system() != 'Linux' or platform.machine() != 'x86_64':
        parser.error('Run on native x86_64 Linux (WSL Linux is acceptable for building).')
    if not re.fullmatch(r'\d{4}-\d{2}-\d{2}',args.date):
        parser.error('Date must be YYYY-MM-DD.')
    root=Path(__file__).resolve().parent.parent
    with zipfile.ZipFile(args.source_archive) as z:
        source=json.loads(z.read('source-manifest.json').decode('utf-8-sig'))
        for item in source['files']:
            if item['path'].startswith('S33D/'):
                p=root/item['path'][5:]
                if not p.is_file() or digest(p) != item['sha256']:
                    raise ValueError('Source archive does not match this frozen source: '+item['path'])
    version=source['version']
    if version != '0.1.0-alpha.1':
        raise ValueError('Review package identity before changing Alpha Preview version.')
    dependencies={}
    for name in ('s33d-app','s33d-agent'):
        binary=args.release_directory/name
        data=binary.read_bytes()
        if data[:6] != b'\x7fELF\x02\x01' or struct.unpack_from('<H',data,18)[0] != 62:
            raise ValueError('Expected x86_64 ELF executable: '+name)
        for value in (str(Path.home()),str(root),str(args.release_directory.parent)):
            if value.encode() in data:
                raise ValueError('Unmapped build/user path in '+name)
        output=subprocess.run(['ldd',str(binary)],capture_output=True,text=True,check=True).stdout
        if 'not found' in output:
            raise ValueError('Missing runtime dependency in '+name)
        dependencies[name]=sorted(set(re.findall(r'^\s*(\S+\.so(?:\.[\w.]+)*)',output,re.M)))
    bridge=subprocess.run([str(args.release_directory/'s33d-agent'),'--version'],capture_output=True,text=True,check=True).stdout.strip()
    if bridge != 's33d-agent '+version:
        raise ValueError('Agent version mismatch')
    if args.output_directory.exists():
        raise ValueError('Use a new output directory; preserve previous packages.')
    stage=args.output_directory/'S33D-Alpha-Preview-Test'
    stage.mkdir(parents=True)
    files=['LICENSE','THIRD_PARTY_LICENSES.md','README.md',
           'assets/icons/s33d-sprouted.png','assets/icons/brand-manifest.json',
           'assets/fonts/OFL-NotoSansSC.txt','assets/licenses/occt/LICENSE_LGPL_21.txt',
           'assets/licenses/occt/OCCT_LGPL_EXCEPTION.txt','Docs/UserManual.md',
           'Docs/UserManual.en.md','Docs/UserManual.zh-CN.md','Docs/AgentControl.md', 'Docs/PhotoToModel.md',
           'Docs/ExactGeometryBuild.md','Docs/FastenerGenerators.md','Docs/ThreadPrinting.md',
           'Docs/LinuxPackaging.md','packaging/RELEASE-NOTES.md',
           'agent-config/codex.toml.example','agent-config/claude-code.mcp.json.example',
           'skills/s33d-modeling/SKILL.md','skills/s33d-modeling/agents/openai.yaml',
           'skills/s33d-modeling/references/modeling-patterns.md',
           'skills/s33d-modeling/references/photo-reconstruction.md',
           'skills/s33d-modeling/references/visual-validation.md',
           'skills/s33d-modeling/references/interface-contracts.md']
    for name in ('calculator','gear-generator','surface-texture','tire-generator','wheel-rim-generator','screw-generator','nut-generator'):
        manifest=json.loads((root/'Plug-ins'/name/'plugin.json').read_text())
        if manifest['id'] != name or manifest['entry'] != 'builtin:'+name:
            raise ValueError('Unsupported plugin manifest: '+name)
        files += ['Plug-ins/'+name+'/plugin.json','Plug-ins/'+name+'/README.md']
    for name in files:
        target=stage/name
        target.parent.mkdir(parents=True,exist_ok=True)
        shutil.copyfile(root/name,target)
    for name in ('s33d-app','s33d-agent'):
        shutil.copyfile(args.release_directory/name,stage/name)
        (stage/name).chmod(0o755)
    shutil.copyfile(root/'scripts/linux-user-install.py',stage/'install.py')
    (stage/'install.sh').write_text('#!/bin/sh\nset -eu\ncd -- "$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"\nexec python3 ./install.py "$@"\n')
    (stage/'install.sh').chmod(0o755)
    (stage/'source').mkdir()
    shutil.copyfile(args.source_archive,stage/'source'/args.source_archive.name)
    (stage/'source/README.md').write_bytes((root/'packaging/SOURCE-README.md').read_bytes())
    (stage/'INSTALL.txt').write_text('S33D Alpha Preview '+version+' - Linux x86_64 local test\n\n'
        'Built on Ubuntu 24.04. Requires compatible glibc/libstdc++ and desktop graphics libraries.\n'
        'Read Docs/LinuxPackaging.md for runtime requirements and tested platform limits.\n'
        'Extract, then run ./install.sh as your ordinary user (Python 3.11+). Do not use sudo.\n'
        'The dated installation is independent; S33D user settings remain shared.\n'
        'No automatic launch, MCP registration, system package installation, or file associations.\n'
        'Sources and required notices are included. This test package is unsigned.\n')
    inventory=[{'path':p.relative_to(stage).as_posix(),'bytes':p.stat().st_size,'sha256':digest(p)}
               for p in sorted(stage.rglob('*')) if p.is_file()]
    manifest={'product':'S33D Alpha Preview','version':version,'local_test_only':True,
              'target':'x86_64-unknown-linux-gnu','build_date':args.date,
              'install_id':'s33d-alpha-preview-test-'+args.date.replace('-',''),
              'source_archive_sha256':digest(args.source_archive),'dependencies':dependencies,'files':inventory}
    (stage/'payload-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
    package=args.output_directory/('S33D-'+version+'-linux-x64-local-test-'+args.date+'.tar.gz')
    with tarfile.open(package,'w:gz') as tar:
        for p in [stage]+sorted(stage.rglob('*')):
            if p.is_symlink():
                raise ValueError('No symlinks permitted in installer payload')
            info=tar.gettarinfo(str(p),arcname=p.relative_to(stage.parent).as_posix())
            info.uid=info.gid=0
            info.uname=info.gname=''
            if p.is_file():
                with p.open('rb') as stream:
                    tar.addfile(info,stream)
            else:
                tar.addfile(info)
    with tarfile.open(package) as tar:
        for member in tar.getmembers():
            p=Path(member.name)
            if p.is_absolute() or '..' in p.parts or member.issym() or member.islnk():
                raise ValueError('Unsafe archive entry')
            if member.isfile():
                data=tar.extractfile(member).read()
                if hashlib.sha256(data).hexdigest() != digest(stage.parent/member.name):
                    raise ValueError('Archive checksum mismatch')
    (package.parent/(package.name+'.sha256')).write_text(digest(package)+'  '+package.name+'\n')
    print(json.dumps({'installer':str(package),'sha256':digest(package),'stage':str(stage),'payload_files':len(inventory)+1}))


if __name__ == '__main__':
    main()
