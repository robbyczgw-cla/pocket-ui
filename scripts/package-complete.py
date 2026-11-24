#!/usr/bin/env python3
"""Package completeness: every core in a package has the system files it needs to start a game, in that package.

  python3 -I scripts/package-complete.py brick   CARD_DIR | card zip | card-update zip | pocket-ui-brick-*.tar.gz
  python3 -I scripts/package-complete.py android pocket-ui-android.apk
  python3 -I scripts/package-complete.py darkos  pocket-ui-darkos-*.zip
  python3 -I scripts/package-complete.py tree    ROOT --cores REL --system REL   (an unpacked image or a device's files)

Runs without a device and without a build: it only reads a finished card folder, zip or APK. Exit 0 = complete,
1 = something is missing (each gap on its own line), 2 = wrong call.

Found 20.11.2026: the 0.6 Brick card had ppsspp_libretro.so but none of PPSSPP's system files, so every PSP game ended
after 1 s with "file may be damaged". Android carried them (assets/system/PPSSPP), the Brick did not; no check compared
the packages. Checks here, per package:
- every core (*_libretro.so) is in NEEDS: a new core must say what it needs, even "nothing" (the class, not just PSP);
- each needed file is in the place that core's system folder is filled from, and not empty:
    brick    app/bios-data/<file>  (app/start.sh seeds Roms/bios from it; a Wi-Fi update replaces only app/)
    android  assets/system/<file>  (the player copies it to files/bios)
    darkos   bin/cores ships our own cores only; dArkOS's own cores read dArkOS's bios folder, so their needs are
             printed as a device check (not a failure here)
    tree     <ROOT>/<system>/<file>
- brick: every core es_systems.cfg names is on the card; android: every core assets/cores.json lists is in lib/.
User-owned BIOS files (PS1, Neo Geo, ...) are not ours to ship and stay out: the app asks the player for them.
"""
import io
import json
import re
import sys
import tarfile
import zipfile
from pathlib import Path

PSP = ['PPSSPP/ppge_atlas.zim', 'PPSSPP/compat.ini', 'PPSSPP/flash0/font/ltn0.pgf', 'PPSSPP/lang/en_US.ini',
       'PPSSPP/vfpu/vfpu_asin_lut65536.dat']  # scripts/brick-ppsspp-data.py REQUIRED, in the system folder
NEEDS = {
    'ppsspp': PSP,
    'scummvm': ['scummvm/extra/sky.cpt', 'scummvm/theme/scummmodern.zip'],
}
# Cores that start a game with nothing of ours in the system folder: no system data, or a built-in/HLE BIOS
# (DSperate and melonDS DS FreeBIOS, Flycast and Yabause/YabaSanshiro HLE, PCSX ReARMed HLE; a real BIOS is optional
# and the player's own). Listed one by one so that a new core lands here only by someone's decision.
for _core in ('gambatte mgba gpsp nestopia fceumm snes9x bsnes pcsx_rearmed picodrive genesis_plus_gx clownmdemu gearsystem '
              'beetle_ngp mednafen_ngp race beetle_wswan mednafen_wswan beetle_pce_fast mednafen_pce_fast beetle_supergrafx '
              'mednafen_supergrafx beetle_vb mednafen_vb handy stella stella2014 prosystem fbneo mupen64plus_next flycast pokemini '
              'fake08 tic80 melondsds dsperate dosbox_pure yabause yabasanshiro').split():
    NEEDS.setdefault(_core, [])


class Package:
    """The files of a folder, zip or APK as relative POSIX paths with their sizes."""

    def __init__(self, path):
        self.path = Path(path)
        if self.path.is_dir():
            self.files = {p.relative_to(self.path).as_posix(): p.stat().st_size
                          for p in self.path.rglob('*') if p.is_file()}
            self._read = lambda name: (self.path / name).read_bytes()
        elif self.path.name.endswith('.tar.gz'):
            self._from_update(self.path.read_bytes())
        else:
            z = zipfile.ZipFile(self.path)
            self.files = {i.filename: i.file_size for i in z.infolist() if not i.is_dir()}
            self._read = z.read
            # A card-update zip: the Wi-Fi update inside (pocket-ui-<platform>-<version>.tar.gz) is the package.
            inner = [n for n in self.files if re.fullmatch(r'pocket-ui-[a-z]+-[^/]+\.tar\.gz', n)]
            if len(inner) == 1 and not any(n.endswith('_libretro.so') for n in self.files):
                self._from_update(z.read(inner[0]))

    def _from_update(self, data):
        """A Wi-Fi update (scripts/brick-update-pack.py): the app tarball, with cores and bios-data as separate
        packages listed in its asset-packages.json. Both count as the app/ folder they unpack into."""
        with tarfile.open(fileobj=io.BytesIO(data)) as t:
            members = {m.name.removeprefix('./'): m for m in t.getmembers() if m.isfile()}
            self.files = {'app/' + n: m.size for n, m in members.items()}
            texts = {'app/' + n: t.extractfile(m).read() for n, m in members.items()
                     if n in ('asset-packages.json', 'es_systems.cfg')}
        for asset in json.loads(texts.get('app/asset-packages.json', b'{}')).get('assets', []):
            self.files['app/' + asset['path']] = asset['size']
        self._read = texts.__getitem__

    def read(self, name):
        return self._read(name)

    def cores(self, pattern):
        """{core id: (prefix before the match, file name)} for every file matching `pattern` (groups: prefix, id)."""
        found = {}
        for name in sorted(self.files):
            m = re.fullmatch(pattern, name)
            if m:
                found.setdefault(m.group(2), (m.group(1), name))
        return found


