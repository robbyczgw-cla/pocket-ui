/* brick-hw: the one thing the Brick's firmware does with an ioctl that pocket-ui expects as a file.
 *
 *   brick-hw bl-get                 print the panel's brightness (0..255) through /dev/disp
 *   brick-hw bl-set N               set it
 *   brick-hw watch DIR [PERCENT]    DIR/brightness (0..max) is what pocket-ui writes as a backlight; keep the panel in step
 *                                   with it. Creates DIR/max_brightness (255) and DIR/brightness (PERCENT of it, default 70)
 *                                   if missing. Runs until killed. Polls twice a second; a write costs one ioctl.
 *
 * The ioctl numbers are those of Allwinner's sunxi display driver (DISP_LCD_SET_BRIGHTNESS 0x102, DISP_LCD_GET_BRIGHTNESS
 * 0x103, arguments {screen, value, 0, 0}); `bl-get` is how a device is checked without changing anything. Plain C, glibc only. */
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/stat.h>
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

int main(int argc, char **argv) {
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
    for (;;) {
      int v = read_int(br);
      if (v >= 0 && v != last) {
        if (v > MAXB) v = MAXB;
        if (bl_set(v) == 0) last = v; /* a failed ioctl is tried again on the next round */
      }
      usleep(500000);
    }
  }
  fprintf(stderr, "usage: brick-hw bl-get | bl-set N | watch DIR [PERCENT]\n");
  return 2;
}
