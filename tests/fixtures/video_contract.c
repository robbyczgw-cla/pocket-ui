/* Special video scenes on the same core used by journeys: every run still emits sound. */
#define WIDTH 160
#define HEIGHT 144
#define FPS 60.0
#define RATE 48000.0
#define NO_SRAM
#define STUB_FULLPATH false
#define retro_init stub_default_init
#define retro_reset stub_default_reset
#define retro_run stub_default_run
#define retro_serialize_size stub_default_size
#define retro_serialize stub_default_serialize
#define retro_unserialize stub_default_unserialize
#include "../../scripts/crash-sweep/stub-core.c"
#undef retro_init
#undef retro_reset
#undef retro_run
#undef retro_serialize_size
#undef retro_serialize
#undef retro_unserialize
static unsigned scene, tick, last = 99;
void retro_init(void){scene=tick=0;last=99;}
void retro_reset(void){retro_init();}
size_t retro_serialize_size(void){return sizeof scene;}
bool retro_serialize(void *d,size_t n){if(n<sizeof scene)return false;memcpy(d,&scene,sizeof scene);return true;}
bool retro_unserialize(const void *d,size_t n){if(n<sizeof scene)return false;memcpy(&scene,d,sizeof scene);last=99;return true;}
void retro_run(void){
    stub_sound();
    ++tick;
    if(input(0,1,0,2)) {
        /* PS1 intro/game and SNES low/high/interlaced modes keep their display aspect. */
        static const struct av modes[] = {
            {{320,240,640,480,4.f/3.f},{60,48000}}, {{512,256,640,480,4.f/3.f},{60,48000}},
            {{640,480,640,480,4.f/3.f},{60,48000}}, {{256,224,640,480,4.f/3.f},{60,48000}},
            {{512,448,640,480,4.f/3.f},{60,48000}}, {{640,360,640,480,16.f/9.f},{60,48000}}
        };
        const struct av *g=&modes[(tick-1)%6];
        env(37,(void *)g);
        unsigned format=1;env(10,&format);
        static uint32_t dynamic[640*480];
        for(unsigned i=0;i<g->g.bw*g->g.bh;++i)dynamic[i]=0x0080c040;
        video(dynamic,g->g.bw,g->g.bh,g->g.bw*4);
        return;
    }
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
