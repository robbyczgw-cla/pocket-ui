#!/usr/bin/env python3
"""Pinned local NDK builds, immutable source archives and complete licence notices. No hosted CI."""
import gzip,hashlib,io,json,os,platform,re,shutil,subprocess,tarfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
os.chdir(ROOT)
lock=json.loads((ROOT/'android/cores.lock.json').read_text())
sdk=Path(os.environ.get('ANDROID_HOME',str(Path.home()/'Library/Android/sdk')))
ndk=Path(os.environ.get('ANDROID_NDK_HOME',str(sdk/'ndk'/lock['ndk'])))
host='darwin-x86_64'if platform.system()=='Darwin'else'linux-x86_64'
tools=ndk/'toolchains/llvm/prebuilt'/host/'bin'
assert tools.is_dir(),f'Missing NDK toolchain: {tools}'
assert re.search(r'^Pkg.Revision\s*=\s*'+re.escape(lock['ndk'])+r'\s*$',(ndk/'source.properties').read_text(),re.M),'The core lock requires the recorded NDK revision'
out=ROOT/'android/build/cores'
for name in ['source','build','lib','licences','corresponding-source']:(out/name).mkdir(parents=True,exist_ok=True)
def run(args,cwd=None):subprocess.run([str(a)for a in args],cwd=cwd,check=True)
def git(p,*args):return subprocess.check_output(['git','-C',str(p),*args])
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def archive(src,dest,prefix,dependencies=()):
    # Git's tracked list excludes build outputs; recurse into pinned submodules if a future core needs them.
    files=[]
    for base,namespace in [(src,''),*[(p,'third-party/'+p.name+'/')for p in dependencies]]:
        files.extend((base,namespace,raw)for raw in git(base,'ls-files','--recurse-submodules','-z').split(b'\0')if raw)
    with dest.open('wb')as raw,gzip.GzipFile(fileobj=raw,mode='wb',mtime=0,filename='')as gz,tarfile.open(fileobj=gz,mode='w|')as tar:
        for base,namespace,name in sorted(files,key=lambda row:(row[1],row[2])):
            relative=os.fsdecode(name);p=base/relative
            if not p.exists():
                data=git(base,'show','HEAD:'+relative);info=tarfile.TarInfo(prefix+'/'+namespace+relative);info.size=len(data);tar.addfile(info,io.BytesIO(data));continue
            info=tar.gettarinfo(str(p),arcname=prefix+'/'+namespace+relative)
            info.uid=info.gid=info.mtime=0;info.uname=info.gname=''
            if info.isfile():
                with p.open('rb')as f:tar.addfile(info,f)
            else:tar.addfile(info)
# NDK runtimes carry their own notices, including the static C++ runtime in C++ cores.
for name in ['NOTICE','NOTICE.toolchain']:
    shutil.copyfile(ndk/name,out/'licences'/('NDK-'+name+'.txt'))
