#!/usr/bin/python3
"""Host-side integration checks for the dArkOS inventory and command boundary."""
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest
import xml.etree.ElementTree as E

HERE = Path(__file__).resolve().parent

def module(name, file):
    spec = importlib.util.spec_from_file_location(name, HERE / file)
    m = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(m)
    return m

systems = module('systems', 'systems.py')
launch = module('launch', 'host-launch.py')


class Systems(unittest.TestCase):
    def test_selected_card_cores_and_native_priorities(self):
        with tempfile.TemporaryDirectory() as t:
            p = Path(t); cores = p / 'cores'; cores.mkdir()
            for core in ('mednafen_ngp', 'ppsspp', 'flycast', 'mame'):
                (cores / (core + '_libretro.so')).write_bytes(b'\x7fELF\x02\x01' + b'\0' * 12 + b'\xb7\0')
            (cores / 'handy_libretro.so').write_bytes(b'\x7fELF\x01\x01' + b'\0' * 14)
            xml = E.Element('systemList')
            for sid, core, folder in [('ngpc', 'mednafen_ngp', 'ngpc'), ('ngp', 'mednafen_ngp', 'ngp'),
                                      ('ppsspp', 'ppsspp', 'psp'), ('dreamcast', 'flycast', 'dreamcast'),
                                      ('atarilynx', 'handy', 'atarilynx'), ('arcade', 'mame', 'arcade'),
                                      ('genesis', 'genesis_plus_gx', 'genesis'), ('Gamecube', '', 'gc'),
                                      ('famicom disk system', '', 'fds'), ('snes', '', 'sufami'), ('snes', '', 'snes')]:
                (p / 'card2' / folder).mkdir(parents=True)
                n = E.SubElement(xml, 'system')
                for k,v in [('name',sid), ('path','/roms/' + folder), ('extension','.zip'), ('command','host %ROM%')]:
                    E.SubElement(n,k).text=v
                e = E.SubElement(E.SubElement(n,'emulators'), 'emulator', name='retroarch')
                E.SubElement(E.SubElement(e,'cores'),'core').text=core
            source=p/'es.cfg'; E.ElementTree(xml).write(source)
            rows={sid:(real,shelf) for sid,real,shelf,_ in systems.systems(source,p/'card2',cores)}
            self.assertEqual(rows['psp'][0], p/'card2/psp')
            self.assertIn('genesis',rows)
            self.assertIn('gamecube',rows)
            self.assertIn('fds',rows)
            self.assertIn('sufami',rows)
            self.assertEqual(rows['snes'][0],p/'card2/snes')
            for sid in ('ngp','ngpc','arcade','psp','dreamcast'):
                self.assertIsNotNone(rows[sid][1].find("emulators/emulator[@name='pocket-core']"))
            for sid in ('atarilynx','gamecube'):
                self.assertIsNone(rows[sid][1].find("emulators/emulator[@name='pocket-core']"))
            # test-6: the 3D systems as on ROCKNIX, only with the plain 64-bit cores.
            self.assertEqual([c.text for c in rows['psp'][1].findall("emulators/emulator[@name='pocket-core']/cores/core")], ['ppsspp'])
            self.assertEqual([c.text for c in rows['dreamcast'][1].findall("emulators/emulator[@name='pocket-core']/cores/core")], ['flycast'])

    def test_3d_systems_keep_the_darkos_route_without_their_64_bit_core(self):
        with tempfile.TemporaryDirectory() as t:
            p = Path(t); cores = p / 'cores'; cores.mkdir()
            # Only the variants dArkOS lists besides flycast: 32-bit and rumble builds are never pocket-core's.
            (cores / 'flycast_rumble_libretro.so').write_bytes(b'\x7fELF\x02\x01' + b'\0' * 12 + b'\xb7\0')
            (cores / 'ppsspp_libretro.so').write_bytes(b'\x7fELF\x01\x01' + b'\0' * 14)
            xml = E.Element('systemList')
            for sid, folder in [('dreamcast', 'dreamcast'), ('ppsspp', 'psp'), ('pspminis', 'pspminis')]:
                (p / 'card' / folder).mkdir(parents=True)
                n = E.SubElement(xml, 'system')
                for k, v in [('name', sid), ('path', '/roms/' + folder), ('extension', '.iso'), ('command', 'host %ROM%')]:
                    E.SubElement(n, k).text = v
                e = E.SubElement(E.SubElement(n, 'emulators'), 'emulator', name='retroarch')
                E.SubElement(E.SubElement(e, 'cores'), 'core').text = 'flycast_rumble'
            source = p / 'es.cfg'; E.ElementTree(xml).write(source)
            rows = {sid: shelf for sid, _, shelf, _ in systems.systems(source, p / 'card', cores)}
            for sid in ('dreamcast', 'psp', 'pspminis'):
                self.assertIsNone(rows[sid].find("emulators/emulator[@name='pocket-core']"), sid)
                self.assertIsNotNone(rows[sid].find("emulators/emulator[@name='darkos']"), sid)

    def test_host_selection_uses_es_system_and_game_choices(self):
        node=E.fromstring('<system><name>ppsspp</name><command>host %EMULATOR% %ROM%</command><emulators><emulator name="standalone-vulkan"/><emulator name="standalone" command="custom %ROM%"/></emulators></system>')
        self.assertEqual(launch.choose(node,{}, {})[1], 'standalone-vulkan')
        self.assertEqual(launch.choose(node,{'ppsspp.emulator':'standalone'}, {})[:2], ('custom %ROM%', 'standalone'))
        self.assertEqual(launch.choose(node,{'ppsspp.emulator':'standalone'}, {'emulator':'standalone-vulkan'})[1], 'standalone-vulkan')

    def test_filename_cannot_become_shell_code_in_any_placeholder(self):
        with tempfile.TemporaryDirectory() as t:
            attack="a '$() `touch PWNED` ; & b.iso"
            values={k:attack for k in ('ROM','ROM_RAW','BASENAME','SYSTEM','EMULATOR','CORE','GOVERNOR')}
            for token in values:
                for quote in ('', "'", '"'):
                    line=launch.command_line('printf %s '+quote+'%'+token+'%'+quote, values)
                    result=subprocess.check_output(['/bin/sh','-c',line],cwd=t,text=True)
                    self.assertEqual(result,attack)
            self.assertFalse((Path(t)/'PWNED').exists())

    def test_unknown_tokens_and_invalid_emulator_names_refused(self):
        with self.assertRaises(ValueError): launch.command_line('host %UNKNOWN%',{})
        node=E.fromstring('<system><name>x</name><emulators><emulator name="bad;touch"/></emulators></system>')
        with self.assertRaises(ValueError): launch.choose(node,{}, {})

    def test_retroarch_gets_the_session_shortcuts_as_appendconfig(self):
        # pocket-ui's HOME shortcuts (ra_hotkeys::prepare_darkos) reach dArkOS's RetroArch for the session only: an
        # --appendconfig on dArkOS's own command line, never a change to ~ark/.config/retroarch/retroarch.cfg.
        with tempfile.TemporaryDirectory() as t:
            session = Path(t) / 'retroarch-shortcuts.cfg'
            es = 'sudo perfmax %GOVERNOR% %ROM%; nice -n -19 /usr/local/bin/%EMULATOR% -L /home/you/.config/%EMULATOR%/cores/%CORE%_libretro.so %ROM%; sudo perfnorm'
            # A game whose name holds the program's path must stay one untouched argument.
            rom = '/roms/snes/x /usr/local/bin/retroarch y.sfc'
            values = dict(ROM=rom, ROM_RAW=rom, BASENAME='b', SYSTEM='snes', EMULATOR='retroarch', CORE='snes9x', GOVERNOR='performance')
            self.assertEqual(launch.with_session(es, 'retroarch', session), es, "no session file: dArkOS's own hotkeys")
            session.write_text('input_enable_hotkey_btn = "10"\n')
            line = launch.command_line(launch.with_session(es, 'retroarch', session), values)
            self.assertIn('nice -n -19 /usr/local/bin/retroarch --appendconfig=' + str(session) + ' -L /home/you/.config/retroarch/cores/snes9x_libretro.so ', line)
            self.assertEqual(line.count('--appendconfig'), 1)
            argv = subprocess.check_output(['/bin/bash', '-c', 'printf "%s\\n" ' + line.split('; ')[1].split('/usr/local/bin/retroarch ', 1)[1]], text=True).splitlines()
            self.assertEqual(argv, ['--appendconfig=' + str(session), '-L', '/home/you/.config/retroarch/cores/snes9x_libretro.so', rom])
            r32 = launch.command_line(launch.with_session(es, 'retroarch32', session), dict(values, EMULATOR='retroarch32'))
            self.assertIn('/usr/local/bin/retroarch32 --appendconfig=', r32)
            # Fixed RetroArch commands too; emulator scripts and standalone emulators keep their own configuration.
            fixed = 'nice -n -19 /usr/local/bin/retroarch -L /home/you/.config/retroarch/cores/snes9x_libretro.so %ROM%'
            self.assertIn('retroarch --appendconfig=', launch.with_session(fixed, 'retroarch', session))
            for other, emu in (('nice -n -19 /usr/local/bin/ppsspp.sh %EMULATOR% %ROM%', 'standalone'),
                               ('nice -n -19 /usr/local/bin/amiga.sh %EMULATOR% %CORE% %ROM%', 'retroarch'),
                               ('nice -n -19 /usr/local/bin/%EMULATOR% %ROM%', 'mupen64plus'),
                               ('nice -n -19 /usr/local/bin/retroarch.sh %ROM%', 'retroarch')):
                self.assertEqual(launch.with_session(other, emu, session), other)
            # A session path with a space stays one word for bash.
            spaced = Path(t) / 'a dir' / 'retroarch-shortcuts.cfg'; spaced.parent.mkdir(); spaced.write_text('x')
            line = launch.command_line(launch.with_session('/usr/local/bin/retroarch -L c %ROM%', 'retroarch', spaced), values)
            argv = subprocess.check_output(['/bin/bash', '-c', 'printf "%s\\n" ' + line.split('retroarch', 1)[1]], text=True).splitlines()
            self.assertEqual(argv[0], '--appendconfig=' + str(spaced))

    def test_session_shortcuts_never_replace_darkos_appendconfig(self):
        # RetroArch keeps only the last --appendconfig: dArkOS's own files stay, ours go behind them ("a|b", later wins).
        with tempfile.TemporaryDirectory() as t:
            p = Path(t); session = p / 'retroarch-shortcuts.cfg'; session.write_text('x')
            values = dict(ROM='/roms/coco/g.dsk', ROM_RAW='/roms/coco/g.dsk', BASENAME='g', SYSTEM='coco', EMULATOR='retroarch', CORE='xroar', GOVERNOR='p')
            run = lambda line: subprocess.check_output(['/bin/bash', '-c', 'printf "%s\\n" ' + line], text=True).splitlines()
            for own in ('--appendconfig=/c/coco.cfg', '--appendconfig /c/coco.cfg', '--appendconfig="/c/coco.cfg"'):
                es = 'nice -n -19 /usr/local/bin/%EMULATOR% -L /c/%CORE%_libretro.so ' + own + ' %ROM%; sudo perfnorm'
                line = launch.command_line(launch.with_session(es, 'retroarch', session), values)
                self.assertEqual(line.count('--appendconfig'), 1, line)
                argv = run(line.split('/usr/local/bin/retroarch ', 1)[1].split(';')[0])
                self.assertIn('--appendconfig=/c/coco.cfg|' + str(session), argv, line)
            # The wrapper's retroarch.cfg.ext (an external pad on an RK3566) comes before ours, as the wrapper would load it.
            home, boot, inputs = p / 'ark', p / 'boot', p / 'input'
            for d in (home / '.config/retroarch', boot, inputs): d.mkdir(parents=True)
            (home / '.config/retroarch/retroarch.cfg.ext').write_text('input_player1_joypad_index = "1"\n')
            for i in range(9): (inputs / ('event%d' % i)).touch()
            devices = p / 'devices'
            one = 'N: Name="retrogame_joypad"\nH: Handlers=event2 js0\n\n'
            devices.write_text(one + 'N: Name="Xbox Wireless Controller"\nH: Handlers=event8 js1\n')
            ext = lambda: launch.wrapper_appendconfig('retroarch', home, boot, devices, inputs)
            self.assertIsNone(ext(), 'not an RK3566: the wrapper appends nothing')
            (boot / 'rk3566.dtb').touch()
            self.assertEqual(ext(), home / '.config/retroarch/retroarch.cfg.ext')
            es = 'nice -n -19 /usr/local/bin/%EMULATOR% -L /c/%CORE%_libretro.so %ROM%'
            line = launch.command_line(launch.with_session(es, 'retroarch', session, [ext()]), values)
            self.assertIn('--appendconfig=' + str(home / '.config/retroarch/retroarch.cfg.ext') + '|' + str(session), run(line.split('retroarch ', 1)[1]))
            devices.write_text(one)
            self.assertIsNone(ext(), 'the built-in pad alone: the wrapper appends nothing, so neither do we')

if __name__ == '__main__': unittest.main()
