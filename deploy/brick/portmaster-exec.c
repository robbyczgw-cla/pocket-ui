/* SPDX-License-Identifier: GPL-2.0-or-later
 * Stock TrimUI lacks PID namespaces. A temporary chroot supplies /bin/bash and
 * legacy paths without writing firmware. pidof/pkill/killall see this launch only.
 */
#define _GNU_SOURCE
#include <dirent.h>
#include <errno.h>
#include <ftw.h>
#include <limits.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mount.h>
#include <sys/prctl.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

static volatile sig_atomic_t child_pid;
static volatile sig_atomic_t stopping;
static pid_t guardian_pid;
static char mounts[16][PATH_MAX];
static int mounted;
static void forward(int sig) { stopping = 1; if (child_pid > 0) kill(child_pid, sig); }
static int removed(const char *p, const struct stat *st, int kind, struct FTW *ftw) {
    (void)st; (void)kind; (void)ftw; return remove(p);
}
static int path(char *out, const char *base, const char *name) {
    if (snprintf(out, PATH_MAX, "%s/%s", base, name) >= PATH_MAX) { errno = ENAMETOOLONG; return -1; }
    return 0;
}
static int make(const char *p) { return mkdir(p, 0700) && errno != EEXIST ? -1 : 0; }
static int bind(const char *from, const char *to) {
    if (mounted >= 16 || mount(from, to, NULL, MS_BIND, NULL)) return -1;
    strcpy(mounts[mounted++], to); return 0;
}
static int link_path(const char *root, const char *name, const char *target) {
    char to[PATH_MAX]; if (path(to, root, name)) return -1;
    return symlink(target, to);
}
static int setup(const char *root, const char *app) {
    char dest[PATH_MAX], source[PATH_MAX], bin[PATH_MAX];
    const char *dirs[] = {"usr", "lib", "etc", "dev", "proc", "sys", "host-bin", "bin", "tmp", "tmp/shm", "opt", "mnt", "storage", NULL};
    for (int i = 0; dirs[i]; ++i) if (path(dest, root, dirs[i]) || make(dest)) return -1;
    // Stock /dev/shm points to /tmp/shm; the chroot has a private /tmp.
    if (path(dest, root, "tmp/shm") || chmod(dest, 01777)) return -1;
    const char *shared[] = {"usr", "lib", "etc", "dev", "proc", "sys", NULL};
    for (int i = 0; shared[i]; ++i) {
        if (path(source, "", shared[i]) || path(dest, root, shared[i]) || bind(source, dest)) return -1;
    }
    // Legacy launchers chmod/write tty0 even after control.txt chose /dev/null.
    // Their console belongs to this launch, so it cannot draw over the UI or
    // change the firmware console's permissions.
    if (path(dest, root, "dev/tty0")) return -1;
    if (access(dest, F_OK) == 0) {
        if (path(source, root, "tmp/console")) return -1;
        FILE *console = fopen(source, "w"); if (!console) return -1;
        if (fclose(console) || bind(source, dest)) return -1;
    }
    if (path(dest, root, "host-bin") || bind("/bin", dest)) return -1;
    if (path(bin, root, "bin")) return -1;
    DIR *dir = opendir("/bin"); if (!dir) return -1;
    struct dirent *entry;
    while ((entry = readdir(dir))) {
        if (!strcmp(entry->d_name, ".") || !strcmp(entry->d_name, "..") || !strcmp(entry->d_name, "bash") ||
            !strcmp(entry->d_name, "pidof") || !strcmp(entry->d_name, "pkill") || !strcmp(entry->d_name, "killall")) continue;
        if (path(source, "/host-bin", entry->d_name) || link_path(bin, entry->d_name, source)) { closedir(dir); return -1; }
    }
    closedir(dir);
    if (path(source, app, "bin/bash") || link_path(bin, "bash", source)) return -1;
    if (path(source, app, "bin/portmaster-exec")) return -1;
    const char *scoped[] = {"pidof", "pkill", "killall", NULL};
    for (int i = 0; scoped[i]; ++i) if (link_path(bin, scoped[i], source)) return -1;
    if (path(dest, root, "mnt/SDCARD") || make(dest) || bind("/mnt/SDCARD", dest)) return -1;
    const char *card = getenv("BRICK_CARD"); if (!card) card = "/mnt/SDCARD";
    if (path(source, card, "pocket-ui/storage") || path(dest, root, "storage") || bind(source, dest)) return -1;
    if (path(dest, root, "storage/roms") || make(dest) || path(source, card, "Roms") || bind(source, dest)) return -1;
    if (link_path(root, "roms", "/storage/roms")) return -1;
    if (path(dest, root, "opt/tools") || make(dest)) return -1;
    const char *pm = getenv("BRICK_PORTMASTER_HOME");
    if (!pm || link_path(root, "opt/tools/PortMaster", pm)) return -1;
    return 0;
}

