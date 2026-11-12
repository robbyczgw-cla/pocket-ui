/* A libretro software core for visible-image contract tests, including Android QA.
 * Directions select RGB565, 0RGB1555 and XRGB8888; A selects no-callback frames.
 * Every scene submits a picture once, then NULL duplicates. Padding is magenta. */
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>
typedef bool (*env_cb)(unsigned, void *);
typedef void (*video_cb)(const void *,unsigned,unsigned,size_t);
typedef int16_t (*input_cb)(unsigned,unsigned,unsigned,unsigned);
static env_cb env; static video_cb video; static input_cb input;
static unsigned scene, tick, last = 99;
struct info { const char *name,*version,*exts;bool fullpath,extract; };
struct av { unsigned w,h,mw,mh;float aspect;double fps,rate; };
void retro_set_environment(env_cb f){env=f;}
void retro_set_video_refresh(video_cb f){video=f;}
void retro_set_input_state(input_cb f){input=f;}
void retro_set_input_poll(void (*f)(void)){(void)f;}
void retro_set_audio_sample(void (*f)(int16_t,int16_t)){(void)f;}
void retro_set_audio_sample_batch(size_t (*f)(const int16_t *,size_t)){(void)f;}
void retro_init(void){scene=tick=0;last=99;}
void retro_deinit(void){}
void retro_get_system_info(struct info *v){*v=(struct info){"Video contract","1","gb|gbc",false,false};}
void retro_get_system_av_info(struct av *v){*v=(struct av){160,144,160,144,160.f/144.f,60,48000};}
bool retro_load_game(const void *game){(void)game;return true;}
void retro_unload_game(void){}
void retro_reset(void){retro_init();}
void retro_set_controller_port_device(unsigned port,unsigned device){(void)port;(void)device;}
void *retro_get_memory_data(unsigned id){(void)id;return NULL;}
size_t retro_get_memory_size(unsigned id){(void)id;return 0;}
size_t retro_serialize_size(void){return sizeof scene;}
bool retro_serialize(void *d,size_t n){if(n<sizeof scene)return false;memcpy(d,&scene,sizeof scene);return true;}
bool retro_unserialize(const void *d,size_t n){if(n<sizeof scene)return false;memcpy(&scene,d,sizeof scene);last=99;return true;}
void retro_run(void){
    ++tick;
    if(input(0,1,0,5))scene=1; /* Down: red, RGB1555. */
    if(input(0,1,0,4))scene=2; /* Up: blue, XRGB8888. */
    if(input(0,1,0,7))scene=0; /* Right: green, RGB565. */
    if(input(0,1,0,8))scene=3; /* A: leave the last image without callbacks. */
    if(scene==3)return;
    if(last==scene){video(NULL,160,144,0);return;}
    unsigned format=scene==0?2:scene==1?0:1;
    env(10,&format);
    size_t bpp=format==1?4:2,pitch=160*bpp+16;
    static uint8_t pixels[144*(160*4+16)];memset(pixels,0xa5,sizeof pixels);
    for(unsigned y=0;y<144;++y)for(unsigned x=0;x<160;++x){
        uint32_t v=scene==0?0x07e0:scene==1?0x7c00:0x000000ff;
        for(size_t b=0;b<bpp;++b)pixels[y*pitch+x*bpp+b]=(uint8_t)(v>>(8*b));
    }
    video(pixels,160,144,pitch);last=scene;
}
