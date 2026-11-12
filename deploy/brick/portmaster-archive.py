#!/usr/bin/env python3
"""Bounded archives with regular-file link materialization for FAT/exFAT cards."""
import ctypes
from contextlib import contextmanager
import os
from pathlib import Path
import stat
import sys
import tarfile
import tempfile
import time
import unicodedata
import zipfile

# Keep in step with src/update/brick_io.rs. Sleep before each write: validation,
# decompression and switching files must never earn credit for a later burst.
WRITE_RATE = 2 * 1024**2
WRITE_CHUNK = 256 * 1024


@contextmanager
def idle_io():
    previous = None
    if sys.platform.startswith('linux'):
        # Linux has no glibc ioprio wrapper. TrimUI is aarch64; x86_64 is the host test path.
        numbers = {'aarch64': (30, 31), 'x86_64': (251, 252), 'armv7l': (314, 315)}
        set_nr, get_nr = numbers[os.uname().machine]
        libc = ctypes.CDLL(None, use_errno=True)
        previous = libc.syscall(get_nr, 1, 0)
        if previous < 0 or libc.syscall(set_nr, 1, 0, 3 << 13) < 0:
            number = ctypes.get_errno()
            raise OSError(number, os.strerror(number))
    try:
        yield
    finally:
        if previous is not None:
            libc.syscall(set_nr, 1, 0, previous)


class Pacer:
    def __init__(self, total):
        self.done, self.total = 0, total
        self.progress()

    def progress(self):
        value = os.environ.get('BRICK_PORTMASTER_PROGRESS')
        if value:
            path = Path(value)
            temporary = path.with_suffix('.new')
            temporary.write_text(f'unpacking|{self.done}|{self.total}\n')
            temporary.replace(path)

    def write(self, out, block):
        for offset in range(0, len(block), WRITE_CHUNK):
            chunk = block[offset:offset + WRITE_CHUNK]
            time.sleep(len(chunk) / WRITE_RATE)
            out.write(chunk)
            out.flush()
            os.fsync(out.fileno())
            self.done += len(chunk)
            self.progress()


class Limits:
    def __init__(self, max_file=32 * 1024**3, max_total=64 * 1024**3,
                 max_entries=100000, ratio=1000, slack=65536, max_names=16 * 1024**2):
        self.max_file = max_file
        self.max_total = max_total
        self.max_entries = max_entries
        self.ratio = ratio
        self.slack = slack
        self.max_names = max_names


def portable_name(name):
    name = name[:-1] if name.endswith('/') else name
    if not name or name.startswith('/') or '\\' in name:
        raise ValueError('Archive contains an unsafe path.')
    parts = name.split('/')
    for part in parts:
        if (not part or part in ('.', '..') or part.endswith(('.', ' '))
                or any(ord(c) < 32 or c in ':*?"<>|' for c in part)
                or len(part.encode('utf-16-le')) > 510):
            raise ValueError('Archive contains a name unsupported by FAT/exFAT.')
    if len(parts) > 64 or len(name.encode('utf-8')) > 4096:
        raise ValueError('Archive path is too long.')
    return name


def name_key(name):
    return unicodedata.normalize('NFC', name).casefold()


def link_target(name, target, hard=False):
    if not target or target.startswith('/') or '\\' in target:
        raise ValueError('Archive link leaves the archive.')
    parts = [] if hard else name.split('/')[:-1]
    for part in target.rstrip('/').split('/'):
        if part == '..':
            if not parts:
                raise ValueError('Archive link leaves the archive.')
            parts.pop()
        elif part not in ('', '.'):
            portable_name(part)
            parts.append(part)
    if not parts:
        # A reference to archive root would copy the link into itself recursively.
        raise ValueError('Archive link refers to the archive root.')
    return '/'.join(parts)


class Entry:
    def __init__(self, name, kind, source=None, size=0, mode=0o644, target=None):
        self.name, self.kind, self.source = name, kind, source
        self.size, self.mode, self.target = size, mode, target


