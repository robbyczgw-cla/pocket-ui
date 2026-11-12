// Silent route probe: same 48 kHz stereo I16 callback and three-burst buffer as pocket-ui.
// Build with the Android NDK (API 28+), link -laaudio -ldl. Never changes system volume.
#include <aaudio/AAudio.h>
#include <dlfcn.h>
#include <inttypes.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>
static int64_t now_ns(void) {
    struct timespec t; clock_gettime(CLOCK_MONOTONIC, &t);
    return (int64_t)t.tv_sec * 1000000000 + t.tv_nsec;
}
static aaudio_data_callback_result_t silence(AAudioStream *s, void *ctx, void *data, int32_t frames) {
    (void)s; (void)ctx; memset(data, 0, (size_t)frames * 4); return AAUDIO_CALLBACK_RESULT_CONTINUE;
}
int main(int argc, char **argv) {
    if (argc != 5 && argc != 6) { fprintf(stderr, "usage: probe USAGE CONTENT PERFORMANCE SHARING [SESSION]\n"); return 2; }
    AAudioStreamBuilder *b = NULL; AAudioStream *s = NULL;
    if (AAudio_createStreamBuilder(&b) != AAUDIO_OK) return 1;
    AAudioStreamBuilder_setDirection(b, AAUDIO_DIRECTION_OUTPUT);
    AAudioStreamBuilder_setFormat(b, AAUDIO_FORMAT_PCM_I16);
    AAudioStreamBuilder_setSampleRate(b, 48000);
    AAudioStreamBuilder_setChannelCount(b, 2);
    AAudioStreamBuilder_setFramesPerDataCallback(b, 256);
    AAudioStreamBuilder_setUsage(b, atoi(argv[1]));
    AAudioStreamBuilder_setContentType(b, atoi(argv[2]));
    AAudioStreamBuilder_setPerformanceMode(b, atoi(argv[3]));
    AAudioStreamBuilder_setSharingMode(b, atoi(argv[4]));
    if (argc == 6) AAudioStreamBuilder_setSessionId(b, atoi(argv[5]));
    AAudioStreamBuilder_setDataCallback(b, silence, NULL);
    int result = AAudioStreamBuilder_openStream(b, &s);
    AAudioStreamBuilder_delete(b);
    if (result) { fprintf(stderr, "open=%d\n", result); return 1; }
    int burst = AAudioStream_getFramesPerBurst(s);
    int resized = AAudioStream_setBufferSizeInFrames(s, burst * 3);
    bool (*mmap_used)(AAudioStream *) = (bool (*)(AAudioStream *))dlsym(RTLD_DEFAULT, "AAudioStream_isMMapUsed");
    printf("# pid=%d usage=%d content=%d performance=%d sharing=%d device=%d rate=%d burst=%d buffer=%d capacity=%d resized=%d mmap=%d session=%d\n",
        getpid(), AAudioStream_getUsage(s), AAudioStream_getContentType(s), AAudioStream_getPerformanceMode(s),
        AAudioStream_getSharingMode(s), AAudioStream_getDeviceId(s), AAudioStream_getSampleRate(s), burst,
        AAudioStream_getBufferSizeInFrames(s), AAudioStream_getBufferCapacityInFrames(s), resized, mmap_used ? mmap_used(s) : -1, AAudioStream_getSessionId(s));
    printf("elapsed_ms,now_ns,written_before,written_after,read,presented,presented_ns,status,xruns\n"); fflush(stdout);
    if (AAudioStream_requestStart(s) != AAUDIO_OK) { AAudioStream_close(s); return 1; }
    int64_t start = now_ns();
    for (int i = 0; i < 120; ++i) {
        int64_t p = -1, t = 0, w0 = AAudioStream_getFramesWritten(s);
        int status = AAudioStream_getTimestamp(s, CLOCK_MONOTONIC, &p, &t);
        int64_t n = now_ns(), w1 = AAudioStream_getFramesWritten(s), r = AAudioStream_getFramesRead(s);
        printf("%.3f,%" PRId64 ",%" PRId64 ",%" PRId64 ",%" PRId64 ",%" PRId64 ",%" PRId64 ",%d,%d\n",
            (n-start)/1e6, n, w0, w1, r, p, t, status, AAudioStream_getXRunCount(s));
        fflush(stdout); usleep(100000);
    }
    AAudioStream_requestStop(s); AAudioStream_close(s); return 0;
}
