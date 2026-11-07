/* brick-hw: the one thing the Brick's firmware does with an ioctl that pocket-ui expects as a file.
 *
 *   brick-hw bl-get                 print the panel's brightness (0..255) through /dev/disp
 *   brick-hw bl-set N               set it
 *   brick-hw power-watch [HOLD_S]  the Power key held for HOLD_S seconds (default 6) runs $BRICK_POWER_CMD once (what keymon's "6 seconds
 *                                   power off" was); shorter presses are not ours (pocket-ui reads them: short = sleep, long = power menu).
 *                                   Finds the key's input device by name (axp2202-pek) in $BRICK_INPUT_SYS (/sys/class/input); BRICK_POWER_DEV names a file
 *                                   or FIFO of input events instead (the host test).
 *   brick-hw watch DIR [PERCENT]    DIR/brightness (0..max) is what pocket-ui writes as a backlight; keep the panel in step
 *                                   with it. Creates DIR/max_brightness (255) and DIR/brightness (PERCENT of it, default 70)
 *                                   if missing. Runs until killed. Follows the file with inotify (the panel changes within a few ms of the write; a
 *                                   short Power press used to wait up to half a second for a twice-a-second poll), and still looks every half second
 *                                   in case inotify is not there; a write costs one ioctl.
 *   brick-hw slider-get            print the mute slider's pin (gpio243) as 0 or 1
 *   brick-hw slider-watch          follow the slider: when it reaches the "muted" position ($BRICK_SLIDER_MUTED, 0 or 1, default 1: the slider down) run
 *                                   "$BRICK_SLIDER_CMD mute", on the other position "$BRICK_SLIDER_CMD unmute" (the card's `volume`). Runs the
 *                                   command once at start for the position it finds, then at every change. Polls four times a second.
 *   brick-hw motor-info            print the rumble motor's pin and /sys/class/motor/voltage (read only: how the strength range is found)
 *   brick-hw motor-pulse MS [V]    the motor on for MS milliseconds (at voltage V when given), then off
 *   brick-hw led-info              print the led_anim driver's files (read only)
 *   brick-hw led-set RRGGBB [PCT]  every zone (lr, m, f1, f2) static in that colour at PCT percent (default 50)
 *   brick-hw led-off               the lights dark
 *   All sysfs paths below $BRICK_SYS (default none): the host test points it at a fake tree. The sysfs files are the Brick's own kernel
 *   interface, read from the device and Knulli's same-driver script for the value ranges (0..60), none of NextUI's code.
 *
 * The ioctl numbers are those of Allwinner's sunxi display driver (DISP_LCD_SET_BRIGHTNESS 0x102, DISP_LCD_GET_BRIGHTNESS
 * 0x103, arguments {screen, value, 0, 0}); `bl-get` is how a device is checked without changing anything. Plain C, glibc only. */
#include <dirent.h>
#include <fcntl.h>
#include <linux/input.h>
#include <poll.h>
#include <sys/inotify.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>

#define DISP_LCD_SET_BRIGHTNESS 0x102
#define DISP_LCD_GET_BRIGHTNESS 0x103
#define MAXB 255

static const char *disp_path(void) { const char *p = getenv("BRICK_DISP"); return p && *p ? p : "/dev/disp"; }

static int bl_get(void) {
  int fd = open(disp_path(), O_RDWR);
  if (fd < 0) return -1;
  unsigned long a[4] = {0, 0, 0, 0};
  int r = ioctl(fd, DISP_LCD_GET_BRIGHTNESS, a);
  close(fd);
  return r < 0 ? -1 : r; /* the driver returns the value */
}

static int bl_set(int v) {
  int fd = open(disp_path(), O_RDWR);
  if (fd < 0) return -1;
  unsigned long a[4] = {0, (unsigned long)v, 0, 0};
  int r = ioctl(fd, DISP_LCD_SET_BRIGHTNESS, a);
  close(fd);
  return r;
}

static int read_int(const char *path) {
  FILE *f = fopen(path, "r");
  if (!f) return -1;
  int v = -1;
  if (fscanf(f, "%d", &v) != 1) v = -1;
  fclose(f);
  return v;
}

static void write_int(const char *path, int v) {
  FILE *f = fopen(path, "w");
  if (!f) return;
  fprintf(f, "%d\n", v);
  fclose(f);
}

