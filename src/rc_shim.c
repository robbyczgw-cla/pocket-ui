/* pocket-ui's glue to rcheevos (vendor/rcheevos, MIT): rc_client and the rapi calls, flattened into plain functions
 * and plain structs for Rust (src/achievements/ffi.rs). Nothing here talks to the network or keeps a thread: requests
 * go out through `pocket_rc_hooks_t.server_call`, which the Rust side answers later with `pocket_rc_respond`.
 *
 * Memory reads go through rcheevos' own libretro helpers (rc_libretro.c): the core's memory map
 * (RETRO_ENVIRONMENT_SET_MEMORY_MAPS) when it has one, else retro_get_memory_data, laid out the way RetroAchievements
 * addresses each console. */

#include "rc_client.h"
#include "rc_hash.h"
#include "rc_api_runtime.h"
#include "rc_api_user.h"
#include "rc_libretro.h"
#include "rc_version.h"

#include <stdlib.h>
#include <string.h>
#include <stdio.h>

/* One achievement, as Rust reads it. Strings belong to rc_client and live as long as the game is loaded (or the
 * list/event they came with): Rust copies them at once. */
typedef struct pocket_rc_achievement {
  uint32_t id;
  uint32_t points;
  const char* title;
  const char* description;
  const char* badge_name;
  const char* badge_url;
  const char* measured_progress;
  float measured_percent;
  int64_t unlock_time;
  uint8_t unlocked;
  uint8_t bucket;
  uint8_t state;
  uint8_t category;
} pocket_rc_achievement_t;

typedef void (*pocket_rc_server_call_fn)(void* userdata, const char* url, const char* post_data, const char* content_type,
                                         rc_client_server_callback_t callback, void* callback_data);
/* type: an RC_CLIENT_EVENT_*, or POCKET_RC_LOGIN_DONE / POCKET_RC_LOAD_DONE with `result` (RC_OK or an error) */
typedef void (*pocket_rc_event_fn)(void* userdata, uint32_t type, int result, const pocket_rc_achievement_t* achievement,
                                   const char* message);
typedef void (*pocket_rc_log_fn)(void* userdata, const char* message);

#define POCKET_RC_LOGIN_DONE 1001
#define POCKET_RC_LOAD_DONE 1002

typedef struct pocket_rc_hooks {
  void* userdata;
  pocket_rc_server_call_fn server_call;
  pocket_rc_event_fn event;
  pocket_rc_log_fn log;
} pocket_rc_hooks_t;

typedef struct pocket_rc {
  pocket_rc_hooks_t hooks;
  rc_libretro_memory_regions_t regions;
  int have_memory;
} pocket_rc_t;

static pocket_rc_t* shim_of(const rc_client_t* client) {
  return (pocket_rc_t*)rc_client_get_userdata(client);
}

static void flatten(const rc_client_achievement_t* a, pocket_rc_achievement_t* out) {
  memset(out, 0, sizeof(*out));
  out->id = a->id;
  out->points = a->points;
  out->title = a->title;
  out->description = a->description;
  out->badge_name = a->badge_name;
  out->badge_url = a->badge_url;
  out->measured_progress = a->measured_progress;
  out->measured_percent = a->measured_percent;
  out->unlock_time = (int64_t)a->unlock_time;
  out->unlocked = a->unlocked;
  out->bucket = a->bucket;
  out->state = a->state;
  out->category = a->category;
}

static uint32_t RC_CCONV read_memory(uint32_t address, uint8_t* buffer, uint32_t num_bytes, rc_client_t* client) {
  pocket_rc_t* p = shim_of(client);
  if (!p || !p->have_memory)
    return 0;
  return rc_libretro_memory_read(&p->regions, address, buffer, num_bytes);
}

static void RC_CCONV server_call(const rc_api_request_t* request, rc_client_server_callback_t callback, void* callback_data,
                                 rc_client_t* client) {
  pocket_rc_t* p = shim_of(client);
  p->hooks.server_call(p->hooks.userdata, request->url, request->post_data, request->content_type, callback, callback_data);
}

static void RC_CCONV event_handler(const rc_client_event_t* event, rc_client_t* client) {
  pocket_rc_t* p = shim_of(client);
  pocket_rc_achievement_t a;
  const char* message = event->server_error ? event->server_error->error_message : NULL;
  if (event->achievement)
    flatten(event->achievement, &a);
  p->hooks.event(p->hooks.userdata, event->type, event->server_error ? event->server_error->result : RC_OK,
                 event->achievement ? &a : NULL, message);
}

