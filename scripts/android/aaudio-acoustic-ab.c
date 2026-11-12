// Relative acoustic A/B measurement. Caller MUST enforce volume=1 and restore volume=0.
#include <aaudio/AAudio.h>
#include <inttypes.h>
#include <math.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#define RATE 48000
#define TONE 480
#define BLOCKS 4
#define CLICKS 6
static int16_t tick[TONE];
static _Atomic int count=0;
struct Event {int block;int64_t time,frame;} events[BLOCKS*CLICKS];
struct Output {int block;int64_t frame;};
static int64_t now_ns(void){struct timespec t;clock_gettime(CLOCK_MONOTONIC,&t);return(int64_t)t.tv_sec*1000000000+t.tv_nsec;}
static aaudio_data_callback_result_t output(AAudioStream *s,void *u,void *data,int32_t n){
    (void)s;struct Output *o=u;int16_t *pcm=data;memset(pcm,0,(size_t)n*4);int64_t at=now_ns();
    for(int i=0;i<n;++i,++o->frame)for(int k=0;k<CLICKS;++k){
        int64_t j=o->frame-(k+1)*RATE;
        if(j>=0&&j<TONE)pcm[2*i]=pcm[2*i+1]=tick[j];
        if(j==0){int c=atomic_load_explicit(&count,memory_order_relaxed);events[c]=(struct Event){o->block,at,o->frame};atomic_store_explicit(&count,c+1,memory_order_release);}
    }
    return AAUDIO_CALLBACK_RESULT_CONTINUE;
}
int main(int argc,char **argv){
    if(argc!=2)return 2;
    uint32_t r=0x13579bdf;for(int i=0;i<TONE;++i){r^=r<<13;r^=r>>17;r^=r<<5;tick[i]=(int16_t)(((r&1)?16384:-16384)*sin(M_PI*i/(TONE-1)));}
    AAudioStreamBuilder *b;AAudioStream *in=NULL;
    AAudio_createStreamBuilder(&b);AAudioStreamBuilder_setDirection(b,AAUDIO_DIRECTION_INPUT);AAudioStreamBuilder_setFormat(b,AAUDIO_FORMAT_PCM_I16);
    AAudioStreamBuilder_setSampleRate(b,RATE);AAudioStreamBuilder_setChannelCount(b,1);AAudioStreamBuilder_setPerformanceMode(b,AAUDIO_PERFORMANCE_MODE_LOW_LATENCY);
    AAudioStreamBuilder_setInputPreset(b,AAUDIO_INPUT_PRESET_VOICE_RECOGNITION);int e=AAudioStreamBuilder_openStream(b,&in);AAudioStreamBuilder_delete(b);
    if(e){fprintf(stderr,"input open %d\n",e);return 1;}
    printf("# input rate=%d burst=%d device=%d mode=%d amplitude_peak=16384 tone_frames=%d\n",AAudioStream_getSampleRate(in),AAudioStream_getFramesPerBurst(in),AAudioStream_getDeviceId(in),AAudioStream_getPerformanceMode(in),TONE);fflush(stdout);
    if(AAudioStream_getSampleRate(in)!=RATE||AAudioStream_getChannelCount(in)!=1){AAudioStream_close(in);return 1;}
    FILE *f=fopen(argv[1],"wb");if(!f){AAudioStream_close(in);return 1;}
    AAudioStream_requestStart(in);int64_t pos=0;int16_t pcm[192];puts("frame,read_end_ns,frames");int failed=0;
    for(int block=-1;block<BLOCKS;++block){
        AAudioStream *out=NULL;struct Output ctx={block,0};
        if(block>=0){AAudio_createStreamBuilder(&b);AAudioStreamBuilder_setDirection(b,AAUDIO_DIRECTION_OUTPUT);AAudioStreamBuilder_setFormat(b,AAUDIO_FORMAT_PCM_I16);
            AAudioStreamBuilder_setSampleRate(b,RATE);AAudioStreamBuilder_setChannelCount(b,2);AAudioStreamBuilder_setPerformanceMode(b,AAUDIO_PERFORMANCE_MODE_LOW_LATENCY);
            AAudioStreamBuilder_setFramesPerDataCallback(b,256);if(block%2)AAudioStreamBuilder_setSessionId(b,AAUDIO_SESSION_ID_ALLOCATE);
            AAudioStreamBuilder_setDataCallback(b,output,&ctx);e=AAudioStreamBuilder_openStream(b,&out);AAudioStreamBuilder_delete(b);if(e){fprintf(stderr,"output open %d\n",e);failed=1;break;}
            AAudioStream_setBufferSizeInFrames(out,AAudioStream_getFramesPerBurst(out)*3);
            printf("# block=%d path=%s burst=%d buffer=%d device=%d session=%d\n",block,block%2?"fast":"raw",AAudioStream_getFramesPerBurst(out),AAudioStream_getBufferSizeInFrames(out),AAudioStream_getDeviceId(out),AAudioStream_getSessionId(out));
            if(AAudioStream_requestStart(out)!=AAUDIO_OK){AAudioStream_close(out);failed=1;break;}
        }
        int64_t start=now_ns(),duration=block<0?1000000000LL:7000000000LL;
        while(now_ns()-start<duration){int n=AAudioStream_read(in,pcm,192,50000000);int64_t at=now_ns();if(n<0){fprintf(stderr,"read %d\n",n);failed=1;break;}if(n){fwrite(pcm,2,n,f);pos+=n;printf("%"PRId64",%"PRId64",%d\n",pos,at,n);}}
        if(out){int64_t p=-1,t=0;int status=AAudioStream_getTimestamp(out,CLOCK_MONOTONIC,&p,&t);printf("# end_block=%d xruns=%d written=%"PRId64" presented=%"PRId64" timestamp_ns=%"PRId64" now_ns=%"PRId64" status=%d\n",block,AAudioStream_getXRunCount(out),AAudioStream_getFramesWritten(out),p,t,now_ns(),status);AAudioStream_requestStop(out);AAudioStream_close(out);}
        fflush(stdout);if(failed)break;
    }
    AAudioStream_requestStop(in);AAudioStream_close(in);fclose(f);
    int c=atomic_load_explicit(&count,memory_order_acquire);for(int k=0;k<c;++k)printf("# click=%d block=%d generated_ns=%"PRId64" output_frame=%"PRId64"\n",k,events[k].block,events[k].time,events[k].frame);
    return failed;
}
