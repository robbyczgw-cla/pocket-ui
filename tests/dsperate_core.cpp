// Exercises the patched libretro entry points and the engine's real registers.
// ROM and save are private inputs supplied by the caller; no ROM is bundled.
#include <map>
#include "libretro.cpp"
#include <cassert>
#include <fstream>
#include <iterator>

static std::string test_dir;
static std::map<std::string,std::string> values;
static uint16_t buttons;
static int16_t right_x, pointer_x, pointer_y;
static bool pointer_down, updated;
static unsigned width, height, geometries;
static retro_audio_buffer_status_callback_t audio_status;
static unsigned repeated;
static unsigned consecutive_repeats,max_repeats;

static bool environment(unsigned command,void* data) {
  switch(command) {
    case RETRO_ENVIRONMENT_SET_VARIABLES:
      for(auto* p=static_cast<retro_variable*>(data);p->key;++p) {
        std::string list=p->value;auto at=list.find("; ");
        if(at!=std::string::npos)values.emplace(p->key,list.substr(at+2,list.find('|',at+2)-(at+2)));
      }
      return true;
    case RETRO_ENVIRONMENT_GET_VARIABLE: {
      auto* v=static_cast<retro_variable*>(data);auto found=values.find(v->key);
      v->value=found==values.end()?nullptr:found->second.c_str();return v->value;
    }
    case RETRO_ENVIRONMENT_GET_VARIABLE_UPDATE:
      *static_cast<bool*>(data)=updated;updated=false;return true;
    case RETRO_ENVIRONMENT_SET_VARIABLE: {
      if(!data)return true;
      auto* v=static_cast<retro_variable*>(data);values[v->key]=v->value;return true;
    }
    case RETRO_ENVIRONMENT_GET_SYSTEM_DIRECTORY:
    case RETRO_ENVIRONMENT_GET_SAVE_DIRECTORY:
      *static_cast<const char**>(data)=test_dir.c_str();return true;
    case RETRO_ENVIRONMENT_SET_PIXEL_FORMAT:
    case RETRO_ENVIRONMENT_SET_MEMORY_MAPS: return true;
    case RETRO_ENVIRONMENT_SET_GEOMETRY: ++geometries;return true;
    case RETRO_ENVIRONMENT_SET_AUDIO_BUFFER_STATUS_CALLBACK:
      audio_status=static_cast<retro_audio_buffer_status_callback*>(data)->callback;return true;
    default:return false;
  }
}
static int16_t input(unsigned port,unsigned device,unsigned index,unsigned id) {
  if(port)return 0;
  if(device==RETRO_DEVICE_JOYPAD)return id<16?bool(buttons&(1u<<id)):0;
  if(device==RETRO_DEVICE_ANALOG)return index==RETRO_DEVICE_INDEX_ANALOG_RIGHT&&id==0?right_x:0;
  if(device==RETRO_DEVICE_POINTER)return id==0?pointer_x:id==1?pointer_y:id==2?pointer_down:0;
  return 0;
}
static void video(const void* data,unsigned w,unsigned h,size_t pitch){
  width=w;height=h;assert(pitch==w*4);
  if(!data){++repeated;++consecutive_repeats;max_repeats=std::max(max_repeats,consecutive_repeats);}else consecutive_repeats=0;
}
static size_t sound(const int16_t*,size_t frames){return frames;}
static std::vector<uint8_t> read(const char* file){std::ifstream f(file,std::ios::binary);assert(f.good());return {std::istreambuf_iterator<char>(f),{}};}
static void layout(const char* value){values["dsperate_screen_layout"]=value;updated=true;retro_run();}

int main(int argc,char** argv) {
  assert(argc>=3);test_dir=argv[2];
  retro_set_environment(environment);retro_set_input_poll([]{});retro_set_input_state(input);
  retro_set_video_refresh(video);retro_set_audio_sample_batch(sound);retro_init();
  values["dsperate_frameskip"]="auto";
  retro_game_info game{};game.path=argv[1];assert(retro_load_game(&game));
  assert(audio_status);audio_status(false,0,false);
  retro_run();assert(width==256&&height==384);

  // The common Controls mapping emits R2 and right analog, not DS A/d-pad.
  buttons=1u<<RETRO_DEVICE_ID_JOYPAD_R2;retro_run();
  assert(!(nds->io.extkeyin&0x40));assert((nds->io.keyinput&0xf1)==0xf1);
  const auto before_x=nds->io.spi_tsc.x;
  right_x=32767;retro_run();assert(nds->io.spi_tsc.x>before_x);right_x=0;
  buttons=0;retro_run();assert(nds->io.extkeyin&0x40);

  // A held layout action changes once; release permits the next change.
  buttons=1u<<RETRO_DEVICE_ID_JOYPAD_R3;retro_run();assert(layout_id==1);
  assert(values["dsperate_screen_layout"]=="bottom-top");
  retro_run();assert(layout_id==1);
  buttons=0;retro_run();buttons=1u<<RETRO_DEVICE_ID_JOYPAD_R3;retro_run();
  assert(layout_id==2&&width==512&&height==192);buttons=0;

  layout("bottom");pointer_down=true;pointer_x=0;pointer_y=0;retro_run();
  assert(!(nds->io.extkeyin&0x40));assert(cursor_x==128&&cursor_y==96);
  layout("top");assert(nds->io.extkeyin&0x40);
  pointer_down=false;layout("right-left");assert(width==512&&height==192);
  assert(geometries>=5);

  // Same raw SRAM as melonDS; import and state restore use the engine's memory.
  auto* ram=static_cast<uint8_t*>(retro_get_memory_data(RETRO_MEMORY_SAVE_RAM));
  const auto ram_size=retro_get_memory_size(RETRO_MEMORY_SAVE_RAM);assert(ram&&ram_size);
  if(argc>=4){auto save=read(argv[3]);assert(save.size()==ram_size);std::memcpy(ram,save.data(),save.size());}
  std::vector<uint8_t> battery(ram,ram+ram_size);
  auto* memory=static_cast<uint8_t*>(retro_get_memory_data(RETRO_MEMORY_SYSTEM_RAM));
  const auto memory_size=retro_get_memory_size(RETRO_MEMORY_SYSTEM_RAM);
  std::vector<uint8_t> before(memory,memory+memory_size);
  std::vector<uint8_t> state(retro_serialize_size());assert(!state.empty());
  assert(retro_serialize(state.data(),state.size()));
  memory[0x100]^=0xff;ram[0]^=0xff;layout("top-bottom");
  assert(retro_unserialize(state.data(),state.size()));
  memory=static_cast<uint8_t*>(retro_get_memory_data(RETRO_MEMORY_SYSTEM_RAM));
  ram=static_cast<uint8_t*>(retro_get_memory_data(RETRO_MEMORY_SAVE_RAM));
  assert(std::memcmp(memory,before.data(),before.size())==0);
  assert(std::memcmp(ram,battery.data(),battery.size())==0);assert(layout_id==3);
  if(argc>=5){auto foreign=read(argv[4]);assert(!retro_unserialize(foreign.data(),foreign.size()));assert(retro_unserialize(state.data(),state.size()));}
  audio_status(true,0,true);for(unsigned n=0;n<20;++n)retro_run();
  assert(repeated>0&&max_repeats<=1);audio_status(false,100,false);
  retro_unload_game();retro_deinit();
  std::puts("DSperate: real core input, pointer/layout, raw SRAM and savestate roundtrip passed");
}