/* The event node whose name is "axp2202-pek", from sysfs (the same names /proc/bus/input/devices lists). */
static int find_power_dev(char *out, size_t n) {
  const char *sys = getenv("BRICK_INPUT_SYS");
  if (!sys || !*sys) sys = "/sys/class/input";
  DIR *d = opendir(sys);
  if (!d) return -1;
  struct dirent *e;
  int found = -1;
  while ((e = readdir(d))) {
    if (strncmp(e->d_name, "event", 5)) continue;
    char p[600], name[128] = "";
    snprintf(p, sizeof p, "%s/%s/device/name", sys, e->d_name);
    FILE *f = fopen(p, "r");
    if (!f) continue;
    if (fgets(name, sizeof name, f)) name[strcspn(name, "\n")] = 0;
    fclose(f);
    if (!strcmp(name, "axp2202-pek")) { snprintf(out, n, "/dev/input/%s", e->d_name); found = 0; break; }
  }
  closedir(d);
  return found;
}

static int power_watch(int hold_s) {
  char path[512];
  const char *dev = getenv("BRICK_POWER_DEV");
  const char *cmd = getenv("BRICK_POWER_CMD");
  if (!cmd || !*cmd) { fprintf(stderr, "brick-hw: BRICK_POWER_CMD is not set\n"); return 2; }
  if (dev && *dev) snprintf(path, sizeof path, "%s", dev);
  else if (find_power_dev(path, sizeof path) < 0) { fprintf(stderr, "brick-hw: no input device named axp2202-pek\n"); return 1; }
  int fd = open(path, O_RDONLY);
  if (fd < 0) { perror(path); return 1; }
  int down = 0, fired = 0;
  struct timespec t0 = {0, 0};
  for (;;) {
    int wait_ms = -1;
    if (down && !fired) {
      struct timespec now; clock_gettime(CLOCK_MONOTONIC, &now);
      long held = (now.tv_sec - t0.tv_sec) * 1000 + (now.tv_nsec - t0.tv_nsec) / 1000000;
      wait_ms = hold_s * 1000 - (int)held;
      if (wait_ms <= 0) { fired = 1; if (system(cmd) != 0) fprintf(stderr, "brick-hw: %s failed\n", cmd); continue; }
    }
    struct pollfd p = {fd, POLLIN, 0};
    int r = poll(&p, 1, wait_ms);
    if (r <= 0) continue;
    struct input_event ev;
    ssize_t n = read(fd, &ev, sizeof ev);
    if (n == 0) { usleep(100000); continue; }       /* a FIFO whose writer went away: wait for the next one */
    if (n != (ssize_t)sizeof ev) continue;
    if (ev.type == EV_KEY && ev.code == KEY_POWER) {
      if (ev.value == 1) { down = 1; fired = 0; clock_gettime(CLOCK_MONOTONIC, &t0); }
      else if (ev.value == 0) down = 0;
    }
  }
}

/* ---- the Brick's own hardware: motor, lights, mute slider (docs/TRIMUI-BRICK-HW.md) ------------------------------------------------ */
static const char *sys_prefix(void) { const char *p = getenv("BRICK_SYS"); return p ? p : ""; }
static void sysp(char *out, size_t n, const char *rel) { snprintf(out, n, "%s/%s", sys_prefix(), rel); }
static int sys_read(const char *rel) { char p[700]; sysp(p, sizeof p, rel); return read_int(p); }
static int sys_exists(const char *rel) { char p[700]; sysp(p, sizeof p, rel); return access(p, F_OK) == 0; }
static void sys_put(const char *rel, const char *v) {
  char p[700]; sysp(p, sizeof p, rel);
  FILE *f = fopen(p, "w");
  if (!f) return;
  fprintf(f, "%s\n", v);
  fclose(f);
}

static void pin_setup(int pin, const char *dir) {
  char rel[96], v[16];
  snprintf(rel, sizeof rel, "sys/class/gpio/gpio%d/value", pin);
  if (!sys_exists(rel)) { snprintf(v, sizeof v, "%d", pin); sys_put("sys/class/gpio/export", v); }
  snprintf(rel, sizeof rel, "sys/class/gpio/gpio%d/direction", pin);
  if (sys_exists(rel)) sys_put(rel, dir);
}

static int slider_value(void) { return sys_read("sys/class/gpio/gpio243/value"); }

/* Settings > Mute slider: "nothing" in the file BRICK_SLIDER_CFG (<data>/brick-slider) leaves the slider without effect (and un-mutes at
 * once); anything else, or no file, is the default: the slider mutes. */
