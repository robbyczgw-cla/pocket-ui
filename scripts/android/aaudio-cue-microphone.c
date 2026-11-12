// Silent native microphone recorder with monotonic read-delivery timestamps; does not change volume.
#include <aaudio/AAudio.h>
#include <stdint.h>
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>
static int64_t now_ns(void){struct timespec t;clock_gettime(CLOCK_MONOTONIC,&t);return(int64_t)t.tv_sec*1000000000+t.tv_nsec;}
int main(int argc,char **argv){
 if(argc!=3)return 2;int seconds=atoi(argv[2]);if(seconds<1||seconds>60)return 2;
 AAudioStreamBuilder *b;AAudioStream *s=NULL;if(AAudio_createStreamBuilder(&b))return 1;
 AAudioStreamBuilder_setDirection(b,AAUDIO_DIRECTION_INPUT);AAudioStreamBuilder_setFormat(b,AAUDIO_FORMAT_PCM_I16);
 AAudioStreamBuilder_setSampleRate(b,48000);AAudioStreamBuilder_setChannelCount(b,1);
 AAudioStreamBuilder_setPerformanceMode(b,AAUDIO_PERFORMANCE_MODE_LOW_LATENCY);AAudioStreamBuilder_setInputPreset(b,AAUDIO_INPUT_PRESET_VOICE_RECOGNITION);
 int error=AAudioStreamBuilder_openStream(b,&s);AAudioStreamBuilder_delete(b);if(error){fprintf(stderr,"input open %d\n",error);return 1;}
 if(AAudioStream_getSampleRate(s)!=48000||AAudioStream_getChannelCount(s)!=1){AAudioStream_close(s);return 1;}
 FILE *f=fopen(argv[1],"wb");if(!f){AAudioStream_close(s);return 1;}
 printf("# input rate=48000 burst=%d device=%d mode=%d\nframe,read_end_ns,frames\n",AAudioStream_getFramesPerBurst(s),AAudioStream_getDeviceId(s),AAudioStream_getPerformanceMode(s));fflush(stdout);
 error=AAudioStream_requestStart(s);int64_t start=now_ns(),pos=0;int16_t pcm[192];
 while(!error&&now_ns()-start<(int64_t)seconds*1000000000){int n=AAudioStream_read(s,pcm,192,50000000);int64_t at=now_ns();if(n<0){error=n;break;}if(n){fwrite(pcm,2,n,f);pos+=n;printf("%"PRId64",%"PRId64",%d\n",pos,at,n);}}
 AAudioStream_requestStop(s);AAudioStream_close(s);fclose(f);return error?1:0;
}
