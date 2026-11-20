/* Same rule-test core as every other journey, with DSperate's bounded envelope.
 * Only the libretro contract is simulated. Real engine tests use dsperate_core.cpp. */
#include <stdio.h>
#include <stdlib.h>
#define retro_get_system_info stub_get_system_info
#define retro_serialize_size stub_serialize_size
#define retro_serialize stub_serialize
#define retro_unserialize stub_unserialize
#include "../../scripts/crash-sweep/stub-core.c"
#undef retro_get_system_info
#undef retro_serialize_size
#undef retro_serialize
#undef retro_unserialize

void retro_get_system_info(struct info *i) { stub_get_system_info(i); i->name="DSperate"; }
size_t retro_serialize_size(void) { return stub_serialize_size()+8; }
bool retro_serialize(void *data,size_t size) {
    if(size<retro_serialize_size())return false;
    uint32_t payload=(uint32_t)stub_serialize_size()+4;
    memcpy(data,&payload,4);memcpy((uint8_t*)data+4,"DSST",4);
    return stub_serialize((uint8_t*)data+8,size-8);
}
bool retro_unserialize(const void *data,size_t size) {
    if(size<8||memcmp((const uint8_t*)data+4,"DSST",4))return false;
    uint32_t payload;memcpy(&payload,data,4);
    bool ok=payload>=4&&payload<=size-4&&stub_unserialize((const uint8_t*)data+8,payload-4);
    const char* receipt=getenv("POCKET_TEST_DS_RESUMED");
    if(ok&&receipt){FILE* f=fopen(receipt,"w");if(f){fprintf(f,"%u\n",frame);fclose(f);}}
    return ok;
}
