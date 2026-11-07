
#include <stdbool.h>
#include <stddef.h>
#include <string.h>
#include <unistd.h>
#include <stdlib.h>
#include <stdio.h>
static unsigned char marker = 7;
static void block(const char *phase) {
 const char *crash = getenv("POCKETUI_TEST_ABORT_PHASE");
 if (crash && !strcmp(crash, phase)) abort();
 const char *wanted = getenv("POCKETUI_TEST_BLOCK_PHASE");
 if (wanted && !strcmp(wanted, phase)) {
  FILE *f = fopen(getenv("POCKETUI_TEST_ENTERED"), "w"); fputs(phase, f); fclose(f);
  for (;;) usleep(1000);
 }
}
struct info { const char *name, *version, *exts; bool need_fullpath, block_extract; };
struct av { unsigned bw, bh, mw, mh; float aspect; double fps, rate; };
struct game { const char *path; const void *data; size_t size; const char *meta; };
void retro_set_environment(void *f) {}
void retro_set_video_refresh(void *f) {}
void retro_set_audio_sample(void *f) {}
void retro_set_audio_sample_batch(void *f) {}
void retro_set_input_poll(void *f) {}
void retro_set_input_state(void *f) {}
void retro_set_controller_port_device(unsigned port, unsigned device) {}
unsigned retro_api_version(void) { return 1; }
void retro_init(void) {}
void retro_deinit(void) {}
void retro_get_system_info(struct info *i) { memset(i, 0, sizeof *i); i->name = "stub"; i->exts = "gb"; }
void retro_get_system_av_info(struct av *a) { memset(a, 0, sizeof *a); a->bw = a->mw = 160; a->bh = a->mh = 144; a->fps = 60; a->rate = 48000; }
bool retro_load_game(const struct game *g) { block("load"); return true; }
void retro_unload_game(void) { block("unload"); }
void retro_reset(void) {}
void retro_run(void) { marker++; block("run"); }
void *retro_get_memory_data(unsigned id) { return NULL; }
size_t retro_get_memory_size(unsigned id) { return 0; }
size_t retro_serialize_size(void) { block("size"); return 16; }
bool retro_serialize(void *d, size_t s) { if (getenv("POCKETUI_TEST_BLOCK_SERIALIZE")) { FILE *f = fopen(getenv("POCKETUI_TEST_BLOCK_SERIALIZE"), "w"); fputs("entered", f); fclose(f); for (;;) usleep(1000); } memset(d, marker, s); return true; }
bool retro_unserialize(const void *d, size_t s) { block("resume"); if (s != 16) return false; marker = *(const unsigned char *)d; return true; }
