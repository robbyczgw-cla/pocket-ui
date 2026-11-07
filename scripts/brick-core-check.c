#include <stdbool.h>
/* A bare libretro frontend that proves a core loads and runs: dlopen, retro_init, load a ROM, run N frames, print what came
 * out (picture size, frames delivered, a hash of the last picture, how many pixels differ from the first, audio frames).
 *   brick-core-check CORE.so ROM [frames]        exit 0 only when every frame delivered a picture
 * Built for the Brick's glibc 2.33 by scripts/brick-core-check.sh and run with the Brick's own loader and libraries. */
#include <dlfcn.h>
#include <dirent.h>
#include <time.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/resource.h>
#include <sys/stat.h>

#define RETRO_ENVIRONMENT_GET_CAN_DUPE 3
#define RETRO_ENVIRONMENT_GET_SYSTEM_DIRECTORY 9
#define RETRO_ENVIRONMENT_SET_PIXEL_FORMAT 10
#define RETRO_ENVIRONMENT_GET_VARIABLE 15
#define RETRO_ENVIRONMENT_GET_VARIABLE_UPDATE 17
#define RETRO_ENVIRONMENT_GET_LOG_INTERFACE 27
#define RETRO_ENVIRONMENT_GET_SAVE_DIRECTORY 31

struct game_info { const char *path; const void *data; size_t size; const char *meta; };
struct system_av_info { struct { unsigned bw, bh, mw, mh; float ar; } geometry; struct { double fps, sr; } timing; };
struct log_cb { void (*log)(int, const char *, ...); };

struct frame_time_callback { void (*callback)(int64_t); int64_t reference; };
static struct frame_time_callback frame_time;

static unsigned fmt = 0; /* 0 = 0RGB1555, 1 = XRGB8888, 2 = RGB565 */
static unsigned frames, calls, aframes, w, h;
static uint32_t first_hash, last_hash;
static uint64_t differ;
static uint8_t *first_pic;
static size_t pic_bytes;

/* VFS v3: ROM/core/sysroot mounts stay read-only; writes are confined to the disposable
 * container /tmp. DS uses directory enumeration, DOS scans for MIDI data, Stella needs stat. */
