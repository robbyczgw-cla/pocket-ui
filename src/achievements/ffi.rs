//! The C side of RetroAchievements: rcheevos (vendor/rcheevos) through pocket-ui's shim (src/rc_shim.c). Every type
//! here mirrors a struct of the shim, not of rcheevos itself, so a newer rcheevos only touches the shim.

use std::ffi::{c_char, c_int, c_uint, c_void, CStr};

/// rc_client_t, opaque.
#[repr(C)]
pub struct RcClient {
    _private: [u8; 0],
}

/// rc_client_server_callback_t: how rc_client hears the answer to one of its requests.
pub type ServerCallback = unsafe extern "C" fn(response: *const c_void, callback_data: *mut c_void);

/// pocket_rc_achievement_t
#[repr(C)]
pub struct RawAchievement {
    pub id: u32,
    pub points: u32,
    pub title: *const c_char,
    pub description: *const c_char,
    pub badge_name: *const c_char,
    pub badge_url: *const c_char,
    pub measured_progress: *const c_char,
    pub measured_percent: f32,
    pub unlock_time: i64,
    pub unlocked: u8,
    pub bucket: u8,
    pub state: u8,
    pub category: u8,
}

pub type ServerCallFn = unsafe extern "C" fn(
    userdata: *mut c_void,
    url: *const c_char,
    post_data: *const c_char,
    content_type: *const c_char,
    callback: ServerCallback,
    callback_data: *mut c_void,
);
pub type EventFn = unsafe extern "C" fn(userdata: *mut c_void, kind: u32, result: c_int, achievement: *const RawAchievement, message: *const c_char);
pub type LogFn = unsafe extern "C" fn(userdata: *mut c_void, message: *const c_char);

/// pocket_rc_hooks_t
#[repr(C)]
pub struct Hooks {
    pub userdata: *mut c_void,
    pub server_call: ServerCallFn,
    pub event: EventFn,
    pub log: LogFn,
}

/// The shim's own event kinds next to rc_client's RC_CLIENT_EVENT_*.
pub const LOGIN_DONE: u32 = 1001;
pub const LOAD_DONE: u32 = 1002;

/// RC_CLIENT_EVENT_* that pocket-core reacts to.
pub const EVENT_ACHIEVEMENT_TRIGGERED: u32 = 1;
pub const EVENT_PROGRESS_SHOW: u32 = 7;
pub const EVENT_PROGRESS_UPDATE: u32 = 9;
pub const EVENT_GAME_COMPLETED: u32 = 15;
pub const EVENT_SERVER_ERROR: u32 = 16;
pub const EVENT_DISCONNECTED: u32 = 17;
pub const EVENT_RECONNECTED: u32 = 18;

/// RC_OK and the results pocket-ui tells apart (rc_error.h).
pub const RC_OK: c_int = 0;
pub const RC_API_FAILURE: c_int = -27;
pub const RC_NO_GAME_LOADED: c_int = -29;
pub const RC_NO_RESPONSE: c_int = -32;
pub const RC_ACCESS_DENIED: c_int = -33;
pub const RC_INVALID_CREDENTIALS: c_int = -34;
pub const RC_EXPIRED_TOKEN: c_int = -35;

/// rc_api_server_response_t's status for "the request never reached the server, try again" (rc_api_request.h).
pub const RETRYABLE_CLIENT_ERROR: c_int = -2;

/// RC_CLIENT_ACHIEVEMENT_BUCKET_*
pub const BUCKET_LOCKED: u8 = 1;
pub const BUCKET_UNLOCKED: u8 = 2;
pub const BUCKET_UNSUPPORTED: u8 = 3;

/// One libretro memory descriptor, `struct retro_memory_descriptor` (the pointers as plain addresses: see
/// `libretro::MemoryDescriptor`, which has the same layout).
pub type MemoryDataFn = unsafe extern "C" fn(id: c_uint) -> *mut c_void;
pub type MemorySizeFn = unsafe extern "C" fn(id: c_uint) -> usize;

pub type RequestSink = unsafe extern "C" fn(userdata: *mut c_void, url: *const c_char, post_data: *const c_char, content_type: *const c_char);
pub type LoginSink =
    unsafe extern "C" fn(userdata: *mut c_void, username: *const c_char, display_name: *const c_char, token: *const c_char, score: u32, softcore: u32);
