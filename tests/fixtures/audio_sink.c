/* Device boundary for journeys: real Audio queue/resampler/writer, a clocked
 * Pulse simple output instead of speakers. Each delivery records time and peak. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>
#include <unistd.h>
#include <string.h>
#include <errno.h>
struct sink { FILE *out; unsigned rate; uint64_t frames; struct timespec started; };
struct spec { int format; uint32_t rate; uint8_t channels; };
void *pa_simple_new(const char *server,const char *name,int dir,const char *dev,const char *label,const struct spec *spec,const void *map,const void *attr,int *err) {
 (void)server;(void)name;(void)dir;(void)dev;(void)label;(void)map;(void)attr;(void)err;
 char executable[4096]; ssize_t n=readlink("/proc/self/exe",executable,sizeof executable-1);
 if(n<0)return NULL; executable[n]=0;
 if(!strstr(executable,"pocket-core") || !getenv("POCKET_TEST_AUDIO_OUT"))return NULL;
 struct sink *s=calloc(1,sizeof *s); s->rate=spec->rate;
 s->out=fopen(getenv("POCKET_TEST_AUDIO_OUT"),"a"); if(!s->out){free(s);return NULL;} return s;
}
int pa_simple_write(void *ptr,const void *data,size_t bytes,int *err) {
 (void)err; struct sink *s=ptr; const int16_t *p=data; unsigned peak=0;
 for(size_t i=0;i<bytes/2;i++){unsigned a=abs(p[i]);if(a>peak)peak=a;}
 struct timespec t;clock_gettime(CLOCK_MONOTONIC,&t);
 if(!s->frames)s->started=t;
 fprintf(s->out,"%.6f %zu %u\n",t.tv_sec+t.tv_nsec/1e9,bytes/4,peak);fflush(s->out);
 /* A device clock: relative sleeps accumulate scheduling/IO overhead and
  * quietly make a declared 48 kHz sink consume only about 46 kHz. */
 s->frames+=bytes/4;
 struct timespec due=s->started;
 due.tv_sec+=s->frames/s->rate;
 due.tv_nsec+=(long)((s->frames%s->rate)*1000000000ULL/s->rate);
 if(due.tv_nsec>=1000000000L){due.tv_sec++;due.tv_nsec-=1000000000L;}
 while(clock_nanosleep(CLOCK_MONOTONIC,TIMER_ABSTIME,&due,NULL)==EINTR){}
 return 0;
}
uint64_t pa_simple_get_latency(void *s,int *err){(void)s;(void)err;return 0;}
void pa_simple_free(void *ptr){struct sink *s=ptr;fclose(s->out);free(s);}
