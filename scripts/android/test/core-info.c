/* Read-only bundled-core metadata probe. No retro_init/load_game/run calls; no audio/input. */
#include <dlfcn.h>
#include <stdbool.h>
#include <stdio.h>
struct info { const char *name, *version, *extensions; bool need_fullpath, block_extract; };
int main(int argc, char **argv) {
    int errors=0;
    for(int i=1;i<argc;i++) {
        void *lib=dlopen(argv[i],RTLD_LAZY);
        if(!lib) { fprintf(stderr,"%s: %s\n",argv[i],dlerror());errors++;continue; }
        void (*get_info)(struct info *)=dlsym(lib,"retro_get_system_info");
        if(!get_info) { errors++;dlclose(lib);continue; }
        struct info info={0};get_info(&info);
        printf("%s|%s|%s|need_fullpath=%d|block_extract=%d\n",argv[i],info.name,info.extensions,info.need_fullpath,info.block_extract);
        dlclose(lib);
    }
    return errors?1:0;
}