struct probe_file { FILE *file; char *path; };
static const char *vpath(struct probe_file *f) { return f->path; }
static struct probe_file *vopen(const char *path, unsigned mode, unsigned hints) {
  (void)hints;
  bool writing = (mode & 2) != 0;
  if (writing && strncmp(path, "/tmp/", 5)) return NULL;
  const char *flags = !writing ? "rb" : (mode & 4) ? "r+b" : (mode & 1) ? "w+b" : "wb";
  FILE *file = fopen(path, flags); if (!file) return NULL;
  struct probe_file *f = malloc(sizeof *f); f->file = file; f->path = strdup(path); return f;
}
static int vclose(struct probe_file *f) { int rc = fclose(f->file); free(f->path); free(f); return rc; }
static int64_t vsize(struct probe_file *f) { struct stat s; return fstat(fileno(f->file), &s) ? -1 : s.st_size; }
static int64_t vtell(struct probe_file *f) { return ftell(f->file); }
static int64_t vseek(struct probe_file *f, int64_t offset, int whence) { return fseek(f->file, offset, whence) ? -1 : ftell(f->file); }
static int64_t vread(struct probe_file *f, void *data, uint64_t count) { return fread(data, 1, count, f->file); }
static int vstat(const char *path, int32_t *size) {
  struct stat s; if (stat(path, &s)) return 0;
  if (size) *size = s.st_size; return 1 | (S_ISDIR(s.st_mode) ? 2 : 0) | (S_ISCHR(s.st_mode) ? 4 : 0);
}
static int64_t vwrite(struct probe_file *f, const void *data, uint64_t count) { return fwrite(data, 1, count, f->file); }
static int vflush(struct probe_file *f) { return fflush(f->file); }
static int vremove(const char *p) { return !strncmp(p, "/tmp/", 5) ? remove(p) : -1; }
static int vrename(const char *a, const char *b) { return !strncmp(a, "/tmp/", 5) && !strncmp(b, "/tmp/", 5) ? rename(a, b) : -1; }
static int64_t vtruncate(struct probe_file *f, int64_t n) { (void)f; (void)n; return -1; }
static int vmkdir(const char *p) { return !strncmp(p, "/tmp/", 5) ? mkdir(p, 0700) : -1; }
struct probe_dir { DIR *dir; struct dirent *entry; char *path; };
static struct probe_dir *vopendir(const char *path, bool hidden) {
  (void)hidden;
  DIR *dir = opendir(path); if (!dir) return NULL;
  struct probe_dir *d = calloc(1, sizeof *d); d->dir = dir; d->path = strdup(path); return d;
}
static bool vreaddir(struct probe_dir *d) { d->entry = readdir(d->dir); return d->entry != NULL; }
static const char *vdir_name(struct probe_dir *d) { return d->entry->d_name; }
static bool vdir_is_dir(struct probe_dir *d) {
  struct stat st; return fstatat(dirfd(d->dir), d->entry->d_name, &st, 0) == 0 && S_ISDIR(st.st_mode);
}
static int vclosedir(struct probe_dir *d) { int rc = closedir(d->dir); free(d->path); free(d); return rc; }
struct probe_vfs {
  const char *(*path)(struct probe_file *);
  struct probe_file *(*open)(const char *, unsigned, unsigned);
  int (*close)(struct probe_file *);
  int64_t (*size)(struct probe_file *), (*tell)(struct probe_file *);
  int64_t (*seek)(struct probe_file *, int64_t, int);
  int64_t (*read)(struct probe_file *, void *, uint64_t);
  int64_t (*write)(struct probe_file *, const void *, uint64_t);
  int (*flush)(struct probe_file *);
  int (*remove)(const char *), (*rename)(const char *, const char *);
  int64_t (*truncate)(struct probe_file *, int64_t);
  int (*stat)(const char *, int32_t *);
  int (*mkdir)(const char *);
  struct probe_dir *(*opendir)(const char *, bool);
  bool (*readdir)(struct probe_dir *);
  const char *(*dirent_name)(struct probe_dir *);
  bool (*dirent_is_dir)(struct probe_dir *);
  int (*closedir)(struct probe_dir *);
};
static struct probe_vfs vfs = { .path=vpath, .open=vopen, .close=vclose, .size=vsize, .tell=vtell, .seek=vseek, .read=vread, .write=vwrite, .flush=vflush, .remove=vremove, .rename=vrename, .truncate=vtruncate, .mkdir=vmkdir, .stat=vstat, .opendir=vopendir, .readdir=vreaddir, .dirent_name=vdir_name, .dirent_is_dir=vdir_is_dir, .closedir=vclosedir };
struct probe_vfs_info { unsigned version; const struct probe_vfs *interface; };