static void RC_CCONV log_message(const char* message, const rc_client_t* client) {
  pocket_rc_t* p = shim_of(client);
  if (p && p->hooks.log)
    p->hooks.log(p->hooks.userdata, message);
}

rc_client_t* pocket_rc_create(const pocket_rc_hooks_t* hooks, int verbose) {
  pocket_rc_t* p = (pocket_rc_t*)calloc(1, sizeof(*p));
  rc_client_t* client;
  if (!p)
    return NULL;
  p->hooks = *hooks;
  client = rc_client_create(read_memory, server_call);
  if (!client) {
    free(p);
    return NULL;
  }
  rc_client_set_userdata(client, p);
  rc_client_set_event_handler(client, event_handler);
  rc_client_enable_logging(client, verbose ? RC_CLIENT_LOG_LEVEL_VERBOSE : RC_CLIENT_LOG_LEVEL_INFO, log_message);
  /* Memory is read on the game thread only (rc_client_do_frame / rc_client_idle), never from the network thread. */
  rc_client_set_allow_background_memory_reads(client, 0);
  /* Softcore: save states, rewind and cheats stay usable (docs/user/achievements.md). */
  rc_client_set_hardcore_enabled(client, 0);
  rc_client_set_unofficial_enabled(client, 0);
  return client;
}

void pocket_rc_destroy(rc_client_t* client) {
  pocket_rc_t* p;
  if (!client)
    return;
  p = shim_of(client);
  rc_client_destroy(client);
  free(p);
}

void pocket_rc_respond(rc_client_server_callback_t callback, void* callback_data, const char* body, size_t body_length,
                       int http_status_code) {
  rc_api_server_response_t response;
  response.body = body;
  response.body_length = body_length;
  response.http_status_code = http_status_code;
  callback(&response, callback_data);
}

/* ---- memory ---------------------------------------------------------------------------------------------------- */

typedef void* (*pocket_rc_memory_data_fn)(unsigned id);
typedef size_t (*pocket_rc_memory_size_fn)(unsigned id);

/* rc_libretro asks for the core's memory without a user pointer; one core per process. */
static pocket_rc_memory_data_fn g_memory_data;
static pocket_rc_memory_size_fn g_memory_size;

static void RC_CCONV core_memory_info(uint32_t id, rc_libretro_core_memory_info_t* info) {
  info->data = g_memory_data ? (uint8_t*)g_memory_data(id) : NULL;
  info->size = (info->data && g_memory_size) ? g_memory_size(id) : 0;
}

/* Points rc_client at the core's memory: its memory map when it gave one (`descriptors`, `count`), else its
 * retro_get_memory_data areas. Returns the number of bytes reachable (0: nothing yet, try again later). */
size_t pocket_rc_init_memory(rc_client_t* client, const struct retro_memory_descriptor* descriptors, unsigned count,
                             pocket_rc_memory_data_fn memory_data, pocket_rc_memory_size_fn memory_size, uint32_t console_id) {
  pocket_rc_t* p = shim_of(client);
  struct retro_memory_map map;
  map.descriptors = descriptors;
  map.num_descriptors = count;
  g_memory_data = memory_data;
  g_memory_size = memory_size;
  p->have_memory = rc_libretro_memory_init(&p->regions, count ? &map : NULL, core_memory_info, console_id);
  return p->have_memory ? p->regions.total_size : 0;
}

int pocket_rc_is_core_allowed(const char* library_name, uint32_t console_id) {
  return rc_libretro_is_system_allowed(library_name, console_id);
}

/* ---- session ---------------------------------------------------------------------------------------------------- */

static void RC_CCONV login_done(int result, const char* error_message, rc_client_t* client, void* userdata) {
  pocket_rc_t* p = shim_of(client);
  (void)userdata;
  p->hooks.event(p->hooks.userdata, POCKET_RC_LOGIN_DONE, result, NULL, error_message);
}

static void RC_CCONV load_done(int result, const char* error_message, rc_client_t* client, void* userdata) {
  pocket_rc_t* p = shim_of(client);
  (void)userdata;
  p->hooks.event(p->hooks.userdata, POCKET_RC_LOAD_DONE, result, NULL, error_message);
}

void pocket_rc_begin_login_with_token(rc_client_t* client, const char* username, const char* token) {
  rc_client_begin_login_with_token(client, username, token, login_done, NULL);
}

void pocket_rc_begin_load_game(rc_client_t* client, const char* hash) {
  rc_client_begin_load_game(client, hash, load_done, NULL);
}

