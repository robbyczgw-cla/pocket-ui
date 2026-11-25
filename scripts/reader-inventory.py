#!/usr/bin/env python3
"""Conservative source inventory; a changed read site requires explicit review.

Includes IO wrappers, directory/metadata and mmap reads. Explicit cfg(test) items are skipped with balanced Rust braces, without
truncating the file at the first test helper or hiding later production code. This is a change detector, not a dynamic coverage claim.
"""
import collections
import difflib
from pathlib import Path
import re
import sys
ROOT = Path(__file__).resolve().parent.parent
PATTERN = re.compile(r'\b(?:read|read_to_string|read_to_end|read_exact|read_at|read_vectored|pread|pread64|read_line|read_until|read_dir|read_link|metadata|symlink_metadata|open|from_reader|map_copy|map_copy_read_only|read_json_or_backup|load_image|load_png|load_jpeg|read_small)\s*\(|\bMmap(?:Options)?::map\s*\(')
# The dynamic cases tied to each production read boundary. Modules not listed
# remain explicit audit work; recording a site never marks it dynamically covered.
COVERAGE = {
    'kids.rs': ['kids-config', 'kids-ledger'], 'library_meta.rs': ['library-meta'],
    'pad_bindings.rs': ['controllers'], 'emu_choice.rs': ['emu-choice'], 'system_cfg.rs': ['system-cfg'],
    'core_options.rs': ['core-options'], 'battery_history.rs': ['battery-history'], 'battery_games.rs': ['battery-games'], 'battery_curve.rs': ['battery-curve'],
    'achievements/account.rs': ['ra-account'], 'achievements/mastery.rs': ['ra-mastery'],
    'achievements/offline.rs': ['ra-pending', 'ra-offline'], 'ports.rs': ['port-catalog', 'port-installed'],
    'cheat_pack.rs': ['cheat-pack'], 'cheat_download.rs': ['cheat-pack'], 'update/cheats.rs': ['cheat-download'],
    'emu_settings.rs': ['emu-dolphin-gfx','emu-dolphin-main','emu-dolphin-game','emu-gc-custom','emu-gc-stock','emu-wii-custom','emu-wii-stock','emu-azahar','emu-pcsx2','emu-pcsx2-game','emu-system'],
    'library_sync.rs': ['library-sync-own','library-sync-foreign','library-history-own','library-history-foreign'],
    'save_copy.rs': ['save-copy-index'], 'port_perf.rs': ['port-perf-game','port-perf-console','port-perf-notes'],
    'game_pictures.rs': ['snap-missing','title-missing'], 'text_fallback.rs': ['fallback-font'],
    'port_files.rs': ['port-game-files'], 'document_library.rs': ['bios-psx','bios-saturn','bios-dreamcast'],
    'update/observation.rs': ['update-pending','update-known-good'],
    'update/brick_seed.rs': ['brick-seed-source','brick-seed-target','brick-seed-marker'], 'untar/brick.rs': ['brick-seed-archive'],
    'update/brick_assets.rs': ['brick-assets-manifest','brick-asset-archive'], 'update/brick_delta.rs': ['brick-delta-file'],
    'update/brick.rs': ['brick-app-archive'], 'update/brick_io.rs': ['brick-app-archive','brick-delta-file'],
    'j2me_size.rs': ['j2me-jar', 'j2me-jad', 'j2me-options'], 'j2me_keys.rs': ['j2me-remap'], 'j2me_java.rs': ['j2me-java'],
    'android_cores.rs': ['android-core-manifest', 'android-core-archive'],
    'android_games.rs': ['android-apps','android-app-icon'], 'android_handheld.rs': ['android-mobile-history', 'android-sidecar'],
    'update/card.rs': ['update-card'], 'update/payload.rs': ['update-payload'], 'untar.rs': ['seed-archive'],
    'fsutil.rs': ['json-backup'], 'game_files.rs': ['cue', 'gdi'], 'frozen.rs': ['cue', 'gdi'],
    'lastframe.rs': ['kept-frame', 'state-preview'], 'launch_picture.rs': ['kept-frame', 'state-preview'],
    'web/mod.rs': ['web-prefs'], 'tailscale.rs': ['tailscale-prefs'], 'perf_fan.rs': ['performance-prefs'],
    'moonlight.rs': ['moonlight-pcs'], 'on_start.rs': ['last-game', 'started-boot'], 'auto_off.rs': ['auto-off'], 'fallback.rs': ['fallback'],
    'achievements/snapshot.rs': ['ra-summary', 'ra-snapshot'], 'achievements/lookup.rs': ['ra-console-progress'],
    'players.rs': ['players-order', 'players-seats'], 'crash.rs': ['crash-marker', 'crash-log'],
    'rescue.rs': ['crash-log', 'launcher-log','rescue-es-note'], 'save_trouble.rs': ['save-trouble'], 'hostsys.rs': ['scummvm-ini'],
    'sleep.rs': ['sleep-config','sleep-resume'], 'rumble.rs': ['haptics'], 'leds.rs': ['leds-config'],
    'display.rs': ['display-config'], 'storage.rs': ['storage-warning'],
    'ui_sound.rs': ['sounds-config'], 'night_light.rs': ['night-light'], 'bezel.rs': ['frame-pack'], 'insert_rig.rs': ['rig-json', 'rig-base'],
    'power.rs': ['power-restore'], 'pixel_path.rs': ['pixel-report'], 'pixel_standalone.rs': ['pixel-standalone','pixel-disc-plain','pixel-disc-rvz','pixel-disc-wia','pixel-disc-ciso','pixel-disc-wbfs','pixel-disc-gcz-stored','pixel-disc-gcz-deflate'],
    'search.rs': ['recent-searches'], 'wiki_text.rs': ['wiki-cache'], 'eject.rs': ['eject-note'],
    'setup.rs': ['setup-resume'], 'game_check.rs': ['check-zip','check-rom'], 'document_check.rs': ['document-zip'],
    'fan_curve.rs': ['fan-recovery-note','fan-beat'], 'dpad_stick_rocknix.rs': ['dpad-remap'],
    'starter.rs': ['starter-zip'], 'quick.rs': ['kids-normal-volume'], 'brick_hw.rs': ['brick-fkeys','brick-lights','brick-slider'],
    'case_knows_art.rs': ['case-art-gba-rom'],
    'libretro.rs': ['sram-file','sram-oversize','rtc-file','core-resume-raw','core-resume-compressed'],
    'boot_screen.rs': ['boot-rom-header','boot-gbc-bios'], 'run_ahead.rs': ['run-ahead-info'],
    'report.rs': ['report-settings','report-log','report-network-profile','report-core-info','report-bluetooth-profile','report-portal','report-previous-portal','report-store','report-share-password','report-remote-password','report-hostname','report-system-cfg','report-core-error','report-live-log','report-previous-log'],
    'text.rs': ['installed-title-font','installed-body-font'], 'os_compat.rs': ['elf-executable'],
    'web/auth.rs': ['web-sessions'], 'timezone.rs': ['timezone-file','timezone-list'],
    'library_systems.rs': ['systems'], 'tools.rs': ['gamelist'],
    'backup.rs': ['backup'], 'backup_saves.rs': ['backup-saves'], 'save_transfer.rs': ['save-transfer'],
    'sync.rs': ['sync-prefs', 'sync-config'], 'sync_conflicts.rs': ['sync-conflict'],
    'frozen_phone.rs': ['frozen-mark'], 'timeline.rs': ['timeline'], 'logs.rs': ['boot-state', 'kernel-log', 'sensors-log'],
    'secret.rs': ['secret'], 'gallery.rs': ['gallery'], 'album.rs': ['album-index', 'album-seen', 'album-settings'], 'app_album.rs': ['album-index', 'jpeg'], 'preview.rs': ['preview'], 'dsv.rs': ['dsv'],
    'settings.rs': ['settings'], 'favorites.rs': ['favorites'], 'recents.rs': ['recents'],
    'system_list.rs': ['systems'], 'library.rs': ['systems', 'playlist'], 'gamelist.rs': ['gamelist'],
    'library_cache.rs': ['library-cache'], 'store.rs': ['installed'], 'cheats.rs': ['cheats', 'own-cheats'],
    'disc.rs': ['disc'], 'softpatch.rs': ['ips', 'bps', 'ups'], 'archive.rs': ['archive', 'runtime-archive'], 'seven_zip.rs': ['7z'],
    'image.rs': ['cover', 'jpeg', 'ppm'], 'covers.rs': ['cover', 'cover-missing'], 'frame_cache.rs': ['frame-cache'],
    'update.rs': ['update'], 'update/channel.rs': ['update'], 'update_hint.rs': ['update-hint'],
    'slots.rs': ['moments'], 'state_file.rs': ['moments','core-resume-raw','core-resume-compressed'], 'game_settings.rs': ['game-settings', 'console-settings'],
    'curator/prefs.rs': ['curator'], 'rom_id.rs': ['checksums'],
    'case_knows_read.rs': ['case-cache', 'art-cache', 'case-knows'],
    'case_knows.rs': ['case-knows'], 'disc_serial.rs': ['disc-cache', 'playlist'], 'play_log.rs': ['play-log'],
}

