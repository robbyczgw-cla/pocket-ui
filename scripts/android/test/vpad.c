// A simulated gamepad for Android test devices: makes a uinput device (like a Bluetooth or USB pad would appear), then
// reads commands on stdin, one per line:
//   key <linux code> <0|1>   button (0x130 BTN_SOUTH ... 0x13e BTN_THUMBR, decimal or 0x..)
//   abs <linux code> <value> axis (0 X, 1 Y, 2 Z, 5 RZ, 9 GAS, 10 BRAKE, 16 HAT0X, 17 HAT0Y)
//   sleep <ms>
//   quit                     (the device goes away when the program ends)
// usage: vpad <name> <vendor> <product> [bus: 3 usb, 5 bluetooth]
// Build static for aarch64 (no NDK needed): zig cc -target aarch64-linux-musl -O2 -static vpad.c -o vpad
// Needs write access to /dev/uinput (the adb shell has it on the Odin 2 Mini). Never touches system settings.
#include <fcntl.h>
#include <linux/uinput.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static int fd;
static void emit(int type, int code, int value) {
    struct input_event e;
    memset(&e, 0, sizeof e);
    e.type = type; e.code = code; e.value = value;
    if (write(fd, &e, sizeof e) < 0) perror("write");
}
static void absaxis(int code, int min, int max) {
    struct uinput_abs_setup s;
    memset(&s, 0, sizeof s);
    s.code = code; s.absinfo.minimum = min; s.absinfo.maximum = max; s.absinfo.flat = code <= 5 ? 15 : 0;
    ioctl(fd, UI_ABS_SETUP, &s);
}
int main(int argc, char **argv) {
    if (argc < 4) { fprintf(stderr, "usage: vpad name vendor product [bus]\n"); return 2; }
    fd = open("/dev/uinput", O_WRONLY | O_NONBLOCK);
    if (fd < 0) { perror("/dev/uinput"); return 1; }
    ioctl(fd, UI_SET_EVBIT, EV_KEY);
    ioctl(fd, UI_SET_EVBIT, EV_ABS);
    for (int c = 0x130; c <= 0x13e; c++) ioctl(fd, UI_SET_KEYBIT, c);
    int axes[] = { 0, 1, 2, 5, 9, 10, 16, 17 };
    for (unsigned i = 0; i < sizeof axes / sizeof *axes; i++) {
        ioctl(fd, UI_SET_ABSBIT, axes[i]);
        int a = axes[i];
        absaxis(a, a == 16 || a == 17 ? -1 : (a == 9 || a == 10 ? 0 : -32767), a == 16 || a == 17 ? 1 : 32767);
    }
    struct uinput_setup setup;
    memset(&setup, 0, sizeof setup);
    setup.id.bustype = argc > 4 ? atoi(argv[4]) : 3;
    setup.id.vendor = strtol(argv[2], 0, 0);
    setup.id.product = strtol(argv[3], 0, 0);
    snprintf(setup.name, sizeof setup.name, "%s", argv[1]);
    if (ioctl(fd, UI_DEV_SETUP, &setup) < 0 || ioctl(fd, UI_DEV_CREATE) < 0) { perror("create"); return 1; }
    fprintf(stderr, "vpad: %s up\n", argv[1]);
    char line[128];
    while (fgets(line, sizeof line, stdin)) {
        char cmd[16]; long a = 0, b = 0;
        int n = sscanf(line, "%15s %li %li", cmd, &a, &b);
        if (n >= 1 && !strcmp(cmd, "quit")) break;
        if (n >= 3 && !strcmp(cmd, "key")) { emit(EV_KEY, a, b); emit(EV_SYN, SYN_REPORT, 0); }
        else if (n >= 3 && !strcmp(cmd, "abs")) { emit(EV_ABS, a, b); emit(EV_SYN, SYN_REPORT, 0); }
        else if (n >= 2 && !strcmp(cmd, "sleep")) usleep(a * 1000);
        fflush(stdout);
    }
    ioctl(fd, UI_DEV_DESTROY);
    close(fd);
    fprintf(stderr, "vpad: gone\n");
    return 0;
}