/* The game rc_client knows: id 0 when none is loaded. */
uint32_t pocket_rc_game(rc_client_t* client, const char** title, const char** hash, const char** badge_url) {
  const rc_client_game_t* game = rc_client_get_game_info(client);
  if (!game || !rc_client_is_game_loaded(client))
    return 0;
  *title = game->title;
  *hash = game->hash;
  *badge_url = game->badge_url;
  return game->id;
}

/* out: core achievements, unlocked, points in all, points unlocked, unsupported */
void pocket_rc_summary(rc_client_t* client, uint32_t out[5]) {
  rc_client_user_game_summary_t s;
  memset(&s, 0, sizeof(s));
  rc_client_get_user_game_summary(client, &s);
  out[0] = s.num_core_achievements;
  out[1] = s.num_unlocked_achievements;
  out[2] = s.points_core;
  out[3] = s.points_unlocked;
  out[4] = s.num_unsupported_achievements;
}

const char* pocket_rc_display_name(rc_client_t* client) {
  const rc_client_user_t* user = rc_client_get_user_info(client);
  return user ? user->display_name : NULL;
}

typedef void (*pocket_rc_achievement_sink)(void* userdata, const pocket_rc_achievement_t* achievement);

/* Every core achievement of the game, locked first, then unlocked, then the ones this version can't run. */
void pocket_rc_list(rc_client_t* client, pocket_rc_achievement_sink sink, void* userdata) {
  rc_client_achievement_list_t* list = rc_client_create_achievement_list(client, RC_CLIENT_ACHIEVEMENT_CATEGORY_CORE,
                                                                         RC_CLIENT_ACHIEVEMENT_LIST_GROUPING_LOCK_STATE);
  uint32_t b, i;
  pocket_rc_achievement_t a;
  if (!list)
    return;
  for (b = 0; b < list->num_buckets; ++b) {
    const rc_client_achievement_bucket_t* bucket = &list->buckets[b];
    for (i = 0; i < bucket->num_achievements; ++i) {
      flatten(bucket->achievements[i], &a);
      a.bucket = bucket->bucket_type;
      sink(userdata, &a);
    }
  }
  rc_client_destroy_achievement_list(list);
}

size_t pocket_rc_user_agent_clause(rc_client_t* client, char* buffer, size_t size) {
  return rc_client_get_user_agent_clause(client, buffer, size);
}

const char* pocket_rc_version(void) {
  return RCHEEVOS_VERSION_STRING;
}

/* ---- hashing ---------------------------------------------------------------------------------------------------- */

/* RetroAchievements' hash of a game for `console_id`: from `buffer` when given (a ROM unpacked from a zip, `path` then
 * only names it), else read from `path` (cartridges, and discs through rhash's cue/chd/iso readers). */
int pocket_rc_hash(char out[33], uint32_t console_id, const char* path, const uint8_t* buffer, size_t size) {
  rc_hash_iterator_t iterator;
  int ok;
  rc_hash_initialize_iterator(&iterator, path, buffer, size);
  ok = rc_hash_generate(out, console_id, &iterator);
  rc_hash_destroy_iterator(&iterator);
  return ok;
}

/* ---- one-shot API calls (Settings, the game card, unlocks kept from an earlier session) ------------------------- */

typedef void (*pocket_rc_request_sink)(void* userdata, const char* url, const char* post_data, const char* content_type);

static int hand_over(int result, rc_api_request_t* request, pocket_rc_request_sink sink, void* userdata) {
  if (result == RC_OK)
    sink(userdata, request->url, request->post_data, request->content_type);
  rc_api_destroy_request(request);
  return result;
}

int pocket_rc_login_request(const char* username, const char* password, pocket_rc_request_sink sink, void* userdata) {
  rc_api_login_request_t params;
  rc_api_request_t request;
  memset(&params, 0, sizeof(params));
  params.username = username;
  params.password = password;
  return hand_over(rc_api_init_login_request(&request, &params), &request, sink, userdata);
}

int pocket_rc_resolve_hash_request(const char* hash, pocket_rc_request_sink sink, void* userdata) {
  rc_api_resolve_hash_request_t params;
  rc_api_request_t request;
  memset(&params, 0, sizeof(params));
  params.game_hash = hash;
  return hand_over(rc_api_init_resolve_hash_request(&request, &params), &request, sink, userdata);
}

int pocket_rc_all_progress_request(const char* username, const char* token, uint32_t console_id, pocket_rc_request_sink sink,
                                   void* userdata) {
  rc_api_fetch_all_user_progress_request_t params;
  rc_api_request_t request;
  memset(&params, 0, sizeof(params));
  params.username = username;
  params.api_token = token;
  params.console_id = console_id;
  return hand_over(rc_api_init_fetch_all_user_progress_request(&request, &params), &request, sink, userdata);
}