/* Standard upstream scripts can keep name-based cleanup without touching a
 * mapper belonging to pocket-ui or to another game session. */
static int scoped_processes(int argc, char **argv, const char *mode) {
    int sig = SIGTERM, full = 0, found = 0;
    const char *name = NULL;
    for (int i = 1; i < argc; ++i) {
        if (!strcmp(argv[i], "-f")) full = 1;
        else if (!strcmp(argv[i], "-9") || !strcmp(argv[i], "-KILL")) sig = SIGKILL;
        else if (!strcmp(argv[i], "-STOP")) sig = SIGSTOP;
        else if (!strcmp(argv[i], "-CONT")) sig = SIGCONT;
        else if (argv[i][0] != '-') name = argv[i];
    }
    if (!name) return 1;
    DIR *dir = opendir("/proc"); if (!dir) return 1;
    struct dirent *entry; pid_t session = getsid(0);
    while ((entry = readdir(dir))) {
        char *end; long value = strtol(entry->d_name, &end, 10);
        if (*end || value <= 1 || value == getpid() || getsid((pid_t)value) != session) continue;
        char filename[128], text[4096];
        snprintf(filename, sizeof filename, "/proc/%ld/%s", value, full ? "cmdline" : "comm");
        FILE *file = fopen(filename, "r"); if (!file) continue;
        size_t len = fread(text, 1, sizeof text - 1, file); fclose(file); text[len] = 0;
        if (full) { for (size_t i = 0; i < len; ++i) if (!text[i]) text[i] = ' '; }
        else if (len && text[len-1] == '\n') text[len-1] = 0;
        if ((full && strstr(text, name)) || (!full && !strcmp(text, name))) {
            found = 1;
            if (!strcmp(mode, "pidof")) printf("%ld ", value);
            else kill((pid_t)value, sig);
        }
    }
    closedir(dir);
    if (found && !strcmp(mode, "pidof")) puts("");
    return found ? 0 : 1;
}

static int parent_of(pid_t pid) {
    char filename[80], text[4096];
    snprintf(filename, sizeof filename, "/proc/%d/stat", pid);
    FILE *file = fopen(filename, "r"); if (!file) return -1;
    size_t len = fread(text, 1, sizeof text - 1, file); fclose(file); text[len] = 0;
    char *rest = strrchr(text, ')'); int parent; char state;
    return rest && sscanf(rest + 1, " %c %d", &state, &parent) == 2 ? parent : -1;
}

/* A stopped process cannot act on TERM. Keep this watcher in a separate session
 * from pause_game's entire port session, and bind its owner to a proc starttime. */