manifest=[]
for core in lock['cores']:
    cid=core['id']
    src=out/'source'/cid;src.mkdir(exist_ok=True)
    if not(src/'.git').exists():run(['git','init','-q',src])
    rev=core['revision']
    if subprocess.run(['git','-C',str(src),'cat-file','-e',rev+'^{commit}'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode:
        run(['git','-C',src,'fetch','--depth','1',core['source']+'.git',rev])
    run(['git','-C',src,'checkout','--detach','-q',rev])
    if(src/'.gitmodules').exists():run(['git','-C',src,'submodule','update','--init','--recursive','--depth','1'])
    cached_files={}
    for raw in git(src,'diff','--name-only','-z').split(b'\0'):
        if raw:
            p=src/os.fsdecode(raw)
            if p.is_file():cached_files[p]=(p.read_bytes(),p.stat())
    patches=[]
    for item in core.get('patches',[]):
        patch=ROOT/item['path'];assert sha(patch)==item['sha256'],'Changed recorded core patch'
        if subprocess.run(['git','-C',str(src),'apply','--reverse','--check',str(patch)],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode==0:run(['git','-C',src,'apply','--reverse',patch])
        patches.append(patch)
    assert subprocess.run(['git','-C',str(src),'diff','--quiet','HEAD','--']).returncode==0,f'Modified core source: {cid}; refusing to label it as the pinned revision'
    for patch in patches:run(['git','-C',src,'apply',patch])
    for p,(content,stat)in cached_files.items():
        if p.is_file()and p.read_bytes()==content:os.utime(p,ns=(stat.st_atime_ns,stat.st_mtime_ns))
    # No build folder (the builder's home) in the cores' strings; scripts/check-binary-paths.sh checks the packaged .so files.
    prefix=f"-ffile-prefix-map={ROOT}=/pocket-ui -ffile-prefix-map={Path.home()}=/home-build"
    os.environ['SOURCE_DATE_EPOCH']=git(src,'show','-s','--format=%ct',rev).decode().strip()
    patch_suffix='-'+hashlib.sha256(b''.join(p.read_bytes()for p in patches)).hexdigest()[:12]if patches else''
    source=out/'corresponding-source'/f'{cid}-{rev}{patch_suffix}.tar.gz'
    notices=[]
    inline=[];seen_comments=set()
    jobs=os.environ.get('ANDROID_CORE_JOBS','4')
    if core['builder']=='cmake':
        build=out/'build'/cid
        run(['cmake','-S',src,'-B',build,'-G','Ninja',f'-DCMAKE_TOOLCHAIN_FILE={ndk}/build/cmake/android.toolchain.cmake','-DANDROID_ABI=arm64-v8a',f'-DANDROID_PLATFORM=android-{lock["api"]}','-DCMAKE_BUILD_TYPE=Release','-DANDROID_STL=c++_static',f'-DCMAKE_C_FLAGS={prefix}',f'-DCMAKE_CXX_FLAGS={prefix}',*['-D'+v for v in core['options']]])
        run(['cmake','--build',build,'--target',core['target'],'--parallel',jobs]);binary=build/core['binary']
    else:
        build=src/core.get('directory','.')
        cc=tools/f'aarch64-linux-android{lock["api"]}-clang'
        cxx=tools/f'aarch64-linux-android{lock["api"]}-clang++'
        run(['make','-C',build,'-j'+jobs,'platform=unix','CC='+str(cc),'CXX='+str(cxx)+' -static-libstdc++','LD='+str(cxx)+' -static-libstdc++','AR='+str(tools/'llvm-ar'),'STRIP='+str(tools/'llvm-strip'),'GIT_VERSION='+rev[:7],*core['options']]);binary=build/core['binary']
    dependencies=[]
    if core.get('fetch_dependencies'):
        for name,dep in core['fetch_dependencies'].items():
            p=build/'_deps'/(name+'-src');assert git(p,'rev-parse','HEAD').decode().strip()==dep['revision'],f'Unpinned build dependency: {name}'
            dependencies.append(p)
    # Preserve all licence files, including embedded dependencies; the source archive preserves inline notices.
    source_files=[]
    for base in [src,*dependencies]:
        source_files.extend((base,raw)for raw in git(base,'ls-files','--recurse-submodules','-z').split(b'\0')if raw)
    for base,raw in source_files:
        if not raw:continue
        relative=os.fsdecode(raw);p=base/relative
        relative=(base.name+'/'if base!=src else'')+relative
        if (p.name.lower().startswith(('license','licence','copying','copyright'))or any(part.lower()in ['licenses','licences']for part in p.parts))and p.is_file():
            dest=out/'licences'/(cid+'--'+relative.replace('/','--'))
            shutil.copyfile(p,dest);notices.append({'path':relative,'sha256':sha(p),'asset':dest.name})
        if p.suffix.lower()in ['.c','.cpp','.h','.hpp','.cc']and p.is_file():
            text=p.read_text(errors='replace')
            for comment in re.findall(r"/\*.*?\*/|(?m:(?:^[ \t]*//[^\n]*\n)+)",text,re.S):
                lower=comment.lower()
                if any(term in lower for term in ['permission is hereby granted','permission to use, copy','redistribution and use','this software is provided','general public license'])or('copyright'in lower and 'licen'in lower):
                    key=hashlib.sha256(comment.encode()).hexdigest()
                    if key not in seen_comments:seen_comments.add(key);inline.append(relative+'\n'+comment)
    if inline:
        dest=out/'licences'/(cid+'--INLINE-NOTICES.txt');dest.write_text('\n\n'.join(inline)+'\n')
        notices.append({'asset':dest.name,'sha256':sha(dest),'scope':'licence comments from pinned tracked source, including embedded dependencies'})
    evidence=[{'path':p,'sha256':sha(src/p)}for p in core['evidence']]
    source_evidence={}
    if os.environ.get('ANDROID_RELEASE_SOURCES')=='1':
        if not source.exists():archive(src,source,f'{cid}-{rev}',dependencies)
        source_evidence={'corresponding_source':source.name,'source_sha256':sha(source)}
    dest=out/'lib'/f'lib{cid}_libretro.so';shutil.copyfile(binary,dest)
    run([tools/'llvm-strip','--strip-unneeded',dest])
    # Fail if the build silently produced a host binary or depends on an unbundled C++ runtime.
    elf=subprocess.check_output([str(tools/'llvm-readelf'),'-h','-d',str(dest)],text=True)
    assert 'AArch64'in elf and 'libc++_shared.so'not in elf,f'Invalid ARM64 self-contained core: {cid}'
    manifest.append({**core,'sha256':sha(dest),'bytes':dest.stat().st_size,'notices':notices,'evidence':evidence,**source_evidence})
(out/'manifest.json').write_text(json.dumps({**{k:lock[k]for k in ['format','abi','api','ndk']},'cores':manifest},indent=2)+'\n')
print('Built '+', '.join(c['id']for c in manifest))