# Private parsers can use the identical policy from tests/common/data_faults.rs
# in their own module. Keep their test identity explicit so removal fails too.
UNIT_COVERAGE = {
    'low_battery.rs': [
        ('src/low_battery.rs', 'reads_the_level_and_the_charger_from_sysfs'),
        ('src/low_battery.rs', 'r36s_does_not_switch_off_when_critical_percent_disagrees_with_voltage'),
    ],
    'background_io.rs': [
        ('src/background_io.rs', 'file_source_rule_background_reads_have_the_same_bytes_and_errors'),
        ('src/background_io.rs', 'r36s_readers_a_cancelled_job_cannot_read_seek_walk_or_stat'),
    ],
    'startup.rs': [('src/startup.rs', 'n64_wait_samples_survive_missing_and_damaged_proc_fields')],
    # Cover folders listed once (`dir_index`): a missing, unreadable or changed folder, FAT's coarse times.
    'dir_index.rs': [('src/dir_index.rs', 'a_folder_is_read_once_and_names_it_lacks_cost_nothing')],
    # The dArkOS motor probe (`rumble::look_for_motor`): no force-feedback device means no Haptics row.
    'rumble.rs': [('src/darkos.rs', 'haptics_goes_where_dark_os_has_nothing_to_vibrate')],
    'covers/custom.rs': [('src/web/tests/cover_diagnostics_tests.rs', 'covers_are_per_game_and_refuse_links_out_of_the_cover_folder')],
    'storage.rs': [('src/storage.rs', 'clearing_downloaded_pictures_keeps_custom_covers_and_backups')],
    'ra_hotkeys.rs': [('src/ra_hotkeys.rs', 'broken_inputs_never_break_the_players_config')],
    'ra_state.rs': [('src/ra_state.rs', 'save_requires_a_finished_new_state_for_this_game')],
    'web/cover_edit.rs': [('src/web/tests/cover_diagnostics_tests.rs', 'cover_empty_state_and_edit_round_trip_with_backups')],
    'web/diagnostics.rs': [
        ('src/web/tests/cover_diagnostics_tests.rs', 'the_diagnosis_zip_only_contains_version_model_and_safe_events'),
        ('src/web/tests/cover_diagnostics_tests.rs', 'diagnostic_empty_states_include_explanations_instead_of_blank_files'),
    ],
    'android_handheld/files.rs': [('src/android_handheld/files.rs','file_source_rule_handheld_activity_files'),('src/android_handheld/files.rs','file_source_rule_handheld_return_picture')],
    'android/src/play_cards.rs': [('src/android_02_tests.rs', 'file_source_rule_mobile_preferences_and_buttons')],
    'dpad_stick_rocknix.rs': [('src/dpad_stick_rocknix.rs', 'file_source_rule_standalone_dpad_settings')],
    # A GitHub symlink's text in the cover download (`covers::symlink_in`): missing/empty/cut/flipped/encoding/too big.
    'covers.rs': [('src/covers.rs', 'a_github_symlink_is_followed_to_the_picture_beside_it')],
    # Pocket-to-Pocket sync's connection; here, not at the end, where every new module's line lands.
    'pocket_sync_page.rs': [('src/pocket_sync_page.rs', 'file_source_rule_pocket_sync_prefs')],
    'pocket_sync/net.rs': [('src/pocket_sync/net_tests.rs', 'file_source_rule_sync_peers_device_copies_and_saves')],
    'pocket_sync/log.rs': [('src/pocket_sync/log.rs', 'file_source_rule_pocket_sync_log')],
    'case_knows_art.rs': [('src/case_knows_art.rs', 'file_source_rule_case_rom_art')],
    'shelf_space.rs': [('src/shelf_space.rs', 'file_source_rule_shelf_space')],
    'h700_volume.rs': [('src/h700_volume.rs', 'file_source_rule_h700_ucm')],
    'brick_profile.rs': [('src/brick_profile.rs', 'file_source_rule_brick_profile')],
    'cheat_data.rs': [('src/cheat_data.rs', 'file_source_rule_cheat_download_status')],
    'hakux.rs': [('src/hakux.rs', 'file_source_rule_hakux_settings')],
    'showcase.rs': [('src/showcase.rs', 'file_source_rule_showcase_frames')],
    'splash.rs': [('src/splash.rs', 'file_source_rule_splash_settings')],
    'standalone.rs': [('src/standalone.rs', 'file_source_rule_standalone_game_headers'), ('src/standalone.rs', 'file_source_rule_standalone_configuration')],
    'system_ctl.rs': [('src/system_ctl.rs', 'file_source_rule_real_hardware_preferences')],
    'wifi.rs': [('src/wifi.rs', 'file_source_rule_wifi_configuration')],
    'android_games.rs': [('src/android_games.rs', 'file_source_rule_android_app_icons'), ('src/android_02_tests.rs', 'file_source_rule_mobile_preferences_and_buttons')],
    'platform/android_content.rs': [('src/platform/android_content.rs', 'file_source_rule_document_identity_cache')],
    'fn_keys.rs': [('src/fn_keys.rs', 'file_source_rule_kids_key_restore')],
    'core_prewarm.rs': [('src/core_prewarm.rs', 'file_source_rule_private_prefetch_elf')],
    'h700_gba.rs': [('src/h700_gba.rs', 'file_source_rule_gba_state_headers')],
    'notice.rs': [('src/notice.rs', 'file_source_rule_notice_files')],
    'mako_osd.rs': [('src/mako_osd.rs', 'file_source_rule_notification_config')],
    'hw_render.rs': [('src/hw_render.rs', 'file_source_rule_retroarch_option_files')],
    'case_knows.rs': [('src/case_knows_fuzz_tests.rs', 'file_source_rule_starts_with_a_recognised_save_of_every_layout')],
    'case_knows_read.rs': [('src/case_knows_fuzz_tests.rs', 'file_source_rule_starts_with_a_recognised_save_of_every_layout')],
    'pocket_sync.rs': [('src/pocket_sync/tests.rs', 'file_source_rule_sync_record_crc_cache_and_saves')],
    'android_first_start.rs': [('src/android_first_start.rs', 'file_source_rule_first_start_games_folders')],
    'romm.rs': [('src/romm_tests.rs', 'file_source_rule_romm_account_token_catalog_device_id_and_parts')],
    'romm_net.rs': [('src/romm_net.rs', 'file_source_rule_http_answers')],
    # PortMaster games from the card: zips, port.json inside them, runtime files.
    'ports_card.rs': [('src/ports_card_tests.rs', 'file_source_rule_card_ports')],
}