struct variable { const char *key, *value; };
struct option_value { const char *value, *label; };
struct option_v2 { const char *key, *desc, *desc_cat, *info, *info_cat, *category; struct option_value values[128]; const char *def; };
struct options_v2 { const void *categories; const struct option_v2 *definitions; };
static struct variable defaults[1024];
static unsigned ndefaults;
static void set_default(const char *key, const char *value) {
  for (unsigned i = 0; i < ndefaults; i++) if (!strcmp(defaults[i].key, key)) { defaults[i].value = value; return; }
  if (ndefaults < 1024) defaults[ndefaults++] = (struct variable){key, value};
}
static void set_v2(const struct options_v2 *options) {
  for (const struct option_v2 *o = options->definitions; o && o->key; o++) set_default(o->key, o->def ? o->def : o->values[0].value);
}
static void logp(int level, const char *f, ...) { (void)level; va_list args; va_start(args, f); vfprintf(stderr, f, args); va_end(args); }
static bool env(unsigned cmd, void *data) {
  switch (cmd) {
  case 21: frame_time = *(struct frame_time_callback *)data; return true;
  case 52: *(unsigned *)data = 2; return true;
  case 59: *(unsigned *)data = 0; return true;
  case 67: set_v2(data); return true;
  case 68: set_v2(*(const struct options_v2 **)data); return true;
  case 16: {
    for (const struct variable *v = data; v && v->key; v++) {
      const char *start = strchr(v->value, ';'); if (!start) continue;
      start++; while (*start == ' ') start++;
      const char *end = strchr(start, '|');
      set_default(v->key, end ? strndup(start, end - start) : strdup(start));
    }
    return true;
  }
  case RETRO_ENVIRONMENT_GET_VARIABLE: {
    struct variable *v = data;
    for (unsigned i = 0; i < ndefaults; i++) if (!strcmp(defaults[i].key, v->key)) { v->value = defaults[i].value; return true; }
    v->value = NULL; return false;
  }
  case 45 | 0x10000: { struct probe_vfs_info *i = data; if (i->version > 3) return false; i->interface = &vfs; return true; }
  case RETRO_ENVIRONMENT_GET_CAN_DUPE: *(bool *)data = 1; return 1;
  case RETRO_ENVIRONMENT_GET_SYSTEM_DIRECTORY:
  case RETRO_ENVIRONMENT_GET_SAVE_DIRECTORY: *(const char **)data = "/tmp"; return 1;
  case RETRO_ENVIRONMENT_SET_PIXEL_FORMAT: fmt = *(unsigned *)data; return 1;
  case RETRO_ENVIRONMENT_GET_VARIABLE_UPDATE: *(bool *)data = 0; return 1;
  case RETRO_ENVIRONMENT_GET_LOG_INTERFACE: ((struct log_cb *)data)->log = (void (*)(int, const char *, ...))logp; return 1;
  default: return 0;
  }
}
static uint32_t fnv(const uint8_t *p, size_t n) { uint32_t x = 2166136261u; while (n--) x = (x ^ *p++) * 16777619u; return x; }
static void video(const void *d, unsigned width, unsigned height, size_t pitch) {
  calls++;
  if (!d) return; /* a duplicated frame (a core that has nothing new to show, a PlayStation program that draws nothing) */
  size_t bpp = fmt == 1 ? 4 : 2, row = width * bpp;
  uint8_t *flat = malloc(row * height);
  for (unsigned y = 0; y < height; y++) memcpy(flat + y * row, (const uint8_t *)d + y * pitch, row);
  uint32_t hs = fnv(flat, row * height);
  if (!frames) { first_hash = hs; first_pic = flat; pic_bytes = row * height; w = width; h = height; }
  else { if (pic_bytes == row * height) for (size_t i = 0; i < pic_bytes; i++) differ += flat[i] != first_pic[i]; free(flat); }
  last_hash = hs; frames++;
}
static void audio1(int16_t l, int16_t r) { (void)l; (void)r; aframes++; }
static size_t audio(const int16_t *d, size_t n) { (void)d; aframes += n; return n; }
static void poll(void) {}
static int16_t input(unsigned p, unsigned d, unsigned i, unsigned id) { (void)p; (void)d; (void)i; (void)id; return 0; }

