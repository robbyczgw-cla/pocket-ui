"""Which files make up one disc game, and a cue sheet for their copies.

device-check copies a game below the test folder under the name `pocketcheck`. A .cue game is a sheet plus one or more
track files named inside it; copying only the sheet (the old way) starts nothing, and renaming the track files without
rewriting the sheet breaks it. Usage (the cue text on stdin):

    device_game_files.py CUE_DIR STEM OUT_CUE      prints "<source path>\\t<copy name>" per track file, writes OUT_CUE

One track file keeps the plain name `STEM.<ext>`; several become `STEM-01.<ext>`, `STEM-02.<ext>` ... The rewritten sheet
names the copies. A reference that leaves the sheet's folder (absolute path, `..`) is refused: a test must never copy
from, or write a sheet that points at, somewhere else.
"""
import os
import re
import sys

FILE_LINE = re.compile(r'^(\s*FILE\s+)(?:"([^"]*)"|(\S+))(\s+\S+\s*)$', re.IGNORECASE)


def plan(cue_text, cue_dir, stem):
    """Return ([(source path, copy name)], new sheet text)."""
    names = []
    for line in cue_text.splitlines():
        m = FILE_LINE.match(line.rstrip('\r'))
        if m:
            name = m.group(2) if m.group(2) is not None else m.group(3)
            if name.startswith('/') or '\\' in name or '..' in name.split('/'):
                raise ValueError('the sheet points outside its folder: ' + name)
            if name not in names:
                names.append(name)
    if not names:
        raise ValueError('the sheet names no track file')
    copies = {}
    for i, name in enumerate(names, 1):
        ext = os.path.splitext(name)[1]
        copies[name] = stem + ext if len(names) == 1 else '%s-%02d%s' % (stem, i, ext)
    out = []
    for line in cue_text.splitlines():
        m = FILE_LINE.match(line.rstrip('\r'))
        if m:
            name = m.group(2) if m.group(2) is not None else m.group(3)
            line = '%s"%s"%s' % (m.group(1), copies[name], m.group(4))
        out.append(line)
    files = [(cue_dir.rstrip('/') + '/' + n, copies[n]) for n in names]
    return files, '\r\n'.join(out) + '\r\n'


def main(argv):
    if len(argv) != 4:
        print(__doc__, file=sys.stderr)
        return 2
    cue_dir, stem, out_cue = argv[1:]
    try:
        files, sheet = plan(sys.stdin.buffer.read().decode('utf-8', 'replace'), cue_dir, stem)
    except ValueError as error:
        print('device_game_files: %s' % error, file=sys.stderr)
        return 1
    with open(out_cue, 'w', newline='') as f:
        f.write(sheet)
    for src, dst in files:
        print('%s\t%s' % (src, dst))
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