def need_files(cores, have, where, problems):
    """Appends a line to `problems` for each core not in NEEDS and each needed file `have(rel)` does not find."""
    for core in sorted(cores):
        if core not in NEEDS:
            problems.append(f'{core}: new core without an entry in NEEDS (scripts/package-complete.py): '
                            'decide which system files it needs, also when none')
            continue
        missing = [rel for rel in NEEDS[core] if not have(rel)]
        if missing:
            problems.append(f'{core}: {len(missing)} of {len(NEEDS[core])} system files missing in {where}: ' + ', '.join(missing))


def check_brick(pkg):
    problems = []
    cores = pkg.cores(r'(.*?app/)cores/([^/]+)_libretro\.so')
    if not cores:
        return ['no app/cores/*_libretro.so: not a Brick card or update'], []
    prefixes = {p for p, _ in cores.values()}
    if len(prefixes) != 1:
        return ['cores in more than one app/ folder: ' + ', '.join(sorted(prefixes))], []
    app = prefixes.pop()
    data = app + 'bios-data/'
    need_files(cores, lambda rel: pkg.files.get(data + rel, 0) > 0, data, problems)
    cfg = app + 'es_systems.cfg'
    if cfg in pkg.files:
        named = set(re.findall(r'<core[^>]*>\s*([A-Za-z0-9_]+)\s*</core>', pkg.read(cfg).decode('utf-8', 'replace')))
        for core in sorted(named - set(cores)):
            problems.append(f'{core}: es_systems.cfg offers it, but {app}cores/ has no {core}_libretro.so')
    return problems, sorted(cores)


def check_android(pkg):
    problems = []
    cores = pkg.cores(r'(lib/[^/]+/)lib([^/]+)_libretro\.so')
    if not cores:
        return ['no lib/*/lib*_libretro.so: not a pocket-ui APK'], []
    need_files(cores, lambda rel: pkg.files.get('assets/system/' + rel, 0) > 0, 'assets/system/', problems)
    if 'assets/cores.json' in pkg.files:
        listed = {c['id'] for c in json.loads(pkg.read('assets/cores.json')).get('cores', [])}
        for core in sorted(listed - set(cores)):
            problems.append(f'{core}: assets/cores.json lists it, but the APK has no lib{core}_libretro.so')
    return problems, sorted(cores)


# dArkOS's own cores that pocket-core starts (deploy/darkos/systems.py CORES): they read <dArkOS roms>/bios.
DARKOS_HOST = ('ppsspp', 'flycast', 'mupen64plus_next')


def check_darkos(pkg):
    problems = []
    cores = pkg.cores(r'(.*?bin/)cores/([^/]+)_libretro\.so')
    if not cores:
        return ['no bin/cores/*_libretro.so: not a dArkOS port'], []
    need_files(cores, lambda rel: False, 'the port (it has no bios folder)', problems)
    for core in DARKOS_HOST:
        if NEEDS[core]:
            print(f"device check (dArkOS's own {core}): roms/bios/" + ', roms/bios/'.join(NEEDS[core]))
    return problems, sorted(cores)


def check_tree(pkg, cores_rel, system_rel):
    problems = []
    pattern = re.escape(cores_rel.strip('/') + '/').replace('\\*', '[^/]+')
    cores = pkg.cores(r'(' + pattern + r')(?:lib)?([^/]+?)_libretro\.so')
    if not cores:
        return [f'no {cores_rel}/*_libretro.so in {pkg.path}'], []
    system = system_rel.strip('/') + '/'
    need_files(cores, lambda rel: pkg.files.get(system + rel, 0) > 0, system, problems)
    return problems, sorted(cores)


def main(argv):
    args = argv[1:]
    if len(args) < 2 or args[0] not in ('brick', 'android', 'darkos', 'tree'):
        print(__doc__.strip(), file=sys.stderr)
        return 2
    kind, path = args[0], args[1]
    if not Path(path).exists():
        print(f'package-complete: {path} does not exist', file=sys.stderr)
        return 2
    pkg = Package(path)
    if kind == 'tree':
        opts = dict(zip(args[2::2], args[3::2]))
        if '--cores' not in opts or '--system' not in opts:
            print('package-complete: tree needs --cores REL and --system REL', file=sys.stderr)
            return 2
        problems, cores = check_tree(pkg, opts['--cores'], opts['--system'])
    else:
        problems, cores = {'brick': check_brick, 'android': check_android, 'darkos': check_darkos}[kind](pkg)
    for line in problems:
        print('MISSING ' + line)
    print(f'{kind} {Path(path).name}: {len(cores)} cores, ' + ('complete' if not problems else f'{len(problems)} gaps'))
    return 1 if problems else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
