/* Test pattern: the smallest libretro core we could ship through the store, for testing core
 * packages end to end (download, /storage/cores, pocket-core or RetroArch loading it).
 * Draws calm colour bars and a square the D-pad moves; any file counts as a game; plays silence.
 * Our own code, MIT like the rest of pocket-ui. The libretro API definitions below are the few
 * this core needs, written from the public libretro API (libretro.h is MIT as well). */
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>

#define RETRO_API_VERSION 1
#define RETRO_ENVIRONMENT_SET_PIXEL_FORMAT 10
#define RETRO_ENVIRONMENT_SET_SUPPORT_NO_GAME 18
#define RETRO_PIXEL_FORMAT_XRGB8888 1
#define RETRO_DEVICE_JOYPAD 1
#define RETRO_DEVICE_ID_JOYPAD_UP 4
#define RETRO_DEVICE_ID_JOYPAD_DOWN 5
#define RETRO_DEVICE_ID_JOYPAD_LEFT 6
#define RETRO_DEVICE_ID_JOYPAD_RIGHT 7
#define RETRO_REGION_NTSC 0

struct retro_system_info {
  const char *library_name, *library_version, *valid_extensions;
  bool need_fullpath, block_extract;
};
struct retro_game_geometry {
  unsigned base_width, base_height, max_width, max_height;
  float aspect_ratio;
};
struct retro_system_timing {
  double fps, sample_rate;
};
struct retro_system_av_info {
  struct retro_game_geometry geometry;
  struct retro_system_timing timing;
};
struct retro_game_info {
  const char *path;
  const void *data;
  size_t size;
  const char *meta;
};

typedef bool (*retro_environment_t)(unsigned cmd, void *data);
typedef void (*retro_video_refresh_t)(const void *data, unsigned width, unsigned height, size_t pitch);
typedef void (*retro_audio_sample_t)(int16_t left, int16_t right);
typedef size_t (*retro_audio_sample_batch_t)(const int16_t *data, size_t frames);
typedef void (*retro_input_poll_t)(void);
typedef int16_t (*retro_input_state_t)(unsigned port, unsigned device, unsigned index, unsigned id);

#define W 320
#define H 240
#define RATE 44100
#ifndef VERSION
#define VERSION "1.0"
#endif

static retro_environment_t env;
static retro_video_refresh_t video;
static retro_audio_sample_batch_t audio_batch;
static retro_input_poll_t input_poll;
static retro_input_state_t input_state;
static uint32_t frame[W * H];
static int16_t silence[2 * RATE / 60];
static int sx = W / 2 - 12, sy = H / 2 - 12;

void retro_set_environment(retro_environment_t cb) {
  bool yes = true;
  env = cb;
  env(RETRO_ENVIRONMENT_SET_SUPPORT_NO_GAME, &yes);
}
void retro_set_video_refresh(retro_video_refresh_t cb) { video = cb; }
void retro_set_audio_sample(retro_audio_sample_t cb) { (void)cb; }
void retro_set_audio_sample_batch(retro_audio_sample_batch_t cb) { audio_batch = cb; }
void retro_set_input_poll(retro_input_poll_t cb) { input_poll = cb; }
void retro_set_input_state(retro_input_state_t cb) { input_state = cb; }
void retro_init(void) {}
void retro_deinit(void) {}
unsigned retro_api_version(void) { return RETRO_API_VERSION; }

void retro_get_system_info(struct retro_system_info *info) {
  memset(info, 0, sizeof(*info));
  info->library_name = "Test pattern";
  info->library_version = VERSION;
  info->valid_extensions = "tp|txt|bin";
  info->need_fullpath = true;
}

void retro_get_system_av_info(struct retro_system_av_info *info) {
  memset(info, 0, sizeof(*info));
  info->geometry = (struct retro_game_geometry){W, H, W, H, 4.0f / 3.0f};
  info->timing = (struct retro_system_timing){60.0, RATE};
}

void retro_set_controller_port_device(unsigned port, unsigned device) { (void)port, (void)device; }
void retro_reset(void) { sx = W / 2 - 12, sy = H / 2 - 12; }

/* Seven muted bars in the shelf's colours, a darker band below, the square on top. */
static const uint32_t bars[7] = {0xd9d6ce, 0x8a9a3b, 0x2e8c9a, 0x5b4aa6, 0xc8102e, 0x7a6cb0, 0x77746d};

void retro_run(void) {
  input_poll();
  int dx = input_state(0, RETRO_DEVICE_JOYPAD, 0, RETRO_DEVICE_ID_JOYPAD_RIGHT) - input_state(0, RETRO_DEVICE_JOYPAD, 0, RETRO_DEVICE_ID_JOYPAD_LEFT);
  int dy = input_state(0, RETRO_DEVICE_JOYPAD, 0, RETRO_DEVICE_ID_JOYPAD_DOWN) - input_state(0, RETRO_DEVICE_JOYPAD, 0, RETRO_DEVICE_ID_JOYPAD_UP);
  sx = sx + 2 * dx < 0 ? 0 : sx + 2 * dx > W - 24 ? W - 24 : sx + 2 * dx;
  sy = sy + 2 * dy < 0 ? 0 : sy + 2 * dy > H - 24 ? H - 24 : sy + 2 * dy;
  for (int y = 0; y < H; y++)
    for (int x = 0; x < W; x++) {
      uint32_t c = bars[x * 7 / W];
      if (y >= H * 3 / 4) c = (x / 20 + y / 20) % 2 ? 0x1f1f22 : 0x2a2a2e;
      if (x >= sx && x < sx + 24 && y >= sy && y < sy + 24) c = 0xece9e1;
      frame[y * W + x] = c;
    }
  video(frame, W, H, W * sizeof(uint32_t));
  audio_batch(silence, RATE / 60);
}

bool retro_load_game(const struct retro_game_info *game) {
  unsigned fmt = RETRO_PIXEL_FORMAT_XRGB8888;
  (void)game;
  return env(RETRO_ENVIRONMENT_SET_PIXEL_FORMAT, &fmt);
}
bool retro_load_game_special(unsigned type, const struct retro_game_info *info, size_t num) { (void)type, (void)info, (void)num; return false; }
void retro_unload_game(void) {}
unsigned retro_get_region(void) { return RETRO_REGION_NTSC; }
size_t retro_serialize_size(void) { return 2 * sizeof(int); }
bool retro_serialize(void *data, size_t size) {
  if (size < 2 * sizeof(int)) return false;
  memcpy(data, &sx, sizeof(int));
  memcpy((char *)data + sizeof(int), &sy, sizeof(int));
  return true;
}
bool retro_unserialize(const void *data, size_t size) {
  if (size < 2 * sizeof(int)) return false;
  memcpy(&sx, data, sizeof(int));
  memcpy(&sy, (const char *)data + sizeof(int), sizeof(int));
  return true;
}
void retro_cheat_reset(void) {}
void retro_cheat_set(unsigned index, bool enabled, const char *code) { (void)index, (void)enabled, (void)code; }
void *retro_get_memory_data(unsigned id) { (void)id; return NULL; }
size_t retro_get_memory_size(unsigned id) { (void)id; return 0; }