static int process_identity(pid_t pid, unsigned long long *birth, pid_t *session) {
    char filename[80], text[4096];
    snprintf(filename, sizeof filename, "/proc/%d/stat", pid);
    FILE *file = fopen(filename, "r"); if (!file) return 0;
    size_t len = fread(text, 1, sizeof text - 1, file); fclose(file); text[len] = 0;
    char *rest = strrchr(text, ')'), *save = NULL;
    if (!rest || rest[1] != ' ') return 0;
    char *field = strtok_r(rest + 2, " ", &save);
    for (int index = 0; field; ++index, field = strtok_r(NULL, " ", &save)) {
        if (index == 0 && (*field == 'Z' || *field == 'X')) return 0;
        if (index == 3) *session = (pid_t)strtol(field, NULL, 10);
        if (index == 19) { *birth = strtoull(field, NULL, 10); return *birth != 0; }
    }
    return 0;
}
static int same_process(pid_t pid, unsigned long long birth) {
    unsigned long long current; pid_t session;
    return process_identity(pid, &current, &session) && current == birth;
}
static void kill_port_session(pid_t session, pid_t supervisor) {
    DIR *dir = opendir("/proc"); if (!dir) return;
    struct dirent *entry;
    while ((entry = readdir(dir))) {
        char *end; long value = strtol(entry->d_name, &end, 10);
        if (*end || value <= 1 || value == supervisor) continue;
        unsigned long long birth; pid_t found_session;
        if (process_identity((pid_t)value, &birth, &found_session) && found_session == session &&
            same_process((pid_t)value, birth)) kill((pid_t)value, SIGKILL);
    }
    closedir(dir);
}
static pid_t start_guardian(pid_t owner, unsigned long long owner_birth) {
    int ready[2]; if (pipe(ready)) return -1;
    pid_t supervisor = getpid(), session = getsid(0);
    unsigned long long supervisor_birth; pid_t ignored;
    if (!process_identity(supervisor, &supervisor_birth, &ignored)) { close(ready[0]); close(ready[1]); return -1; }
    pid_t watcher = fork();
    if (watcher == 0) {
        close(ready[0]);
        signal(SIGTERM, SIG_DFL); signal(SIGINT, SIG_DFL); signal(SIGHUP, SIG_DFL);
        if (setsid() < 0 || write(ready[1], "1", 1) != 1) _exit(125);
        close(ready[1]);
        const struct timespec nap = {0, 100000000};
        while (same_process(owner, owner_birth) && same_process(supervisor, supervisor_birth)) nanosleep(&nap, NULL);
        /* Never resume card writers after the UI has gone. Kill stopped guests
         * first, then wake only the C supervisor to reap and undo its mounts. */
        kill_port_session(session, supervisor);
        if (same_process(supervisor, supervisor_birth)) {
            kill(supervisor, SIGTERM);
            kill(supervisor, SIGCONT);
        }
        _exit(0);
    }
    close(ready[1]);
    char byte; ssize_t count;
    do { count = read(ready[0], &byte, 1); } while (count < 0 && errno == EINTR);
    close(ready[0]);
    if (watcher < 0 || count != 1) {
        if (watcher > 0) { kill(watcher, SIGKILL); while (waitpid(watcher, NULL, 0) < 0 && errno == EINTR) {} }
        return -1;
    }
    return watcher;
}
static void stop_guardian(void) {
    if (guardian_pid <= 0) return;
    /* This is our unreaped direct child, so its PID cannot have been reused. */
    kill(guardian_pid, SIGTERM);
    while (waitpid(guardian_pid, NULL, 0) < 0 && errno == EINTR) {}
    guardian_pid = 0;
}
static void cleanup_children(int sig) {
    DIR *dir = opendir("/proc"); if (!dir) return;
    struct dirent *entry;
    while ((entry = readdir(dir))) {
        char *end; long value = strtol(entry->d_name, &end, 10);
        if (*end || value <= 1 || value == getpid()) continue;
        int parent = parent_of((pid_t)value), steps = 0;
        while (parent > 1 && parent != getpid() && steps++ < 64) parent = parent_of(parent);
        if (parent == getpid()) kill((pid_t)value, sig);
    }
    closedir(dir);
    while (waitpid(-1, NULL, WNOHANG) > 0) {}
}