BIN_UNIT_COVERAGE = {
    'app_bluetooth.rs': [('src/app_bluetooth.rs', 'file_source_rule_bluetooth_switch')],
    'app_tailscale.rs': [('src/app_tailscale.rs', 'file_source_rule_tailscale_switch')],
    'app_card_update.rs': [('src/app_card_update.rs', 'file_source_rule_card_update_seen')],
    'app_library.rs': [('src/app_library.rs', 'file_source_rule_library_unread_count')],
    'app_sync_conflicts.rs': [('src/app_sync_conflicts.rs', 'file_source_rule_sync_conflicts_seen')],
    'app_cabinet.rs': [('src/app_cabinet.rs', 'file_source_rule_cover_workers')],
    'app_romm.rs': [('src/app_romm.rs', 'file_source_rule_romm_server_file')],
}

JVM_COMPANION_COVERAGE = {
    'android/java/xyz/pocketui/app/BiosFiles.java': ['scripts/android/test/file-reader-rule.sh'],
    **{f'android/java/xyz/pocketui/app/{name}.java': ['scripts/android/test/java-reader-rule.sh']
       for name in ['GameCopy','GameDirect','SaveBackup','SaveCopy','UpdateInstaller','Views']},
}
JVM_CASE_COVERAGE = {
    'GameCopy': ['game-stage','game-set-stage','game-copy-stream','game-copy-set-stream'],
    'GameDirect': ['direct-chd3','direct-chd4','direct-chd5'],
    'SaveBackup': ['backup-zip','backup-export-source'],
    'SaveCopy': ['save-copy-manifest','save-copy-file','save-copy-read-error','save-copy-put'],
    'UpdateInstaller': ['update-notice','update-download','update-read','update-apk-stream'],
    'Views': ['views-handheld','views-mobile'],
}