class Tree:
    def __init__(self, entries, limits):
        self.nodes, self.children, self.keys = {}, {}, {}
        self.limits = limits
        self.explicit = set()
        self.checked_filesystems = set()
        self.names_bytes = 0
        total = 0
        for entry in entries:
            name = portable_name(entry.name)
            parts = name.split('/')
            for count in range(1, len(parts)):
                parent = '/'.join(parts[:count])
                self._register(Entry(parent, 'dir'), False)
            self._register(entry, True)
            if entry.kind == 'file':
                if not 0 <= entry.size <= limits.max_file:
                    raise ValueError('Archive file exceeds the extraction size budget.')
                total += entry.size
            if total > limits.max_total or len(self.nodes) > limits.max_entries:
                raise ValueError('Archive exceeds the extraction budget.')
        for name in self.nodes:
            parent = name.rsplit('/', 1)[0] if '/' in name else ''
            self.children.setdefault(parent, []).append(name)
        # Validate the complete graph before a caller extracts even one member.
        self.full_plan = self.plan()

    def _register(self, entry, explicit):
        name = entry.name
        key = name_key(name)
        if key in self.keys and self.keys[key] != name:
            raise ValueError('Archive names collide on FAT/exFAT.')
        self.keys[key] = name
        previous = self.nodes.get(name)
        if previous is not None:
            if previous.kind != 'dir' or entry.kind != 'dir' or (explicit and name in self.explicit):
                raise ValueError('Archive contains duplicate or conflicting paths.')
        else:
            self.names_bytes += len(name.encode('utf-8'))
            if self.names_bytes > self.limits.max_names:
                raise ValueError('Archive names exceed the metadata budget.')
            self.nodes[name] = entry
        if explicit:
            self.explicit.add(name)

    def _resolve(self, name, seen):
        if len(seen) > 128:
            raise ValueError('Archive link chain is too deep.')
        # Links may name a path below another directory link.
        parts = name.split('/')
        for count in range(1, len(parts) + 1):
            prefix = '/'.join(parts[:count])
            node = self.nodes.get(prefix)
            if node is not None and node.kind == 'link':
                if prefix in seen:
                    raise ValueError('Archive contains a link cycle.')
                suffix = '/'.join(parts[count:])
                next_name = node.target + ('/' + suffix if suffix else '')
                return self._resolve(next_name, seen + (prefix,))
        if name not in self.nodes:
            raise ValueError('Archive link target is missing.')
        return self.nodes[name], seen

    def plan(self, members=None):
        output, keys, total, names_bytes = {}, {}, 0, 0

        def visit(source, dest, seen):
            nonlocal total, names_bytes
            node, aliases = self._resolve(source, seen)
            if node.name in aliases:
                raise ValueError('Archive contains a directory link cycle.')
            dest = portable_name(dest)
            key = name_key(dest)
            if key in keys and keys[key] != dest:
                raise ValueError('Materialized archive names collide on FAT/exFAT.')
            keys[key] = dest
            if dest in output:
                raise ValueError('Materialized archive contains conflicting paths.')
            output[dest] = node
            names_bytes += len(dest.encode('utf-8'))
            if names_bytes > self.limits.max_names:
                raise ValueError('Materialized names exceed the metadata budget.')
            if node.kind == 'file':
                total += node.size
            else:
                for child in sorted(self.children.get(node.name, [])):
                    visit(child, dest + '/' + child.rsplit('/', 1)[-1], aliases + (node.name,))
            if total > self.limits.max_total or len(output) > self.limits.max_entries:
                raise ValueError('Materialized links exceed the extraction budget.')

        for name in sorted(self.children.get('', [])) if members is None else members:
            visit(name, name, ())
        return output


def destination_paths(root, plan):
    root = Path(root).absolute()
    if root.is_symlink():
        raise ValueError('Extraction destination is a symbolic link.')
    paths = {}
    # Preflight existing trees too: never follow a link or alias into player data.
    for name, node in plan.items():
        current = root
        for component in name.split('/'):
            if current.exists():
                if not current.is_dir() or current.is_symlink():
                    raise ValueError('Extraction path crosses an existing file or link.')
                for existing in current.iterdir():
                    if name_key(existing.name) == name_key(component) and existing.name != component:
                        raise ValueError('Extraction name collides with an existing FAT/exFAT name.')
            current = current / component
            if current.is_symlink():
                raise ValueError('Extraction path crosses an existing symbolic link.')
        if current.exists() and ((node.kind == 'dir') != current.is_dir()):
            raise ValueError('Extraction path conflicts with an existing file or directory.')
        paths[name] = current
    return paths


def existing_directory(root):
    root = Path(root).absolute()
    while not root.is_dir():
        if root == root.parent:
            raise ValueError('Cannot inspect extraction filesystem.')
        root = root.parent
    return root


def filesystem_file_limit(root):
    if not sys.platform.startswith('linux'):
        return None
    # f_type is the first native long in Linux statfs. A generously sized opaque
    # buffer avoids depending on the rest of its architecture-specific layout.
    data = ctypes.create_string_buffer(256)
    libc = ctypes.CDLL(None, use_errno=True)
    libc.statfs.argtypes = [ctypes.c_char_p, ctypes.c_void_p]
    libc.statfs.restype = ctypes.c_int
    if libc.statfs(os.fsencode(existing_directory(root)), ctypes.byref(data)) != 0:
        number = ctypes.get_errno()
        raise OSError(number, os.strerror(number))
    magic = ctypes.c_long.from_buffer(data).value
    return 2**32 - 1 if magic == 0x4d44 else None  # MSDOS_SUPER_MAGIC: FAT, not exFAT.