static int slider_does_nothing(void) {
  const char *f = getenv("BRICK_SLIDER_CFG");
  char buf[16] = {0};
  if (!f || !*f) return 0;
  FILE *fp = fopen(f, "r");
  if (!fp) return 0;
  if (!fgets(buf, sizeof buf, fp)) buf[0] = 0;
  fclose(fp);
  return strncmp(buf, "nothing", 7) == 0;
}

static int slider_watch(void) {
  const char *cmd = getenv("BRICK_SLIDER_CMD");
  if (!cmd || !*cmd) { fprintf(stderr, "brick-hw: BRICK_SLIDER_CMD is not set\n"); return 2; }
  const char *m = getenv("BRICK_SLIDER_MUTED");
  int muted_at = m && *m == '0' ? 0 : 1;   /* the slider down (1) is muted on the real Brick (03.11., its owner) */
  pin_setup(243, "in");
  int last = -2;
  for (;;) {
    int v = slider_value();
    /* The state the speaker should have: the slider's, unless the player turned the slider off (2 = unmute for that). */
    int want = v < 0 ? -1 : slider_does_nothing() ? 0 : (v == muted_at ? 1 : 0);
    if (want >= 0 && want != last) {
      char line[700];
      snprintf(line, sizeof line, "%s %s", cmd, want ? "mute" : "unmute");
      fprintf(stderr, "brick-hw: slider %d: %s\n", v, line);
      if (system(line) != 0) fprintf(stderr, "brick-hw: %s failed\n", line);
      last = want;
    }
    usleep(250000);
  }
}

static void motor_pulse(int ms, const char *volt) {
  pin_setup(227, "out");
  if (volt && *volt && sys_exists("sys/class/motor/voltage")) sys_put("sys/class/motor/voltage", volt);
  sys_put("sys/class/gpio/gpio227/value", "1");
  usleep((useconds_t)ms * 1000);
  sys_put("sys/class/gpio/gpio227/value", "0");
}

static const char *ZONES[] = {"lr", "m", "f1", "f2"};

static void led_apply(const char *hex, int mode, int scale) {
  char rel[96], v[16];
  sys_put("sys/class/led_anim/effect_enable", "0");
  for (int i = 0; i < 4; i++) {
    snprintf(rel, sizeof rel, "sys/class/led_anim/effect_rgb_hex_%s", ZONES[i]);
    if (sys_exists(rel)) sys_put(rel, hex);
    snprintf(rel, sizeof rel, "sys/class/led_anim/effect_cycles_%s", ZONES[i]);
    if (sys_exists(rel)) sys_put(rel, "-1");
    snprintf(rel, sizeof rel, "sys/class/led_anim/effect_%s", ZONES[i]);
    snprintf(v, sizeof v, "%d", mode);
    if (sys_exists(rel)) sys_put(rel, v);
  }
  snprintf(v, sizeof v, "%d", scale);
  sys_put("sys/class/led_anim/max_scale", v);
  sys_put("sys/class/led_anim/effect_enable", "1");
}

static void print_file(const char *rel) {
  char p[700], buf[256] = "";
  sysp(p, sizeof p, rel);
  FILE *f = fopen(p, "r");
  if (!f) { printf("%s: (not there)\n", rel); return; }
  if (fgets(buf, sizeof buf, f)) buf[strcspn(buf, "\n")] = 0;
  fclose(f);
  printf("%s: %s\n", rel, buf);
}

static void list_dir(const char *rel) {
  char p[700];
  sysp(p, sizeof p, rel);
  DIR *d = opendir(p);
  if (!d) { printf("%s: (not there)\n", rel); return; }
  struct dirent *e;
  while ((e = readdir(d))) if (e->d_name[0] != '.') printf("%s/%s\n", rel, e->d_name);
  closedir(d);
}

