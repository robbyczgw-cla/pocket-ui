/* The log callback pocket-core hands to libretro cores (RETRO_ENVIRONMENT_GET_LOG_INTERFACE). It is
 * printf-style and C-variadic, which stable Rust can't define, so it lives here: it formats the line with
 * the core's real arguments (strings, doubles, any number of them) and hands the text to Rust. Built by
 * build.rs through the `cc` crate (with zig's cc under cargo-zigbuild). */
#include <stdarg.h>
#include <stdio.h>

typedef void (*pocket_log_sink)(unsigned level, const char *text);

static pocket_log_sink sink;
static unsigned min_level;

/* Where formatted lines go, and the lowest level worth formatting. Set before the core gets the callback. */
void pocket_log_setup(pocket_log_sink s, unsigned level) {
    sink = s;
    min_level = level;
}

void pocket_log_printf(unsigned level, const char *fmt, ...) {
    char buf[512];
    va_list ap;
    if (!fmt || !sink || level < min_level)
        return;
    va_start(ap, fmt);
    vsnprintf(buf, sizeof buf, fmt, ap);
    va_end(ap);
    sink(level, buf);
}