def available_bytes(root):
    space = os.statvfs(existing_directory(root))
    return space.f_bavail * space.f_frsize


def destination_budget(plan, root):
    file_limit = filesystem_file_limit(root)
    if file_limit is not None and any(node.kind == 'file' and node.size > file_limit for node in plan.values()):
        raise ValueError('Archive file exceeds the FAT32 per-file limit of 4 GiB minus one byte.')
    size = sum(node.size for node in plan.values() if node.kind == 'file')
    if size > available_bytes(root):
        raise ValueError('There is not enough free card space to unpack this archive.')


def materialize(tree, dest, opener, members=None):
    with idle_io():
        _materialize(tree, dest, opener, members)


def _materialize(tree, dest, opener, members=None):
    plan = tree.plan(members)
    paths = destination_paths(dest, plan)
    # Even per-member HarbourMaster extraction preflights the complete payload.
    # A FAT volume can hold more than 4 GiB in total; only individual files have
    # that ceiling. exFAT/ext4 retain the general streaming budgets instead.
    device = existing_directory(dest).stat().st_dev
    destination_budget(tree.full_plan if device not in tree.checked_filesystems else plan, dest)
    tree.checked_filesystems.add(device)
    pacer = Pacer(sum(node.size for node in plan.values() if node.kind == 'file'))
    for name, node in plan.items():
        target = paths[name]
        if node.kind == 'dir':
            target.mkdir(parents=True, exist_ok=True)
            continue
        target.parent.mkdir(parents=True, exist_ok=True)
        temporary = None
        try:
            with tempfile.NamedTemporaryFile(prefix='.pm-', dir=target.parent, delete=False) as out:
                temporary = Path(out.name)
                copied = 0
                with opener(node.source) as source:
                    for block in iter(lambda: source.read(WRITE_CHUNK), b''):
                        copied += len(block)
                        if copied > node.size:
                            raise ValueError('Archive file exceeds its declared size.')
                        pacer.write(out, block)
                if copied != node.size:
                    raise ValueError('Archive file is truncated.')
            try:
                temporary.chmod(0o755 if node.mode & 0o111 else 0o644)
            except OSError:
                # FAT/exFAT mounts define permissions globally; data must still install.
                pass
            temporary.replace(target)
        finally:
            if temporary is not None:
                temporary.unlink(missing_ok=True)


def zip_tree(zf, limits):
    entries = []
    names_bytes = 0
    for info in zf.infolist():
        name = portable_name(info.orig_filename)
        names_bytes += len(name.encode('utf-8'))
        if names_bytes > limits.max_names:
            raise ValueError('Archive names exceed the metadata budget.')
        mode = info.external_attr >> 16
        kind = stat.S_IFMT(mode)
        if info.flag_bits & 1 or kind not in (0, stat.S_IFREG, stat.S_IFDIR, stat.S_IFLNK):
            raise ValueError('Archive contains encrypted or special files.')
        if info.file_size > limits.max_file or info.file_size > info.compress_size * limits.ratio + limits.slack:
            raise ValueError('Archive file exceeds the size or compression-ratio budget.')
        if info.is_dir() or kind == stat.S_IFDIR:
            entries.append(Entry(name, 'dir'))
        elif kind == stat.S_IFLNK:
            if info.file_size > 4096:
                raise ValueError('Archive link target is too long.')
            target = zf.read(info).decode('utf-8', errors='strict')
            entries.append(Entry(name, 'link', target=link_target(name, target)))
        else:
            entries.append(Entry(name, 'file', info, info.file_size, mode))
        if len(entries) > limits.max_entries:
            raise ValueError('Archive has too many entries.')
    return Tree(entries, limits)


class PortableZipFile(zipfile.ZipFile):
    def __init__(self, *args, limits=None, **kwargs):
        source = args[0] if args else kwargs.get('file')
        mode = args[1] if len(args) > 1 else kwargs.get('mode', 'r')
        if mode == 'r':
            owned = isinstance(source, (str, bytes, os.PathLike))
            stream = open(source, 'rb') if owned else source
            position = stream.tell()
            try:
                end = zipfile._EndRecData(stream)
                if end is not None and end[zipfile._ECD_SIZE] > 32 * 1024**2:
                    raise zipfile.BadZipFile('Archive central directory exceeds the metadata budget.')
            finally:
                if owned:
                    stream.close()
                else:
                    stream.seek(position)
        super().__init__(*args, **kwargs)
        self.portable_limits = limits or Limits()
        try:
            self.portable_tree = zip_tree(self, self.portable_limits) if self.mode == 'r' else None
        except (ValueError, UnicodeError) as error:
            self.close()
            raise zipfile.BadZipFile(str(error)) from error

    def extract(self, member, path=None, pwd=None):
        name = portable_name(member.filename if isinstance(member, zipfile.ZipInfo) else member)
        self.extractall(path, [name], pwd)
        return str(Path(path or os.curdir) / name)

    def extractall(self, path=None, members=None, pwd=None):
        names = None if members is None else [portable_name(m.filename if isinstance(m, zipfile.ZipInfo) else m) for m in members]
        try:
            materialize(self.portable_tree, path or os.curdir,
                        lambda info: self.open(info, pwd=pwd), names)
        except ValueError as error:
            raise zipfile.BadZipFile(str(error)) from error