int pocket_rc_award_request(const char* username, const char* token, uint32_t achievement_id, const char* hash,
                            uint32_t seconds_since_unlock, pocket_rc_request_sink sink, void* userdata) {
  rc_api_award_achievement_request_t params;
  rc_api_request_t request;
  memset(&params, 0, sizeof(params));
  params.username = username;
  params.api_token = token;
  params.achievement_id = achievement_id;
  params.hardcore = 0;
  params.game_hash = hash;
  params.seconds_since_unlock = seconds_since_unlock;
  return hand_over(rc_api_init_award_achievement_request(&request, &params), &request, sink, userdata);
}

static void error_text(const rc_api_response_t* response, int result, char* error, size_t error_size) {
  const char* text = (response && response->error_message && *response->error_message) ? response->error_message : rc_error_str(result);
  if (error && error_size) {
    strncpy(error, text, error_size - 1);
    error[error_size - 1] = '\0';
  }
}

typedef void (*pocket_rc_login_sink)(void* userdata, const char* username, const char* display_name, const char* token,
                                     uint32_t score, uint32_t score_softcore);

int pocket_rc_login_response(const char* body, size_t length, int status, pocket_rc_login_sink sink, void* userdata,
                             char* error, size_t error_size) {
  rc_api_login_response_t response;
  rc_api_server_response_t server;
  int result;
  server.body = body;
  server.body_length = length;
  server.http_status_code = status;
  memset(&response, 0, sizeof(response));
  result = rc_api_process_login_server_response(&response, &server);
  if (result == RC_OK && !response.response.succeeded)
    result = RC_API_FAILURE;
  if (result == RC_OK && (!response.api_token || !*response.api_token))
    result = RC_INVALID_JSON;
  if (result == RC_OK)
    sink(userdata, response.username, response.display_name, response.api_token, response.score, response.score_softcore);
  else
    error_text(&response.response, result, error, error_size);
  rc_api_destroy_login_response(&response);
  return result;
}

int pocket_rc_resolve_hash_response(const char* body, size_t length, int status, uint32_t* game_id, char* error,
                                    size_t error_size) {
  rc_api_resolve_hash_response_t response;
  rc_api_server_response_t server;
  int result;
  server.body = body;
  server.body_length = length;
  server.http_status_code = status;
  memset(&response, 0, sizeof(response));
  result = rc_api_process_resolve_hash_server_response(&response, &server);
  if (result == RC_OK && !response.response.succeeded)
    result = RC_API_FAILURE;
  if (result == RC_OK)
    *game_id = response.game_id;
  else
    error_text(&response.response, result, error, error_size);
  rc_api_destroy_resolve_hash_response(&response);
  return result;
}

typedef void (*pocket_rc_progress_sink)(void* userdata, uint32_t game_id, uint32_t achievements, uint32_t unlocked,
                                        uint32_t unlocked_hardcore);

int pocket_rc_all_progress_response(const char* body, size_t length, int status, pocket_rc_progress_sink sink, void* userdata,
                                    char* error, size_t error_size) {
  rc_api_fetch_all_user_progress_response_t response;
  rc_api_server_response_t server;
  uint32_t i;
  int result;
  server.body = body;
  server.body_length = length;
  server.http_status_code = status;
  memset(&response, 0, sizeof(response));
  result = rc_api_process_fetch_all_user_progress_server_response(&response, &server);
  if (result == RC_OK && !response.response.succeeded)
    result = RC_API_FAILURE;
  if (result == RC_OK) {
    for (i = 0; i < response.num_entries; ++i) {
      const rc_api_all_user_progress_entry_t* e = &response.entries[i];
      sink(userdata, e->game_id, e->num_achievements, e->num_unlocked_achievements, e->num_unlocked_achievements_hardcore);
    }
  }
  else {
    error_text(&response.response, result, error, error_size);
  }
  rc_api_destroy_fetch_all_user_progress_response(&response);
  return result;
}

int pocket_rc_award_response(const char* body, size_t length, int status, char* error, size_t error_size) {
  rc_api_award_achievement_response_t response;
  rc_api_server_response_t server;
  int result;
  server.body = body;
  server.body_length = length;
  server.http_status_code = status;
  memset(&response, 0, sizeof(response));
  result = rc_api_process_award_achievement_server_response(&response, &server);
  if (result == RC_OK && !response.response.succeeded)
    result = RC_API_FAILURE;
  if (result != RC_OK)
    error_text(&response.response, result, error, error_size);
  rc_api_destroy_award_achievement_response(&response);
  return result;
}

const char* pocket_rc_error_str(int result) {
  return rc_error_str(result);
}
