#define WIDTH 160
#define HEIGHT 144
#define FPS 60.0
#define RATE 48000.0
#define NO_SRAM
#define retro_load_game stub_default_load
#define retro_unload_game stub_default_unload
#define retro_reset stub_default_reset
#define retro_run stub_default_run
#define retro_serialize_size stub_default_size
#define retro_serialize stub_default_serialize
#define retro_unserialize stub_default_unserialize
#include "POCKET_SHARED_STUB"
#undef retro_load_game
#undef retro_unload_game
#undef retro_reset
#undef retro_run
#undef retro_serialize_size
#undef retro_serialize
#undef retro_unserialize
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
bool retro_load_game(const struct game *g) { block("load"); return stub_default_load(g); }
void retro_unload_game(void) { block("unload"); }
void retro_reset(void) {}
void retro_run(void) { marker++; block("run"); stub_default_run(); }
size_t retro_serialize_size(void) { block("size"); return 16; }
bool retro_serialize(void *d, size_t s) { if (getenv("POCKETUI_TEST_BLOCK_SERIALIZE")) { FILE *f = fopen(getenv("POCKETUI_TEST_BLOCK_SERIALIZE"), "w"); fputs("entered", f); fclose(f); for (;;) usleep(1000); } memset(d, marker, s); return true; }
bool retro_unserialize(const void *d, size_t s) { block("resume"); if (s != 16) return false; marker = *(const unsigned char *)d; return true; }