def rust_code(text):
    """Keep Rust punctuation while blanking comments and string/char contents."""
    token = re.compile(r'//[^\n]*|/\*|(?:br|r)(?P<hash>\#*)"|b?"|b?\'(?:\\.|[^\\\'\n])\'')
    chars = list(text)
    pos = 0
    while True:
        m = token.search(text, pos)
        if not m: break
        start, end = m.span()
        if m.group().startswith('//'):
            pass
        elif m.group() == '/*':
            depth = 1
            while depth and end < len(text):
                n = re.search(r'/\*|\*/', text[end:])
                if not n: end = len(text); break
                depth += 1 if n.group() == '/*' else -1
                end += n.end()
        elif m.group('hash') is not None:
            ending = '"' + m.group('hash')
            n = text.find(ending, end)
            end = len(text) if n < 0 else n + len(ending)
        elif m.group().endswith('"'):
            while end < len(text):
                if text[end] == '\\': end += 2
                elif text[end] == '"': end += 1; break
                else: end += 1
        for i in range(start, min(end, len(text))):
            if chars[i] != '\n': chars[i] = ' '
        pos = end
    return ''.join(chars)

def requires_test(expression):
    """Conservatively prove cfg cannot hold without test; unknown cfg stays."""
    expression=expression.strip()
    if expression=='test': return True
    m=re.fullmatch(r'(all|any)\((.*)\)',expression,re.S)
    if not m: return False
    parts=[]; start=0; depth=0
    for at,c in enumerate(m.group(2)):
        depth += (c=='(')-(c==')')
        if c==',' and depth==0:
            parts.append(m.group(2)[start:at]); start=at+1
    parts.append(m.group(2)[start:])
    flags=[requires_test(part) for part in parts if part.strip()]
    return (any(flags) if m.group(1)=='all' else bool(flags) and all(flags))