static int hw_command(int argc, char **argv) {
  const char *c = argv[1];
  if (!strcmp(c, "slider-get")) {
    int v = slider_value();
    if (v < 0) { fprintf(stderr, "brick-hw: cannot read gpio243\n"); return 1; }
    printf("%d\n", v);
    return 0;
  }
  if (!strcmp(c, "slider-watch")) return slider_watch();
  if (!strcmp(c, "motor-info")) {
    print_file("sys/class/gpio/gpio227/value");
    print_file("sys/class/gpio/gpio227/direction");
    list_dir("sys/class/motor");
    print_file("sys/class/motor/voltage");
    return 0;
  }
  if (!strcmp(c, "motor-pulse") && argc >= 3) {
    int ms = atoi(argv[2]);
    if (ms < 1) ms = 1;
    if (ms > 2000) ms = 2000;
    motor_pulse(ms, argc > 3 ? argv[3] : NULL);
    return 0;
  }
  if (!strcmp(c, "led-info")) {
    list_dir("sys/class/led_anim");
    print_file("sys/class/led_anim/max_scale");
    print_file("sys/class/led_anim/effect_enable");
    for (int i = 0; i < 4; i++) {
      char rel[96];
      snprintf(rel, sizeof rel, "sys/class/led_anim/effect_%s", ZONES[i]);
      print_file(rel);
      snprintf(rel, sizeof rel, "sys/class/led_anim/effect_rgb_hex_%s", ZONES[i]);
      print_file(rel);
    }
    return 0;
  }
  if (!strcmp(c, "led-set") && argc >= 3) {
    /* RRGGBB in 0..255 per channel, on the driver's 0..60 scale */
    unsigned long rgb = strtoul(argv[2], NULL, 16);
    int pct = argc > 3 ? atoi(argv[3]) : 50;
    if (pct < 0) pct = 0;
    if (pct > 100) pct = 100;
    char hex[16];
    snprintf(hex, sizeof hex, "%02x%02x%02x", (int)(((rgb >> 16) & 255) * 60 + 127) / 255, (int)(((rgb >> 8) & 255) * 60 + 127) / 255, (int)((rgb & 255) * 60 + 127) / 255);
    led_apply(hex, 4, pct * 60 / 100);
    return 0;
  }
  if (!strcmp(c, "led-off")) {
    led_apply("000000", 0, 0);
    sys_put("sys/class/led_anim/effect_enable", "0");
    /* the driver's raw picture glows on past the effects (03.11.): all 23 LEDs black */
    if (sys_exists("sys/class/led_anim/anim_frames_enable")) sys_put("sys/class/led_anim/anim_frames_enable", "0");
    if (sys_exists("sys/class/led_anim/frame_hex")) {
      char black[23 * 7 + 1] = "";
      for (int i = 0; i < 23; i++) strcat(black, "000000 ");
      sys_put("sys/class/led_anim/frame_hex", black);
    }
    return 0;
  }
  return -1;
}

int main(int argc, char **argv) {
  if (argc >= 2) { int r = hw_command(argc, argv); if (r >= 0) return r; }
  if (argc >= 2 && !strcmp(argv[1], "bl-get")) {
    int v = bl_get();
    if (v < 0) { fprintf(stderr, "brick-hw: cannot read the brightness through %s\n", disp_path()); return 1; }
    printf("%d\n", v);
    return 0;
  }
  if (argc >= 3 && !strcmp(argv[1], "bl-set")) {
    int v = atoi(argv[2]);
    if (v < 0) v = 0;
    if (v > MAXB) v = MAXB;
    return bl_set(v) < 0 ? 1 : 0;
  }
  if (argc >= 3 && !strcmp(argv[1], "watch")) {
    char dir[512], br[560], mx[560];
    snprintf(dir, sizeof dir, "%s", argv[2]);
    snprintf(br, sizeof br, "%s/brightness", dir);
    snprintf(mx, sizeof mx, "%s/max_brightness", dir);
    mkdir(dir, 0755);
    write_int(mx, MAXB);
    int pct = argc > 3 ? atoi(argv[3]) : 70;
    if (read_int(br) < 0) write_int(br, MAXB * pct / 100);
    int last = -1;
    /* A write by pocket-ui is a truncate and a write; the close is when the whole value is there. */
    int ino = inotify_init1(IN_NONBLOCK);
    if (ino >= 0 && inotify_add_watch(ino, dir, IN_CLOSE_WRITE | IN_MOVED_TO) < 0) { close(ino); ino = -1; }
    for (;;) {
      int v = read_int(br);
      if (v >= 0 && v != last) {
        if (v > MAXB) v = MAXB;
        if (bl_set(v) == 0) last = v; /* a failed ioctl is tried again on the next round */
      }
      if (ino >= 0) {
        struct pollfd pf = {ino, POLLIN, 0};
        if (poll(&pf, 1, 500) > 0) {
          char buf[4096];
          while (read(ino, buf, sizeof buf) > 0) {} /* the events only say "look again" */
        }
      } else {
        usleep(500000);
      }
    }
  }
  if (argc >= 2 && !strcmp(argv[1], "power-watch")) return power_watch(argc > 2 ? atoi(argv[2]) : 6);
  fprintf(stderr, "usage: brick-hw bl-get | bl-set N | watch DIR [PERCENT] | power-watch [HOLD_S] | slider-get | slider-watch | motor-info | motor-pulse MS [V] | led-info | led-set RRGGBB [PCT] | led-off\n");
  return 2;
}
