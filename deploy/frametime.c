// Frame-time logger for any emulator, loaded with LD_PRELOAD: records the moment of every presented frame
// (eglSwapBuffers, glXSwapBuffers, vkQueuePresentKHR) so pocket-core, RetroArch and the standalone
// emulators are timed the same way. Each process that presents writes CLOCK_MONOTONIC nanoseconds, one
// per line, to /tmp/ft/<comm>.<pid> (flushed every 30 frames). deploy/frametime-stats.py reads them.
//
// Apps that look the functions up themselves (SDL, glutin, Vulkan loaders) are caught through dlsym and
// vkGet{Instance,Device}ProcAddr. Build (on the Mac):
//   zig cc -target aarch64-linux-gnu.2.38 -shared -fPIC -O2 -o frametime.so deploy/frametime.c
#define _GNU_SOURCE
#include <dlfcn.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>

typedef void *(*dlsym_fn)(void *, const char *);
typedef unsigned (*egl_swap_fn)(void *, void *);
typedef unsigned (*egl_swap_damage_fn)(void *, void *, int *, int);
typedef void (*glx_swap_fn)(void *, unsigned long);
typedef int (*vk_present_fn)(void *, const void *);
typedef void *(*vk_gpa_fn)(void *, const char *);

static dlsym_fn real_dlsym_p;
static egl_swap_fn real_egl_swap;
static egl_swap_damage_fn real_egl_swap_damage_khr, real_egl_swap_damage_ext;
static glx_swap_fn real_glx_swap;
static vk_present_fn real_vk_present;
static vk_gpa_fn real_vk_gipa, real_vk_gdpa;

static uint64_t buf[30];
static int nbuf, fd = -1;

static void *real_dlsym(void *h, const char *name) {
    if (!real_dlsym_p) {
        real_dlsym_p = (dlsym_fn)dlvsym(RTLD_NEXT, "dlsym", "GLIBC_2.34");
        if (!real_dlsym_p)
            real_dlsym_p = (dlsym_fn)dlvsym(RTLD_NEXT, "dlsym", "GLIBC_2.17");
    }
    return real_dlsym_p(h, name);
}

static void flush_frames(void) {
    char text[30 * 21];
    int n = 0;
    if (fd < 0) {
        char comm[32] = "?", path[96];
        int c = open("/proc/self/comm", O_RDONLY);
        if (c >= 0) {
            ssize_t r = read(c, comm, sizeof comm - 1);
            close(c);
            comm[r > 0 ? r : 0] = 0;
            char *nl = strchr(comm, '\n');
            if (nl)
                *nl = 0;
        }
        mkdir("/tmp/ft", 0777);
        snprintf(path, sizeof path, "/tmp/ft/%s.%d", comm, (int)getpid());
        fd = open(path, O_WRONLY | O_CREAT | O_APPEND | O_CLOEXEC, 0644);
    }
    for (int i = 0; i < nbuf; i++)
        n += snprintf(text + n, sizeof text - n, "%llu\n", (unsigned long long)buf[i]);
    if (fd >= 0 && write(fd, text, n) < 0) {
    }
    nbuf = 0;
}

static void frame(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    buf[nbuf++] = (uint64_t)ts.tv_sec * 1000000000ull + ts.tv_nsec;
    if (nbuf == 30)
        flush_frames();
}

__attribute__((destructor)) static void done(void) {
    if (nbuf)
        flush_frames();
}

// The next definition of `name`: in the global scope, else in the library that has it when the app
// loaded that privately (RTLD_LOCAL, as SDL and Vulkan loaders do). Such a library's own references to
// its exported functions resolve to ours (we are preloaded), so pointers it hands out may be our
// wrappers, which then need the real function from here.
static void *lookup(const char *name) {
    void *f = real_dlsym(RTLD_NEXT, name);
    if (!f) {
        const char *lib = !strncmp(name, "vk", 2)    ? "libvulkan.so.1"
                          : !strncmp(name, "glX", 3) ? "libGLX.so.0"
                          : !strncmp(name, "xcb", 3) ? "libxcb-shm.so.0"
                                                     : "libEGL.so.1";
        void *h = dlopen(lib, RTLD_LAZY | RTLD_NOLOAD);
        if (h)
            f = real_dlsym(h, name);
    }
    return f;
}

unsigned eglSwapBuffers(void *dpy, void *surface) {
    if (!real_egl_swap)
        real_egl_swap = (egl_swap_fn)lookup("eglSwapBuffers");
    frame();
    return real_egl_swap(dpy, surface);
}

unsigned eglSwapBuffersWithDamageKHR(void *dpy, void *surface, int *rects, int n) {
    if (!real_egl_swap_damage_khr)
        real_egl_swap_damage_khr = (egl_swap_damage_fn)lookup("eglSwapBuffersWithDamageKHR");
    frame();
    return real_egl_swap_damage_khr(dpy, surface, rects, n);
}

unsigned eglSwapBuffersWithDamageEXT(void *dpy, void *surface, int *rects, int n) {
    if (!real_egl_swap_damage_ext)
        real_egl_swap_damage_ext = (egl_swap_damage_fn)lookup("eglSwapBuffersWithDamageEXT");
    frame();
    return real_egl_swap_damage_ext(dpy, surface, rects, n);
}

void glXSwapBuffers(void *dpy, unsigned long drawable) {
    if (!real_glx_swap)
        real_glx_swap = (glx_swap_fn)lookup("glXSwapBuffers");
    frame();
    real_glx_swap(dpy, drawable);
}