def without_test_items(text):
    code = rust_code(text)
    spans = []
    for m in re.finditer(r'#\s*\[\s*cfg\s*\(([^]]*)\)\s*\]', code):
        expression = m.group(1).strip()
        if not requires_test(expression): continue
        # Attributes on the same item can precede its declaration.
        pos = m.end()
        while True:
            a = re.match(r'\s*#\s*\[[^]]*\]', code[pos:])
            if not a: break
            pos += a.end()
        opening = re.search(r'[;{]', code[pos:])
        if not opening: continue
        end = pos + opening.end()
        if opening.group() == '{':
            depth = 1
            while depth and end < len(code):
                depth += (code[end] == '{') - (code[end] == '}')
                end += 1
        spans.append((m.start(), end))
    chars = list(text)
    for start, end in spans:
        for i in range(start, end):
            if chars[i] != '\n': chars[i] = ' '
    return ''.join(chars)

FS_COPY = re.compile(r'\b(?:std::)?fs::copy\s*\(')

JAVA_READ = re.compile(r'\b(?:FileInputStream|FileReader|RandomAccessFile|DataInputStream|BufferedInputStream|ZipFile|ZipInputStream)\s*\(|\b(?:openInputStream|openFileDescriptor|readAllBytes|readBytes|readText|readString|readLines|bufferedReader|inputStream|newInputStream|newBufferedReader|getResourceAsStream|openRawResource|openAssetFileDescriptor|decodeFile|decodeStream|loadThumbnail|readUTF|readFully|listFiles)\s*\(')

def sites():
    result = []
    files = [*(ROOT / 'src').rglob('*.rs'), *(ROOT / 'android').rglob('*.rs')]
    files += [*(ROOT / 'android').rglob('*.java'), *(ROOT / 'android').rglob('*.kt')]
    for file in sorted(set(files)):
        if any(part in {'target', 'build', '.git', '.gradle'} for part in file.relative_to(ROOT).parts): continue
        if file.stem.endswith(('_tests', '_test')) or file.stem.startswith('test'):
            continue
        counts = collections.Counter()
        production=without_test_items(file.read_text())
        # Search actual code, not literals. Splitting at // would hide a reader
        # after a URL string on the same line (notably Android content URIs).
        for line, executable in zip(production.splitlines(), rust_code(production).splitlines()):
            code = line.strip()
            scan = re.sub(r'\bfn\s+\w+\s*\(', 'fn declaration(', executable)
            if file.suffix == ".rs": scan = re.sub(r'\.read\(\s*\)', '.lock_read()', scan)
            if (PATTERN.search(scan) or FS_COPY.search(scan) or file.suffix != '.rs' and JAVA_READ.search(scan)) :
                counts[code] += 1
        for code, count in sorted(counts.items()):
            result.append((str(file.relative_to(ROOT)), count, code))
    return result

