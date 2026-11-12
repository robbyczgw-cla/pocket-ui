/* Private session dead-man: real input restores the owner even while Wi-Fi is down.
 * No grabs, writes to input nodes, or device-specific control writes. */
#include <linux/input.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <poll.h>
#include <fcntl.h>
#include <unistd.h>
#include <time.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <errno.h>

static int pressed(const struct input_event *e) {
    return (e->type == EV_KEY && e->value == 1) || (e->type == EV_ABS && e->value != 0);
}

int main(int argc, char **argv) {
    if (argc != 3 || strncmp(argv[1], "/tmp/brick-journey-", 19)) return 2;
    char stop[512], abort_file[512], restore[512];
    if (snprintf(stop, sizeof stop, "%s/stop", argv[1]) >= (int)sizeof stop ||
        snprintf(abort_file, sizeof abort_file, "%s/owner-abort", argv[1]) >= (int)sizeof abort_file ||
        snprintf(restore, sizeof restore, "%s/restore.sh", argv[1]) >= (int)sizeof restore) return 2;
    int lifetime = atoi(argv[2]);
    if (lifetime < 1 || lifetime > 540) return 2;
    struct pollfd fds[3];
#ifndef BRICK_SESSION_INPUT_ROOT
#define BRICK_SESSION_INPUT_ROOT "/dev/input"
#endif
    const char *nodes[] = {BRICK_SESSION_INPUT_ROOT "/event0", BRICK_SESSION_INPUT_ROOT "/event1", BRICK_SESSION_INPUT_ROOT "/event3"};
    for (int i = 0; i < 3; i++) {
        fds[i].fd = open(nodes[i], O_RDONLY | O_NONBLOCK | O_CLOEXEC);
        fds[i].events = POLLIN;
        if (fds[i].fd < 0) return 3;
    }
    struct timespec start, now;
    clock_gettime(CLOCK_BOOTTIME, &start);
    for (;;) {
        if (!access(stop, F_OK)) return 0;
        clock_gettime(CLOCK_BOOTTIME, &now);
        int abort_session = now.tv_sec - start.tv_sec >= lifetime;
        int n = poll(fds, 3, 100);
        if (n < 0 && errno != EINTR) abort_session = 1;
        for (int i = 0; i < 3; i++) {
            if (fds[i].revents & (POLLERR | POLLHUP | POLLNVAL)) abort_session = 1;
            if (!(fds[i].revents & POLLIN)) continue;
            struct input_event events[32];
            ssize_t bytes = read(fds[i].fd, events, sizeof events);
            for (ssize_t j = 0; j < bytes / (ssize_t)sizeof events[0]; j++)
                if (pressed(&events[j])) abort_session = 1;
        }
        if (abort_session) {
            int fd = open(abort_file, O_WRONLY | O_CREAT | O_CLOEXEC, 0600);
            if (fd >= 0) close(fd);
            fd = open(stop, O_WRONLY | O_CREAT | O_CLOEXEC, 0600);
            if (fd >= 0) close(fd);
            execl("/bin/sh", "sh", restore, argv[1], (char *)NULL);
            return 4;
        }
    }
}