pub type ProgressSink = unsafe extern "C" fn(userdata: *mut c_void, game_id: u32, achievements: u32, unlocked: u32, unlocked_hardcore: u32);
pub type AchievementSink = unsafe extern "C" fn(userdata: *mut c_void, achievement: *const RawAchievement);

extern "C" {
    pub fn pocket_rc_create(hooks: *const Hooks, verbose: c_int) -> *mut RcClient;
    pub fn pocket_rc_destroy(client: *mut RcClient);
    pub fn pocket_rc_respond(callback: ServerCallback, callback_data: *mut c_void, body: *const c_char, len: usize, status: c_int);
    pub fn pocket_rc_init_memory(
        client: *mut RcClient,
        descriptors: *const c_void,
        count: c_uint,
        data: Option<MemoryDataFn>,
        size: Option<MemorySizeFn>,
        console_id: u32,
    ) -> usize;
    pub fn pocket_rc_is_core_allowed(library_name: *const c_char, console_id: u32) -> c_int;
    pub fn pocket_rc_begin_login_with_token(client: *mut RcClient, username: *const c_char, token: *const c_char);
    pub fn pocket_rc_begin_load_game(client: *mut RcClient, hash: *const c_char);
    pub fn pocket_rc_game(client: *mut RcClient, title: *mut *const c_char, hash: *mut *const c_char, badge_url: *mut *const c_char) -> u32;
    pub fn pocket_rc_summary(client: *mut RcClient, out: *mut u32);
    pub fn pocket_rc_display_name(client: *mut RcClient) -> *const c_char;
    pub fn pocket_rc_list(client: *mut RcClient, sink: AchievementSink, userdata: *mut c_void);
    pub fn pocket_rc_user_agent_clause(client: *mut RcClient, buffer: *mut c_char, size: usize) -> usize;
    pub fn pocket_rc_version() -> *const c_char;
    pub fn pocket_rc_hash(out: *mut c_char, console_id: u32, path: *const c_char, buffer: *const u8, size: usize) -> c_int;
    pub fn pocket_rc_login_request(username: *const c_char, password: *const c_char, sink: RequestSink, userdata: *mut c_void) -> c_int;
    pub fn pocket_rc_resolve_hash_request(hash: *const c_char, sink: RequestSink, userdata: *mut c_void) -> c_int;
    pub fn pocket_rc_all_progress_request(username: *const c_char, token: *const c_char, console_id: u32, sink: RequestSink, userdata: *mut c_void) -> c_int;
    pub fn pocket_rc_award_request(
        username: *const c_char,
        token: *const c_char,
        achievement_id: u32,
        hash: *const c_char,
        seconds_since_unlock: u32,
        sink: RequestSink,
        userdata: *mut c_void,
    ) -> c_int;
    pub fn pocket_rc_login_response(
        body: *const c_char,
        len: usize,
        status: c_int,
        sink: LoginSink,
        userdata: *mut c_void,
        error: *mut c_char,
        error_size: usize,
    ) -> c_int;
    pub fn pocket_rc_resolve_hash_response(body: *const c_char, len: usize, status: c_int, game_id: *mut u32, error: *mut c_char, error_size: usize) -> c_int;
    pub fn pocket_rc_all_progress_response(
        body: *const c_char,
        len: usize,
        status: c_int,
        sink: ProgressSink,
        userdata: *mut c_void,
        error: *mut c_char,
        error_size: usize,
    ) -> c_int;
    pub fn pocket_rc_award_response(body: *const c_char, len: usize, status: c_int, error: *mut c_char, error_size: usize) -> c_int;
    pub fn rc_client_do_frame(client: *mut RcClient);
    pub fn rc_client_idle(client: *mut RcClient);
    pub fn rc_client_reset(client: *mut RcClient);
    pub fn rc_client_is_game_loaded(client: *const RcClient) -> c_int;
    pub fn pocket_rc_error_str(result: c_int) -> *const c_char;
}

/// A C string as an owned String; empty for NULL.
pub(crate) fn text(p: *const c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
    }
}

/// rcheevos' words for a result code.
pub fn error_str(result: c_int) -> String {
    text(unsafe { pocket_rc_error_str(result) })
}

/// The version of the rcheevos built in ("12.5.0").
pub fn version() -> String {
    text(unsafe { pocket_rc_version() })
}