def inventory():
    return ''.join(f'{file}\t{count}\t{code}\n' for file, count, code in sites())

def coverage():
    by_file = collections.Counter()
    for file, count, _ in sites(): by_file[file] += count
    def labels(file):
        module=file.removeprefix('src/')
        return COVERAGE.get(module, []) + ['unit:'+name for _, name in UNIT_COVERAGE.get(module, [])] + ['bin-unit:'+name for _, name in BIN_UNIT_COVERAGE.get(module, [])] + ['jvm-companion:'+path for path in JVM_COMPANION_COVERAGE.get(module, [])] + ['jvm-case:'+name for name in JVM_CASE_COVERAGE.get(Path(module).stem, [])]
    return ''.join(f'{file}\t{count}\t{",".join(labels(file)) or "UNASSIGNED"}\n' for file, count in sorted(by_file.items()))

if __name__ == '__main__':
    baseline = ROOT / 'tests/fixtures/reader-inventory.tsv'
    # A cfg(test) helper before production used to hide that production reader.
    sample = '#[cfg(test)] fn helper() { let x = r#"{\\\"}"#; fs::read("test"); }\nfn real() { fs::read("real"); }'
    stripped = without_test_items(sample)
    assert 'fs::read("real")' in stripped and 'fs::read("test")' not in stripped
    # A comment whose last character is a quote must not consume the next item.
    assert 'fn real()' in rust_code('// comment ends in a quote "\nfn real() { fs::read("real"); }')
    assert requires_test('all(test, feature=abc)')
    assert not requires_test('all(any(test, feature=abc), target_os=linux)')
    assert not requires_test('any(test, target_os=android)')
    assert FS_COPY.search('std::fs::copy(from,to)') and FS_COPY.search('fs::copy(a,b)')
    assert not FS_COPY.search('model.copy(a,b)')
    assert PATTERN.search('file.read_at(buf,0)') and PATTERN.search('libc::pread(fd,buf,n,0)')
    assert PATTERN.search(rust_code('let uri="content://game"; fs::read(uri);'))
    assert not PATTERN.search(rust_code('let example="fs::read(file)"; // fs::read(comment)'))
    actual = inventory()
    if sys.argv[1:] == ['--write']:
        baseline.write_text(actual)
        (ROOT / 'tests/fixtures/reader-coverage.tsv').write_text(coverage())
    elif sys.argv[1:] == ['--coverage']:
        sys.stdout.write(coverage())
    else:
        if baseline.read_text() != actual:
            sys.stderr.writelines(difflib.unified_diff(baseline.read_text().splitlines(True), actual.splitlines(True), fromfile='reviewed readers', tofile='current readers'))
            sys.exit('Reader changed: add its missing/empty/truncated/flipped/encoding case and audit the inventory.')
        # A case cannot silently disappear while its source module stays recorded.
        cases = set()
        for path in [ROOT / 'tests/data_sources.rs', *ROOT.glob('tests/common/data_*.rs')]:
            content = path.read_text()
            for declaration in re.finditer(r'const\s+(?:[A-Z0-9_]*SOURCES)\s*:[^=]*=\s*&?\[', content):
                test = content[declaration.end():].split('];', 1)[0]
                cases.update(re.findall(r'"([a-z0-9_-]+)"', test))
        missing = {case for entries in COVERAGE.values() for case in entries} - cases
        if missing: sys.exit(f'Inventoried fault cases missing from SOURCES: {sorted(missing)}')

        for entries in [*UNIT_COVERAGE.values(), *BIN_UNIT_COVERAGE.values()]:
            for file, name in entries:
                if not re.search(r'\bfn\s+'+re.escape(name)+r'\s*\(', (ROOT/file).read_text()):
                    sys.exit(f'Inventoried private fault test missing: {file}::{name}')

        for paths in JVM_COMPANION_COVERAGE.values():
            for path in paths:
                if not (ROOT/path).is_file(): sys.exit(f'Inventoried JVM companion missing: {path}')

        java_rule=(ROOT/'scripts/android/test/JavaReaderRule.java').read_text()
        java_sources=java_rule.split('SOURCES=',1)[1].split(';',1)[0]
        java_cases=set(re.findall(r'"([a-z0-9-]+)"',java_sources))
        expected_java={case for cases in JVM_CASE_COVERAGE.values() for case in cases}
        if java_cases != expected_java:
            sys.exit(f'Java reader registry mismatch: missing={sorted(expected_java-java_cases)}, unassigned={sorted(java_cases-expected_java)}')
