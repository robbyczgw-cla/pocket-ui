/* Headless cheat-effect probe; reuse the smoke frontend's VFS/options, never its
 * video hashing. Build: cc -O1 scripts/brick-cheat-check.c -ldl -o cheat-check
 * Run under an external timeout, in a disposable directory: cores may create
 * files beside their content even when a save directory is supplied.
 *
 * cheat-check CORE.so ROM CODE RAM_OFFSET EXPECTED [WIDTH [FRAMES [WARMUP]]]
 * Numbers accept 0x prefixes. WIDTH is 1, 2 or 4, little endian; defaults 1/3/120.
 * BRICK_CHEAT_SYSTEM_DIR supplies BIOS files; BRICK_CHEAT_SAVE_DIR defaults /tmp.
 * BRICK_CHEAT_MEMORY_ID defaults 2 (RETRO_MEMORY_SYSTEM_RAM).
 * BRICK_CHEAT_MAP_START selects an exact SET_MEMORY_MAPS RAM region instead
 * (mGBA reports an incorrect generic GBA RAM size; use 0x02000000 for EWRAM).
 * Uses in-memory states; never reads/writes the player's saves or states.
 */
#define main brick_smoke_main
#include "brick-core-check.c"
#undef main
#include <errno.h>
#include <limits.h>
#include <unistd.h>

static unsigned pictures;
static unsigned memory_id = 2, width = 1, step_frames = 3;
static size_t offset;
struct cheat_memory_descriptor {
  uint64_t flags;
  void *ptr;
  size_t offset, start, select, disconnect, len;
  const char *addrspace;
};
struct cheat_memory_map { const struct cheat_memory_descriptor *descriptors; unsigned count; };
static struct cheat_memory_descriptor maps[128];
static unsigned map_count;
static bool use_map;
static size_t map_start;
static uint32_t expected, sentinel;
static void (*run_frame)(void);
static void *(*memory_data)(unsigned);
static size_t (*memory_size)(unsigned);

static void discard_video(const void *data, unsigned width_, unsigned height, size_t pitch) {
  (void)data; (void)width_; (void)height; (void)pitch; pictures++;
}
static bool cheat_env(unsigned command, void *data) {
  if (command == (36 | 0x10000)) { /* RETRO_ENVIRONMENT_SET_MEMORY_MAPS */
    const struct cheat_memory_map *map = data;
    if (map->count > 128) return false;
    memcpy(maps, map->descriptors, map->count * sizeof maps[0]);
    map_count = map->count;
    return true;
  }
  if (command == RETRO_ENVIRONMENT_GET_SYSTEM_DIRECTORY || command == RETRO_ENVIRONMENT_GET_SAVE_DIRECTORY) {
    const char *value = getenv(command == RETRO_ENVIRONMENT_GET_SYSTEM_DIRECTORY ? "BRICK_CHEAT_SYSTEM_DIR" : "BRICK_CHEAT_SAVE_DIR");
    *(const char **)data = value ? value : "/tmp";
    return true;
  }
  return env(command, data);
}
static unsigned long long number(const char *text_, unsigned long long limit) {
  char *end;
  errno = 0;
  unsigned long long value = strtoull(text_, &end, 0);
  if (errno || end == text_ || *end || *text_ == '-' || value > limit) {
    fprintf(stderr, "FAIL invalid number: %s\n", text_); exit(2);
  }
  return value;
}
/* Re-query every access: unserialize may replace the core's memory allocation. */
static uint8_t *ram(void) {
  if (use_map) {
    for (unsigned i = 0; i < map_count; i++) {
      const struct cheat_memory_descriptor *map = &maps[i];
      if (map->start == map_start && map->ptr && (map->flags & 4) && !(map->flags & 1) && !map->disconnect &&
          offset <= map->len && width <= map->len - offset && map->offset <= SIZE_MAX - offset) {
        return (uint8_t *)map->ptr + map->offset + offset;
      }
    }
    fprintf(stderr, "FAIL no writable linear RAM map start=0x%zx offset=0x%zx width=%u maps=%u\n", map_start, offset, width, map_count);
    exit(1);
  }
  size_t size = memory_size(memory_id);
  uint8_t *data = memory_data(memory_id);
  if (!data || offset > size || width > size - offset) {
    fprintf(stderr, "FAIL memory id=%u size=%zu offset=%zu width=%u\n", memory_id, size, offset, width); exit(1);
  }
  return data + offset;
}
static uint32_t read_value(void) {
  uint8_t *data = ram(); uint32_t value = 0;
  for (unsigned i = 0; i < width; i++) value |= (uint32_t)data[i] << (i * 8);
  return value;
}
static void write_value(uint32_t value) {
  uint8_t *data = ram();
  for (unsigned i = 0; i < width; i++) data[i] = (uint8_t)(value >> (i * 8));
}
static void run_frames(unsigned count) {
  for (unsigned i = 0; i < count; i++) {
    if (frame_time.callback) frame_time.callback(frame_time.reference);
    run_frame();
  }
}
static bool check(const char *phase, bool active) {
  uint32_t actual = read_value();
  bool pass = active ? actual == expected : actual != expected;
  printf("%s phase=%s memory_id=%u offset=0x%zx width=%u value=0x%08x expected=0x%08x sentinel=0x%08x\n",
         pass ? "OK" : "FAIL", phase, memory_id, offset, width, actual, expected, sentinel);
  return pass;
}

