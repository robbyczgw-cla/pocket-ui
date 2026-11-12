#!/usr/bin/env python3
"""Run the pinned/patched core's real buffer and libretro state functions.

Usage: python3 scripts/android/test-yaba-state.py android/build/cores/source/yabasanshiro
The stream stub seeks back to patch its header, as Yaba's actual serializer does.
The app-private folder is writable; the default tmpfile function is unavailable.
"""
import os, pathlib, subprocess, sys, tempfile

def function(source, name):
    start = source.index(name + '(') if name + '(' in source else source.index(name + ' (')
    start = source.rfind('\n', 0, start) + 1
    brace = source.index('{', start)
    depth = 1
    end = brace + 1
    while depth:
        depth += (source[end] == '{') - (source[end] == '}')
        end += 1
    return source[start:end]

root = pathlib.Path(sys.argv[1]) / 'yabause/src'
memory = (root / 'memory.c').read_text()
retro = (root / 'libretro/libretro.c').read_text()
code = r'''
#define _GNU_SOURCE
#define ANDROID 1
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdbool.h>
#include <assert.h>
#include <unistd.h>
#include <limits.h>
static char g_save_dir[PATH_MAX];
static int locked, fail_write, fail_load, legacy_load, geometry_load, rendering_started = 1;
static void ScspLockThread(void) { assert(!locked); locked = 1; }
static void ScspUnLockThread(void) { assert(locked); locked = 0; }
#define SCSP_MUTE_SYSTEM 1
#define MUSASHI_STATE_BYTES 240
static void ScspMuteAudio(int x) { (void)x; }
static void ScspUnMuteAudio(int x) { (void)x; }
static void retro_set_resolution(void) {}
static FILE *unavailable_tmpfile(void) { return NULL; }
#define tmpfile unavailable_tmpfile
static const unsigned char state[512] = "YSS\1header:preserve player progress";
static int YabSaveStateStream(FILE *fp) {
    assert(locked);
    if (fail_write) return -2;
    assert(fwrite(state, 1, sizeof(state), fp) == sizeof(state));
    assert(fseek(fp, 4, SEEK_SET) == 0);
    assert(fwrite("saved!", 1, 6, fp) == 6);
    return 0;
}
static int YabLoadStateStream(FILE *fp) {
    unsigned char data[sizeof(state)], expected[sizeof(state)];
    assert(locked);
    if (fail_load) return -3;
    memcpy(expected, state, sizeof(state)); memcpy(expected + 4, "saved!", 6);
    assert(fseek(fp, 0, SEEK_END) == 0);
    size_t expected_size=geometry_load ? sizeof(state)+20480 : legacy_load ? sizeof(state)-MUSASHI_STATE_BYTES : sizeof(state);
    if ((size_t)ftell(fp) != expected_size) return -2;
    size_t prefix=expected_size<sizeof(state) ? expected_size : sizeof(state);
    assert(fseek(fp, 0, SEEK_SET) == 0);
    assert(fread(data, 1, prefix, fp) == prefix);
    assert(memcmp(data, expected, prefix) == 0);
    return 0;
}
'''
if 'YabAndroidStateFile(void)' in retro:
    code += function(retro, 'YabAndroidStateFile') + '\n'
code += '\n'.join(function(memory, n) for n in ['YabSaveStateBuffer', 'YabLoadStateBuffer'])
code += '\n'.join(function(retro, n) for n in ['retro_serialize_size', 'retro_serialize', 'retro_unserialize'])
code += r'''
int main(void) {
    snprintf(g_save_dir, sizeof(g_save_dir), "%s", getenv("TEST_STATE_DIR"));
    void *buffer = NULL; size_t size = 0;
    assert(YabSaveStateBuffer(&buffer, &size) == 0);
    assert(size == sizeof(state) && buffer);
    assert(YabLoadStateBuffer(buffer, size) == 0); free(buffer);
    assert(YabSaveStateBuffer(NULL, &size) == 0 && size == sizeof(state));
    assert(retro_serialize_size() == sizeof(state));
    unsigned char output[sizeof(state) + 16]; memset(output, 0xa5, sizeof(output));
    assert(retro_serialize(output, sizeof(state)));
    assert(retro_unserialize(output, sizeof(state)));
    legacy_load=1;assert(retro_unserialize(output,sizeof(state)-MUSASHI_STATE_BYTES));legacy_load=0;
    // Disasteroids switches 224 -> 240 lines after boot: its complete state is
    // 20,480 bytes larger. A size query of the fresh core is not a file validator.
    unsigned char wide[sizeof(state)+20480]={0};memcpy(wide,output,sizeof(state));
    geometry_load=1;
    assert(retro_unserialize(wide,sizeof(wide)) && "a valid 240-line game must Continue from a fresh 224-line core");
    assert(!retro_unserialize(wide,sizeof(wide)-1));
    geometry_load=0;

    assert(!retro_unserialize(output,sizeof(state)-1));
    assert(!retro_unserialize(output,sizeof(state)-MUSASHI_STATE_BYTES-1));
    for (int i = sizeof(state); i < sizeof(output); i++) assert(output[i] == 0xa5);
    assert(!retro_serialize(output, 3));
    assert(!retro_serialize(output, sizeof(output)));
    assert(!retro_serialize(NULL, sizeof(state)));
    assert(YabSaveStateBuffer(&buffer, NULL) != 0);
    assert(YabLoadStateBuffer(NULL, 8) != 0);
    assert(YabLoadStateBuffer(output, 0) != 0);
    char saved_dir[PATH_MAX]; strcpy(saved_dir, g_save_dir); g_save_dir[0] = 0;
    buffer = (void *)1; size = 999;
    assert(YabSaveStateBuffer(&buffer, &size) != 0 && !buffer && size == 0);
    assert(!retro_serialize(output, sizeof(state)));
    assert(!retro_unserialize(output, sizeof(state)));
    assert(retro_serialize_size() == 0);
    strcpy(g_save_dir, saved_dir); fail_write = 1;
    assert(!retro_serialize(output, sizeof(state)));
    assert(retro_serialize_size() == 0);
    fail_write = 0; fail_load = 1;
    assert(!retro_unserialize(output, sizeof(state)));
    rendering_started = 0;
    assert(retro_serialize_size() == 0);
    assert(!retro_serialize(output, sizeof(state)));
    assert(!retro_unserialize(output, sizeof(state)));
    assert(!locked);
    puts("Yaba: real state functions round-trip without tmpfile; failed states reject safely");
}
'''
with tempfile.TemporaryDirectory(prefix='yaba-state-test-') as tmp:
    p = pathlib.Path(tmp)
    (p / 'test.c').write_text(code)
    subprocess.run(['cc', '-g', '-fsanitize=address,undefined', '-o', str(p / 'test'), str(p / 'test.c')], check=True)
    subprocess.run([str(p / 'test')], check=True, timeout=10, env={**os.environ, "TEST_STATE_DIR": tmp})
    artifacts = {"test", "test.c"}
    if sys.platform == "darwin": artifacts.add("test.dSYM")  # clang -g companion, not a state file
    assert {x.name for x in p.iterdir()} <= artifacts, "State temp files must be unlinked"
