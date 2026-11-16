/* A libretro core for the crash sweep (scripts/crash-sweep.py) and the stub-core journeys: it plays like a real one
 * as far as pocket-core can tell. Every frame it draws a picture (XRGB8888, a bar that moves with the frame count),
 * hands over a frame's worth of sound in one batch (a soft 440 Hz tone, so pocket-core's whole audio path runs:
 * the fade-in after the Cabinet's hand-over, the queue, the writer), reads the pad, keeps a battery save that changes
 * as it plays, and saves and loads its state (the frame count and the save, so a kept moment goes on where it was).
 *
 * The test cores in tests/ leave the sound out; this one doesn't, on purpose (Fable's test audit, 14.11.2026: the
 * Cabinet hand-over's divide by zero lived in the audio path no test reached).
 *
 * Build: cc -shared -fPIC -O1 [-DRATE=32040 -DFPS=60.0988 -DWIDTH=256 -DHEIGHT=224] [-DNO_SRAM] -o stub_libretro.so stub-core.c -lm
 * The defines give other cores' timings (a PAL one at 50 fps, a GBA's 32768 Hz). Original code, no game in it. */
#include <math.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>

#ifndef RATE
#define RATE 32040.0
#endif
#ifndef FPS
#define FPS 60.0988
#endif
#ifndef WIDTH
#define WIDTH 256
#endif
#ifndef HEIGHT
#define HEIGHT 224
#endif

struct info { const char *name, *version, *exts; bool need_fullpath, block_extract; };
struct geometry { unsigned bw, bh, mw, mh; float aspect; };
struct timing { double fps, rate; };
struct av { struct geometry g; struct timing t; };
struct game { const char *path; const void *data; size_t size; const char *meta; };

typedef bool (*env_t)(unsigned, void *);
typedef void (*video_t)(const void *, unsigned, unsigned, size_t);
typedef void (*sample_t)(int16_t, int16_t);
typedef size_t (*batch_t)(const int16_t *, size_t);
typedef void (*poll_t)(void);
typedef int16_t (*state_t)(unsigned, unsigned, unsigned, unsigned);

static env_t env;
static video_t video;
static batch_t batch;
static poll_t poll;
static state_t input;
static uint32_t frame;
static double phase;
static uint32_t fb[WIDTH * HEIGHT];
static uint8_t sram[8192];
static int16_t sound[2 * 2048];

void retro_set_environment(env_t f) { env = f; }
void retro_set_video_refresh(video_t f) { video = f; }
void retro_set_audio_sample(sample_t f) { (void)f; }
void retro_set_audio_sample_batch(batch_t f) { batch = f; }
void retro_set_input_poll(poll_t f) { poll = f; }
void retro_set_input_state(state_t f) { input = f; }
void retro_set_controller_port_device(unsigned port, unsigned device) { (void)port; (void)device; }
unsigned retro_api_version(void) { return 1; }
void retro_init(void) {}
void retro_deinit(void) {}
void retro_get_system_info(struct info *i) {
    memset(i, 0, sizeof *i);
    i->name = "crash-sweep stub";
    i->version = "1";
    i->exts = "gb|gbc|gba|nes|fds|sfc|smc|md|gen|bin|sms|gg|sg|pce|ngp|ngc|ws|wsc|lnk|a26|a78|vb|min|n64|z64|v64|nds|cue|chd|iso|pbp|m3u|zip|7z|p8|tic|exe|dsk";
    i->need_fullpath = true;
}
void retro_get_system_av_info(struct av *a) {
    memset(a, 0, sizeof *a);
    a->g.bw = a->g.mw = WIDTH;
    a->g.bh = a->g.mh = HEIGHT;
    a->g.aspect = (float)WIDTH / (float)HEIGHT;
    a->t.fps = FPS;
    a->t.rate = RATE;
}
bool retro_load_game(const struct game *g) {
    (void)g;
    unsigned xrgb8888 = 1;
    return env && env(10, &xrgb8888); /* SET_PIXEL_FORMAT */
}
bool retro_load_game_special(unsigned t, const struct game *g, size_t n) { (void)t; (void)g; (void)n; return false; }
void retro_unload_game(void) {}
unsigned retro_get_region(void) { return FPS < 55 ? 1 : 0; }
void retro_reset(void) { frame = 0; }
void retro_run(void) {
    if (poll) poll();
    int16_t pressed = 0;
    if (input)
        for (unsigned b = 0; b < 16; b++) pressed |= input(0, 1, 0, b) ? (int16_t)(1 << b) : 0;
    frame++;
    if (frame % 60 == 0) sram[(frame / 60) % sizeof sram] = (uint8_t)(frame ^ (uint32_t)pressed); /* the game "saves" */
    unsigned bar = frame % WIDTH;
    for (unsigned y = 0; y < HEIGHT; y++)
        for (unsigned x = 0; x < WIDTH; x++)
            fb[y * WIDTH + x] = x == bar || x == (bar + 1) % WIDTH ? 0xFFFFFF : pressed ? 0x2060C0 : (x * 255 / WIDTH) << 16 | (y * 255 / HEIGHT) << 8;
    if (video) video(fb, WIDTH, HEIGHT, WIDTH * 4);
    /* A frame's sound in one batch, as most cores send it; the count wobbles by one like theirs. */
    size_t n = (size_t)(RATE / FPS) + (frame & 1);
    if (n > 2048) n = 2048;
    for (size_t i = 0; i < n; i++) {
        int16_t s = (int16_t)(3000.0 * sin(phase));
        phase += 2.0 * 3.14159265358979 * 440.0 / RATE;
        if (phase > 2.0 * 3.14159265358979) phase -= 2.0 * 3.14159265358979;
        sound[2 * i] = sound[2 * i + 1] = s;
    }
    if (batch && n) batch(sound, n);
}
size_t retro_serialize_size(void) { return 64 + sizeof sram; }
bool retro_serialize(void *d, size_t s) {
    if (s < retro_serialize_size()) return false;
    memset(d, 0, s);
    memcpy(d, "STUBSTATE", 9);
    memcpy((uint8_t *)d + 16, &frame, sizeof frame);
    memcpy((uint8_t *)d + 64, sram, sizeof sram);
    return true;
}
bool retro_unserialize(const void *d, size_t s) {
    if (s < retro_serialize_size() || memcmp(d, "STUBSTATE", 9) != 0) return false;
    memcpy(&frame, (const uint8_t *)d + 16, sizeof frame);
    memcpy(sram, (const uint8_t *)d + 64, sizeof sram);
    return true;
}
void retro_cheat_reset(void) {}
void retro_cheat_set(unsigned i, bool on, const char *code) { (void)i; (void)on; (void)code; }
#ifdef NO_SRAM
/* A game without a battery save (most NES and Mega Drive games): nothing of its own until a moment is kept, so the
 * Cabinet's TV may hand its run over (src/showcase.rs `may_continue`). */
void *retro_get_memory_data(unsigned id) { (void)id; return NULL; }
size_t retro_get_memory_size(unsigned id) { (void)id; return 0; }
#else
void *retro_get_memory_data(unsigned id) { return id == 0 ? sram : NULL; }
size_t retro_get_memory_size(unsigned id) { return id == 0 ? sizeof sram : 0; }
#endif
