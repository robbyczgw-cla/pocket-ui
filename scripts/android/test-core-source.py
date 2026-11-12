#!/usr/bin/env python3
"""Exercise the actual archive writer with real pinned Git submodules, without running NDK builds."""
import ast,gzip,io,os,subprocess,tarfile,tempfile,unittest
from pathlib import Path

def git(p,*args): return subprocess.check_output(['git','-C',str(p),*args],stderr=subprocess.PIPE)
code=ast.parse(Path(__file__).with_name('build-cores.py').read_text())
archive_node=next(n for n in code.body if isinstance(n,ast.FunctionDef) and n.name=='archive')
exec(compile(ast.Module(body=[archive_node],type_ignores=[]),'build-cores.py','exec'))

class SourceArchives(unittest.TestCase):
    def repo(self,path):
        path.mkdir();git(path,'init','-q');return path
    def commit(self,path):
        git(path,'add','.');git(path,'-c','user.name=Test','-c','user.email=test@example.invalid','commit','-qm','Pinned fixture')
    def test_a_configure_removed_submodule_header_is_still_in_corresponding_source(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);dep=self.repo(root/'dependency');(dep/'zconf.h').write_text('original pinned header\n');self.commit(dep)
            src=self.repo(root/'core');git(src,'-c','protocol.file.allow=always','submodule','add','-q',str(dep),'deps/zlib');self.commit(src)
            (src/'deps/zlib/zconf.h').unlink() # Upstream zlib's CMake moves this tracked file while configuring.
            dest=root/'source.tar.gz';archive(src,dest,'core-pinned')
            with tarfile.open(dest) as tar:
                self.assertEqual(tar.extractfile('core-pinned/deps/zlib/zconf.h').read(),b'original pinned header\n')
    def test_a_failed_archive_is_not_mistaken_for_complete_source_on_retry(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);src=self.repo(root/'core');(src/'file').write_text('source');self.commit(src)
            # An unreadable/untracked entry forces failure after the writer opens its output.
            real_git=globals()['git']
            def bad_git(p,*args):
                if args[:1]==('ls-files',): return b'gone\0'
                return real_git(p,*args)
            globals()['git']=bad_git;dest=root/'source.tar.gz'
            try:
                with self.assertRaises(subprocess.CalledProcessError):archive(src,dest,'core-pinned')
                self.assertFalse(dest.exists(),'A partial source archive must never be accepted on the next build')
            finally:globals()['git']=real_git

if __name__=='__main__':unittest.main()