// Software-drawn X11 windows (standalone melonDS through Qt without OpenGL): each repaint is a shared-memory put.
typedef unsigned (*xcb_shm_put_fn)(void *, uint32_t, uint32_t, uint16_t, uint16_t, uint16_t, uint16_t, uint16_t, uint16_t, int16_t, int16_t, uint8_t,
                                   uint8_t, uint8_t, uint32_t, uint32_t);
static xcb_shm_put_fn real_xcb_shm_put;

unsigned xcb_shm_put_image(void *c, uint32_t drawable, uint32_t gc, uint16_t tw, uint16_t th, uint16_t sx, uint16_t sy, uint16_t sw, uint16_t sh,
                           int16_t dx, int16_t dy, uint8_t depth, uint8_t format, uint8_t send_event, uint32_t seg, uint32_t offset) {
    if (!real_xcb_shm_put)
        real_xcb_shm_put = (xcb_shm_put_fn)lookup("xcb_shm_put_image");
    // Only large puts: the game picture, not the menu bar or a cursor.
    if ((unsigned)sw * sh >= 256 * 192)
        frame();
    return real_xcb_shm_put(c, drawable, gc, tw, th, sx, sy, sw, sh, dx, dy, depth, format, send_event, seg, offset);
}

static int vk_present(void *queue, const void *info) {
    frame();
    return real_vk_present(queue, info);
}

static void *wrap_vk(const char *name, void *real);
void *vkGetInstanceProcAddr(void *instance, const char *name);
void *vkGetDeviceProcAddr(void *device, const char *name);
int vkQueuePresentKHR(void *queue, const void *info);

static void *vk_gdpa(void *device, const char *name) {
    return wrap_vk(name, real_vk_gdpa(device, name));
}

static void *vk_gipa(void *instance, const char *name) {
    return wrap_vk(name, real_vk_gipa(instance, name));
}

// Hands out our wrappers for the functions we time and for the lookups that lead to them.
static void *wrap_vk(const char *name, void *real) {
    // A lookup may resolve to our own exported wrapper (the loader finds it through the global scope);
    // storing that as "the real one" would recurse forever.
    if (!real || !name || real == (void *)vk_present || real == (void *)vk_gdpa || real == (void *)vk_gipa || real == (void *)vkGetInstanceProcAddr ||
        real == (void *)vkGetDeviceProcAddr || real == (void *)vkQueuePresentKHR)
        return real;
    if (!strcmp(name, "vkQueuePresentKHR")) {
        real_vk_present = (vk_present_fn)real;
        return (void *)vk_present;
    }
    if (!strcmp(name, "vkGetDeviceProcAddr")) {
        real_vk_gdpa = (vk_gpa_fn)real;
        return (void *)vk_gdpa;
    }
    if (!strcmp(name, "vkGetInstanceProcAddr")) {
        real_vk_gipa = (vk_gpa_fn)real;
        return (void *)vk_gipa;
    }
    return real;
}

void *vkGetInstanceProcAddr(void *instance, const char *name) {
    if (!real_vk_gipa)
        real_vk_gipa = (vk_gpa_fn)lookup("vkGetInstanceProcAddr");
    return real_vk_gipa ? wrap_vk(name, real_vk_gipa(instance, name)) : 0;
}

void *vkGetDeviceProcAddr(void *device, const char *name) {
    if (!real_vk_gdpa)
        real_vk_gdpa = (vk_gpa_fn)lookup("vkGetDeviceProcAddr");
    return real_vk_gdpa ? wrap_vk(name, real_vk_gdpa(device, name)) : 0;
}

int vkQueuePresentKHR(void *queue, const void *info) {
    if (!real_vk_present)
        real_vk_present = (vk_present_fn)lookup("vkQueuePresentKHR");
    return vk_present(queue, info);
}

static void *wrap(const char *name, void *real);

typedef void *(*egl_gpa_fn)(const char *);
static egl_gpa_fn real_egl_gpa;

void *eglGetProcAddress(const char *name) {
    if (!real_egl_gpa)
        real_egl_gpa = (egl_gpa_fn)lookup("eglGetProcAddress");
    return real_egl_gpa ? wrap(name, real_egl_gpa(name)) : 0;
}

void *dlsym(void *handle, const char *name) {
    return wrap(name, real_dlsym(handle, name));
}

static void *wrap(const char *name, void *real) {
    if (!real || !name || real == (void *)eglSwapBuffers || real == (void *)eglSwapBuffersWithDamageKHR || real == (void *)eglSwapBuffersWithDamageEXT ||
        real == (void *)eglGetProcAddress || real == (void *)glXSwapBuffers)
        return real;
    if (!strcmp(name, "eglSwapBuffers")) {
        real_egl_swap = (egl_swap_fn)real;
        return (void *)eglSwapBuffers;
    }
    if (!strcmp(name, "eglSwapBuffersWithDamageKHR")) {
        real_egl_swap_damage_khr = (egl_swap_damage_fn)real;
        return (void *)eglSwapBuffersWithDamageKHR;
    }
    if (!strcmp(name, "eglSwapBuffersWithDamageEXT")) {
        real_egl_swap_damage_ext = (egl_swap_damage_fn)real;
        return (void *)eglSwapBuffersWithDamageEXT;
    }
    if (!strcmp(name, "eglGetProcAddress")) {
        real_egl_gpa = (egl_gpa_fn)real;
        return (void *)eglGetProcAddress;
    }
    if (!strcmp(name, "glXSwapBuffers")) {
        real_glx_swap = (glx_swap_fn)real;
        return (void *)glXSwapBuffers;
    }
    return wrap_vk(name, real);
}
