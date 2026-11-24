"""Prove the shipped source kit can rebuild S33D against modified OCCT.

The marker change occurs only in a fresh audit extraction, never the release kit.
"""
from pathlib import Path,PurePosixPath
import hashlib,json,os,subprocess,sys,tarfile,zipfile,shutil,time

root=Path(os.environ['AUDIT_ROOT']).resolve();evidence=root/'evidence';evidence.mkdir()
kit=root/'kit';kit.mkdir();target=root/'build';assert not target.exists()
marker='S33D-OCCT-RELINK-AUDIT-20261123-2'
report={'passed':False,'source_commit':'1ed6e375970fb0e6af62e90d16192d837350e986','marker':marker,'fresh_native_target':True}
def sha(path):
    with path.open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
def safe(name):
    p=PurePosixPath(name);assert not p.is_absolute() and '..' not in p.parts and ':' not in name and '\\' not in name
    return p
def run(name,args,env):
    print('Starting '+name,flush=True)
    with (evidence/(name+'.log')).open('wb') as log:
        result=subprocess.run(args,stdout=log,stderr=subprocess.STDOUT,env=env)
    assert result.returncode==0,name+' failed; inspect the evidence log'
    print('Completed '+name,flush=True)
try:
    archive=root/'corresponding-source.zip'
    report['source_zip_sha256']=sha(archive)
    assert report['source_zip_sha256']=='cb4ab4e8bb4ac60adcd12128a406dc7224fcbbe17b624e57b634457440675c1f'
    with zipfile.ZipFile(archive) as z:
        manifest=json.loads(z.read('source-manifest.json').decode('utf-8-sig'))
        expected={item['path']:item['sha256'] for item in manifest['files']}
        assert set(z.namelist())==set(expected)|{'source-manifest.json'}
        for entry in z.infolist():
            parts=safe(entry.filename);assert (entry.external_attr>>16)&0o170000!=0o120000
            data=z.read(entry);assert entry.filename=='source-manifest.json' or hashlib.sha256(data).hexdigest()==expected[entry.filename]
            dest=kit.joinpath(*parts.parts);dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(data)
    crate=kit/'third-party/occt-sys-0.6.0.crate';assert sha(crate)==manifest['occt_crate_sha256']
    expanded=kit/'third-party/expanded';expanded.mkdir()
    with tarfile.open(crate,'r:gz') as tar:
        for member in tar.getmembers():
            parts=safe(member.name);assert member.isdir() or member.isfile()
            dest=expanded.joinpath(*parts.parts)
            if member.isdir():dest.mkdir(parents=True,exist_ok=True)
            else:
                dest.parent.mkdir(parents=True,exist_ok=True)
                with tar.extractfile(member) as stream,dest.open('wb') as out:shutil.copyfileobj(stream,out)
    local=expanded/'occt-sys-0.6.0'
    standard=local/'OCCT/src/Standard/Standard.cxx'
    original=standard.read_bytes();text=original.decode('utf-8')
    needle='Standard_Address Standard::Allocate(const Standard_Size theSize)\n{'
    normalized=text.replace('\r\n','\n');assert normalized.count(needle)==1
    replacement=needle+'\n  // S33D independent relink audit change (2026-11-23); audit copy only.\n  static volatile const char s33dAuditMarker[] = "'+marker+'";\n  (void)s33dAuditMarker[0];\n  (void)s33dAuditMarker[sizeof(s33dAuditMarker) - 2];'
    standard.write_text(normalized.replace(needle,replacement),encoding='utf-8',newline='\n')
    report['modified_member']='OCCT/src/Standard/Standard.cxx';report['original_member_sha256']=hashlib.sha256(original).hexdigest();report['modified_member_sha256']=sha(standard)
    report['supplied_compatibility_patch_sha256']=sha(kit/'S33D/crates/kernel/native/occt_patches/Standard_Type.cxx')
    env=dict(os.environ,CARGO_HOME=str(root/'cargo-home'),CARGO_TARGET_DIR=str(target),CARGO_BUILD_JOBS='2',CMAKE_BUILD_PARALLEL_LEVEL='2',MSBUILDDISABLENODEREUSE='1',_CL_='/MP2',CARGO_PROFILE_RELEASE_DEBUG='0',CARGO_PROFILE_RELEASE_STRIP='debuginfo',S33D_BUILD_ID='20261123-2-occt-audit',S33D_BUILD_TIME='2026-11-23T00:00:00Z',S33D_BUILD_CHANNEL='development')
    rules=[]
    for prefix,label in [(str(root),'/audit'),(str(root/'cargo-home'),'/cargo-cache'),(os.environ.get('USERPROFILE',''),'/build-user')]:
        if prefix:
            for form in {prefix,prefix.replace('\\','/')}:rules.append('--remap-path-prefix='+form+'='+label)
    env['CARGO_ENCODED_RUSTFLAGS']='\x1f'.join(rules)
    cargo=shutil.which('cargo');assert cargo
    base=[cargo,'+1.97.1'];manifest_path=str(kit/'S33D/Cargo.toml')
    run('fetch-locked-dependencies',base+['fetch','--locked','--manifest-path',manifest_path],env)
    patch='patch.crates-io.occt-sys.path="'+str(local).replace('\\','/')+'"'
    run('build-replacement',base+['build','--release','--manifest-path',manifest_path,'--config',patch,'--bin','s33d-app','--bin','s33d-agent','-j','2'],env)
    app=target/'release/s33d-app.exe';agent=target/'release/s33d-agent.exe'
    assert marker.encode() in app.read_bytes(),'The replacement library marker was not linked into S33D'
    roots=set()
    for output in (target/'release/build').glob('s33d-kernel-*/output'):
        for line in output.read_text(encoding='utf-8',errors='replace').splitlines():
            if line.startswith('cargo:rustc-link-search=native='):
                d=Path(line.split('=',2)[2]).resolve()
                if d.name=='lib' and (d.parent/'build/CMakeCache.txt').is_file():roots.add(d.parent)
    assert len(roots)==1
    native=roots.pop();lib=native/'lib/TKernel.lib';assert marker.encode() in lib.read_bytes()
    cache=(native/'build/CMakeCache.txt').read_text(encoding='utf-8-sig')
    import re
    flags=re.search(r'^CMAKE_CXX_FLAGS_RELEASE:[^=]+=(.*)$',cache,re.M).group(1)
    assert '/O2' in flags and '/Ob2' in flags and 'NDEBUG' in flags
    run('kernel-geometry-tests',base+['test','--release','--manifest-path',manifest_path,'--config',patch,'-p','s33d-kernel','-j','2','--','--test-threads=2'],env)
    version=subprocess.check_output([str(agent),'--version'],env=env,text=True).strip();assert version=='s33d-agent 0.1.0-alpha.1'
    report.update(passed=True,replacement_marker_in_library_and_s33d=True,release_optimization_verified=True,kernel_tests_passed=True,agent_version=version,app_sha256=sha(app),agent_sha256=sha(agent),tkernel_sha256=sha(lib),native_library_count=len(list((native/'lib').glob('*.lib'))),supplied_source_entries_verified=len(expected))
finally:
    (evidence/'relink-result.json').write_text(json.dumps(report,indent=2),encoding='utf-8')
print(json.dumps({'passed':True,'replacement_library_linked':True,'kernel_tests_passed':True}),flush=True)
