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

if __name__ == '__main__': unittest.main()