def extract_zip(source, dest, limits=None):
    with PortableZipFile(source, limits=limits) as zf:
        zf.extractall(dest)


def extract_tar(source, dest, limits=None, portmaster=False):
    limits = limits or Limits(max_total=700 * 1024 * 1024 if portmaster else 64 * 1024**3)
    with tarfile.open(source, 'r:*') as tar:
        entries, mapping, raw_links = [], {}, []
        unpacked, count, names_bytes = 0, 0, 0
        for member in tar:
            count += 1
            original = portable_name(member.name)
            names_bytes += len(original.encode('utf-8'))
            if names_bytes > limits.max_names:
                raise ValueError('Archive names exceed the metadata budget.')
            if not (member.isfile() or member.isdir() or member.issym() or member.islnk()):
                raise ValueError('Archive contains special files.')
            if member.size > limits.max_file:
                raise ValueError('Archive file exceeds the extraction size budget.')
            unpacked += member.size
            if unpacked > limits.max_total or count > limits.max_entries:
                raise ValueError('Archive exceeds the extraction budget.')
            name = original
            if portmaster:
                parts = original.split('/')
                if len(parts) >= 2 and parts[1] == 'PortMaster':
                    name = '/'.join(parts[1:])
                elif len(parts) == 2 and parts[1] == 'LICENSE' and member.isfile():
                    name = 'PortMaster/LICENSE-PortMaster.txt'
                else:
                    continue
            mapping[original] = name
            if member.issym() or member.islnk():
                entry = Entry(name, 'link')
                raw_links.append((entry, link_target(original, member.linkname, member.islnk())))
            else:
                entry = Entry(name, 'dir' if member.isdir() else 'file', member, member.size, member.mode)
            entries.append(entry)
        if unpacked > Path(source).stat().st_size * limits.ratio + limits.slack:
            raise ValueError('Archive exceeds the compression-ratio budget.')
        for entry, target in raw_links:
            if target in mapping:
                entry.target = mapping[target]
            elif not portmaster:
                entry.target = target
            else:
                parts = target.split('/')
                if len(parts) < 2 or parts[1] != 'PortMaster':
                    raise ValueError('Archive link target is outside the selected tree.')
                # An implicit directory or a path below a directory alias may
                # not have its own header. Translate its namespace first; the
                # bounded graph resolver verifies the actual target afterwards.
                entry.target = '/'.join(parts[1:])
        tree = Tree(entries, limits)
        materialize(tree, dest, tar.extractfile)


def extract_portmaster_tar(source, dest, limits=None):
    extract_tar(source, dest, limits, portmaster=True)


def copy_path(source, dest):
    """Preserved engine libraries can be larger than the bootstrap itself."""
    source, dest = Path(source), Path(dest)
    if source.is_symlink():
        raise ValueError('Preserved runtime data is a symbolic link.')
    paths = sorted(source.rglob('*')) if source.is_dir() else [source]
    entries = []
    for path in paths:
        metadata = path.lstat()
        if stat.S_ISDIR(metadata.st_mode):
            kind = 'dir'
        elif stat.S_ISREG(metadata.st_mode):
            kind = 'file'
        else:
            raise ValueError('Preserved runtime data contains a link or special file.')
        name = path.relative_to(source).as_posix() if source.is_dir() else dest.name
        entries.append(Entry(name, kind, path, metadata.st_size if kind == 'file' else 0,
                             metadata.st_mode))
    tree = Tree(entries, Limits())
    materialize(tree, dest if source.is_dir() else dest.parent, lambda path: path.open('rb'))


if __name__ == '__main__':
    try:
        if len(sys.argv) != 4 or sys.argv[1] not in ('zip', 'tar'):
            raise ValueError('usage: portmaster-archive.py zip|tar ARCHIVE DEST')
        (extract_zip if sys.argv[1] == 'zip' else extract_tar)(*sys.argv[2:])
    except (OSError, ValueError, zipfile.BadZipFile, tarfile.TarError) as error:
        print('PortMaster: ' + str(error), file=sys.stderr)
        sys.exit(1)