int main(int argc, char **argv) {
    const char *mode = strrchr(argv[0], '/'); mode = mode ? mode + 1 : argv[0];
    if (!strcmp(mode, "pidof") || !strcmp(mode, "pkill") || !strcmp(mode, "killall")) return scoped_processes(argc, argv, mode);
    const char *app = getenv("BRICK_APP"), *run = getenv("BRICK_RUN");
    if (argc < 2 || !app || app[0] != '/' || !run || run[0] != '/') {
        fputs("PortMaster: expected launcher and absolute BRICK_APP/BRICK_RUN\n", stderr); return 2;
    }
    const char *owner_text = getenv("BRICK_PORTMASTER_OWNER_PID"); char *end;
    long owner = owner_text ? strtol(owner_text, &end, 10) : 0;
    unsigned long long owner_birth; pid_t owner_session;
    if (!owner_text || !*owner_text || *end || owner <= 1 || owner > INT_MAX ||
        !process_identity((pid_t)owner, &owner_birth, &owner_session) || owner_session == getsid(0)) {
        fputs("PortMaster: expected a live UI owner outside the port session\n", stderr); return 125;
    }
    char root[PATH_MAX], bash[PATH_MAX];
    if (path(bash, app, "bin/bash") || access(bash, X_OK)) { perror("PortMaster Bash"); return 125; }
    if (path(root, run, "portmaster-root-XXXXXX") || !mkdtemp(root)) { perror("port root"); return 125; }
    int result = 125;
    prctl(PR_SET_CHILD_SUBREAPER, 1);
    signal(SIGTERM, forward); signal(SIGINT, forward); signal(SIGHUP, forward);
    guardian_pid = start_guardian((pid_t)owner, owner_birth);
    if (guardian_pid < 0) { perror("port owner watcher"); goto cleanup; }
    if (setup(root, app)) { perror("PortMaster root setup"); goto cleanup; }
    if (stopping) goto cleanup;
    pid_t game = fork(); if (game < 0) { perror("port fork"); goto cleanup; }
    if (game == 0) {
        signal(SIGTERM, SIG_DFL); signal(SIGINT, SIG_DFL); signal(SIGHUP, SIG_DFL);
        prctl(PR_SET_PDEATHSIG, SIGTERM);
        if (chroot(root) || chdir("/")) { perror("port chroot"); _exit(125); }
        // Firmware's ld.so.cache may point through /tmp. The ordinary library
        // directories must also work without that host cache inside the chroot.
        char libraries[PATH_MAX];
        snprintf(libraries, sizeof libraries, "%s:/lib:/usr/lib", getenv("LD_LIBRARY_PATH") ? getenv("LD_LIBRARY_PATH") : "");
        setenv("LD_LIBRARY_PATH", libraries, 1);
        argv[0] = bash;
        execv(bash, argv); perror("launch port"); _exit(125);
    }
    child_pid = game;
    int status, ticks = 0;
    const struct timespec nap = {0, 100000000};
    for (;;) {
        pid_t ended = waitpid(game, &status, WNOHANG);
        if (ended == game) break;
        if (ended < 0 && errno != EINTR) { perror("wait port"); goto cleanup; }
        if (stopping && ++ticks == 10) kill(game, SIGKILL);
        if (stopping && ticks > 15) { status = SIGKILL; break; }
        nanosleep(&nap, NULL);
    }
    result = WIFEXITED(status) ? WEXITSTATUS(status) : 128 + WTERMSIG(status);
    stop_guardian();
    cleanup_children(SIGTERM);
    nanosleep(&nap, NULL);
    cleanup_children(SIGKILL);
cleanup:
    stop_guardian();
    {
        int safe = 1;
        while (mounted) if (umount2(mounts[--mounted], MNT_DETACH)) { perror("port root unmount"); safe = 0; }
        // Never walk a tree if a bind might still point at the firmware or card.
        if (safe) nftw(root, removed, 16, FTW_DEPTH | FTW_PHYS);
    }
    return result;
}