int main(int argc, char **argv) {
  if (argc < 3) { fprintf(stderr, "usage: %s CORE.so ROM [frames]\n", argv[0]); return 2; }
  int want = argc > 3 ? atoi(argv[3]) : 120;
  void *so = dlopen(argv[1], RTLD_NOW);
  if (!so) { printf("FAIL dlopen: %s\n", dlerror()); return 1; }
  typedef void (*set_env_t)(bool (*)(unsigned, void *));
  typedef void (*set_video_t)(void (*)(const void *, unsigned, unsigned, size_t));
  typedef void (*set_sample_t)(void (*)(int16_t, int16_t));
  typedef void (*set_batch_t)(size_t (*)(const int16_t *, size_t));
  typedef void (*set_poll_t)(void (*)(void));
  typedef void (*set_input_t)(int16_t (*)(unsigned, unsigned, unsigned, unsigned));
  typedef void (*void_t)(void);
  typedef bool (*load_t)(const struct game_info *);
  typedef void (*av_t)(struct system_av_info *);
#define SYM(t, n) t n = (t)dlsym(so, #n); if (!n) { printf("FAIL missing %s\n", #n); return 1; }
  SYM(set_env_t, retro_set_environment)
  SYM(set_video_t, retro_set_video_refresh)
  SYM(set_sample_t, retro_set_audio_sample)
  SYM(set_batch_t, retro_set_audio_sample_batch)
  SYM(set_poll_t, retro_set_input_poll)
  SYM(set_input_t, retro_set_input_state)
  SYM(void_t, retro_init)
  SYM(load_t, retro_load_game)
  SYM(av_t, retro_get_system_av_info)
  SYM(void_t, retro_run)
  SYM(void_t, retro_unload_game)
  SYM(void_t, retro_deinit)
  retro_set_environment(env);
  retro_set_video_refresh(video); retro_set_audio_sample(audio1); retro_set_audio_sample_batch(audio);
  retro_set_input_poll(poll); retro_set_input_state(input);
  retro_init();
  if (strcmp(argv[2], "--load-only") == 0) {
    retro_deinit();
    if (dlclose(so) != 0) { printf("FAIL dlclose: %s\n", dlerror()); return 1; }
    struct rusage usage; getrusage(RUSAGE_SELF, &usage);
    printf("OK init/deinit/dlclose peak_rss_kib=%ld (no game)\n", usage.ru_maxrss);
    return 0;
  }
  bool no_content = strcmp(argv[2], "--no-content") == 0;
  struct game_info gi = { argv[2], NULL, 0, NULL };
  if (!no_content) {
    FILE *f = fopen(argv[2], "rb");
    if (!f) { printf("FAIL cannot read %s\n", argv[2]); return 1; }
    fseek(f, 0, SEEK_END); long n = ftell(f); fseek(f, 0, SEEK_SET);
    void *rom = malloc(n); if (fread(rom, 1, n, f) != (size_t)n) return 1; fclose(f);
    gi.data = rom; gi.size = (size_t)n;
  }
  if (!retro_load_game(no_content ? NULL : &gi)) { printf("FAIL retro_load_game\n"); return 1; }
  struct system_av_info av; memset(&av, 0, sizeof av); retro_get_system_av_info(&av);
  struct timespec begin, end; clock_gettime(CLOCK_MONOTONIC, &begin);
  for (int i = 0; i < want; i++) { if (frame_time.callback) frame_time.callback(frame_time.reference); retro_run(); }
  clock_gettime(CLOCK_MONOTONIC, &end);
  double elapsed = end.tv_sec - begin.tv_sec + (end.tv_nsec - begin.tv_nsec) / 1e9;
  retro_unload_game(); retro_deinit();
  /* pocket-core drops the library (dlclose) when a game ends: the core's static destructors must survive that */
  if (getenv("BRICK_CHECK_NO_DLCLOSE")) { printf("(dlclose skipped) "); } else if (dlclose(so) != 0) { printf("FAIL dlclose: %s\n", dlerror()); return 1; }
  printf("%s frames=%u/%d (%u with a new picture) picture=%ux%u fmt=%u fps=%.2f rate=%.0f audio_frames=%u first=%08x last=%08x changed_bytes=%llu\n",
         calls == (unsigned)want ? "OK" : "FAIL", calls, want, frames, w, h, fmt, av.timing.fps, av.timing.sr, aframes, first_hash, last_hash,
         (unsigned long long)differ);
  struct rusage usage; getrusage(RUSAGE_SELF, &usage);
  printf("run_ms=%.3f ms_per_call=%.3f peak_rss_kib=%ld (host/container timing, includes video hashing; not A133P speed)\n", elapsed * 1000, elapsed * 1000 / want, usage.ru_maxrss);
  return calls == (unsigned)want ? 0 : 1;
}