int main(int argc, char **argv) {
  if (argc < 6 || argc > 9 || !strcmp(argv[1], "--help")) {
    fprintf(stderr, "usage: %s CORE.so ROM CODE RAM_OFFSET EXPECTED [WIDTH [FRAMES [WARMUP]]]\n", argv[0]); return 2;
  }
  offset = number(argv[4], SIZE_MAX);
  expected = number(argv[5], UINT32_MAX);
  if (argc > 6) width = number(argv[6], 4);
  if (width != 1 && width != 2 && width != 4) { fprintf(stderr, "FAIL width must be 1, 2 or 4\n"); return 2; }
  uint32_t mask = width == 4 ? UINT32_MAX : (1u << (width * 8)) - 1;
  if (expected > mask) { fprintf(stderr, "FAIL expected value exceeds width\n"); return 2; }
  sentinel = expected ^ (0x5a5a5a5a & mask);
  if (argc > 7) step_frames = number(argv[7], 100000);
  unsigned warmup = argc > 8 ? number(argv[8], 100000) : 120;
  if (!step_frames) { fprintf(stderr, "FAIL frames must be positive\n"); return 2; }
  if (getenv("BRICK_CHEAT_MEMORY_ID")) memory_id = number(getenv("BRICK_CHEAT_MEMORY_ID"), UINT_MAX);
  if (getenv("BRICK_CHEAT_MAP_START")) { use_map = true; map_start = number(getenv("BRICK_CHEAT_MAP_START"), SIZE_MAX); }
  /* Known SNES/gpSP parsers have fixed buffers. Refuse oversize input rather
   * than letting the diagnostic itself overflow a core. */
  if (strlen(argv[3]) >= 256) { fprintf(stderr, "FAIL code must be shorter than 256 bytes\n"); return 2; }
  setvbuf(stdout, NULL, _IOLBF, 0);
  /* The stock firmware has no timeout applet; a hung core must still end. */
  alarm(30);
  void *so = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
  if (!so) { fprintf(stderr, "FAIL dlopen: %s\n", dlerror()); return 1; }
  typedef void (*set_env_t)(bool (*)(unsigned, void *));
  typedef void (*set_video_t)(void (*)(const void *, unsigned, unsigned, size_t));
  typedef void (*set_sample_t)(void (*)(int16_t, int16_t));
  typedef void (*set_batch_t)(size_t (*)(const int16_t *, size_t));
  typedef void (*set_poll_t)(void (*)(void));
  typedef void (*set_input_t)(int16_t (*)(unsigned, unsigned, unsigned, unsigned));
  typedef void (*void_t)(void);
  typedef bool (*load_t)(const struct game_info *);
  typedef void (*cheat_t)(unsigned, bool, const char *);
  typedef void *(*memory_t)(unsigned);
  typedef size_t (*memory_size_t)(unsigned);
  typedef size_t (*state_size_t)(void);
  typedef bool (*serialize_t)(void *, size_t);
  typedef bool (*unserialize_t)(const void *, size_t);
  SYM(set_env_t, retro_set_environment)
  SYM(set_video_t, retro_set_video_refresh)
  SYM(set_sample_t, retro_set_audio_sample)
  SYM(set_batch_t, retro_set_audio_sample_batch)
  SYM(set_poll_t, retro_set_input_poll)
  SYM(set_input_t, retro_set_input_state)
  SYM(void_t, retro_init)
  SYM(load_t, retro_load_game)
  SYM(void_t, retro_run)
  SYM(void_t, retro_unload_game)
  SYM(void_t, retro_deinit)
  SYM(void_t, retro_cheat_reset)
  SYM(cheat_t, retro_cheat_set)
  SYM(memory_t, retro_get_memory_data)
  SYM(memory_size_t, retro_get_memory_size)
  SYM(state_size_t, retro_serialize_size)
  SYM(serialize_t, retro_serialize)
  SYM(unserialize_t, retro_unserialize)
  run_frame = retro_run; memory_data = retro_get_memory_data; memory_size = retro_get_memory_size;
  retro_set_environment(cheat_env);
  retro_set_video_refresh(discard_video);
  retro_set_audio_sample(audio1); retro_set_audio_sample_batch(audio);
  retro_set_input_poll(poll); retro_set_input_state(input);
  retro_init();
  FILE *file = fopen(argv[2], "rb");
  if (!file) { perror("FAIL open ROM"); return 1; }
  if (fseek(file, 0, SEEK_END)) { perror("FAIL seek ROM"); return 1; }
  long length = ftell(file);
  if (length <= 0 || fseek(file, 0, SEEK_SET)) { fprintf(stderr, "FAIL ROM size\n"); return 1; }
  void *rom = malloc((size_t)length);
  if (!rom || fread(rom, 1, (size_t)length, file) != (size_t)length) { fprintf(stderr, "FAIL read ROM\n"); return 1; }
  fclose(file);
  struct game_info game = {argv[2], rom, (size_t)length, NULL};
  if (!retro_load_game(&game)) { fprintf(stderr, "FAIL retro_load_game\n"); return 1; }
  run_frames(warmup);
  printf("INFO warmup=%u pictures=%u ram_size=%zu mapped=%s map_start=0x%zx initial=0x%08x\n",
         warmup, pictures, memory_size(memory_id), use_map ? "yes" : "no", map_start, read_value());
  bool pass = true;
  retro_cheat_reset(); write_value(sentinel); run_frames(step_frames);
  /* A naturally equal game value cannot prove the effect of a cheat. */
  pass &= check("baseline-cheats-off", false);
  retro_cheat_set(0, true, argv[3]); write_value(sentinel); run_frames(step_frames);
  pass &= check("cheat-on", true);
  run_frames(step_frames); pass &= check("continue", true);
  size_t state_size = retro_serialize_size();
  void *state = state_size ? malloc(state_size) : NULL;
  bool saved = state && retro_serialize(state, state_size);
  if (!saved) { fprintf(stderr, "FAIL serialize size=%zu\n", state_size); pass = false; }
  else {
    write_value(sentinel);
    if (!retro_unserialize(state, state_size)) { fprintf(stderr, "FAIL unserialize\n"); pass = false; }
    else {
      pass &= check("state-restored-ram", true);
      /* Do not reapply: detect whether loading a state loses active cheats. */
      write_value(sentinel); run_frames(step_frames); pass &= check("state-resume-cheat-on", true);
    }
  }
  retro_cheat_set(0, false, argv[3]); write_value(sentinel); run_frames(step_frames);
  printf("INFO direct-disable-still-writes=%s value=0x%08x (diagnostic; frontend uses reset/reapply)\n",
         read_value() == expected ? "yes" : "no", read_value());
  retro_cheat_reset(); write_value(sentinel); run_frames(step_frames);
  pass &= check("cheat-off-reset-reapply", false);
  if (saved) {
    if (!retro_unserialize(state, state_size)) { fprintf(stderr, "FAIL off-state-unserialize\n"); pass = false; }
    else {
      /* Do not reset after loading: detect cheat lists embedded in the state. */
      write_value(sentinel); run_frames(step_frames);
      pass &= check("state-resume-cheat-off", false);
    }
  }
  retro_cheat_reset(); retro_unload_game(); retro_deinit();
  free(state); free(rom);
  if (dlclose(so)) { fprintf(stderr, "FAIL dlclose: %s\n", dlerror()); pass = false; }
  printf("%s cheat-effect on/continue/state/off pictures=%u\n", pass ? "PASS" : "FAIL", pictures);
  return pass ? 0 : 1;
}
