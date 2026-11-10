"""Names from the shipped snapshot libraries, without rendering art or learning i18n names."""
import ast
import pathlib
import subprocess
import tempfile


def declarations(filename, wanted):
    tree = ast.parse(pathlib.Path(filename).read_text())
    found = {}
    for node in tree.body:
        if isinstance(node, ast.Assign):
            for target in node.targets:
                if isinstance(target, ast.Name) and target.id in wanted:
                    found[target.id] = ast.literal_eval(node.value)
    return found


curator = declarations('scripts/curator-fake-library.py', {'SYSTEMS', 'GAMES'})
for _, name, _ in curator['SYSTEMS']:
    print(name)
for games in curator['GAMES'].values():
    for name in games:
        print(name)
# Use the real deterministic generator, with no cover output. Empty ROM files only;
# temporary files are removed even on failure. This follows future generator changes.
with tempfile.TemporaryDirectory(prefix='pocket-name-corpus-') as scratch:
    subprocess.run(['sh', 'scripts/fake-library.sh', scratch, '5000'], check=True, stdout=subprocess.DEVNULL)
    for path in (pathlib.Path(scratch) / 'roms').glob('*/*'):
        print(path.name)
    import xml.etree.ElementTree as ET
    for node in ET.parse(pathlib.Path(scratch) / 'es_systems.cfg').iter('fullname'):
        print(node.text)
# This fixture's file and gamelist spellings differ. Exercise both, for every entry.
other = declarations('scripts/other-layouts-big-fixture.py', {'sysdefs'})
for _, name, _ in other['sysdefs']:
    print(name)
for i in range(2500):
    print(f'Game number {i:05d}')
    print(f'Game number {i}')
# Performance libraries take a count, not a finite catalogue. Include the complete
# 10k-game benchmark spelling range as well as its retained homebrew entry.
print('ucity')
for i in range(10000):
    print(f'zz Perf Game {i:04d} (World)')
