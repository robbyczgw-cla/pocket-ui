//! Activity bridge. A global JNI reference survives worker attachment; no borrowed Activity crosses threads.
use jni::{objects::{GlobalRef, JObject, JString, JValue}, JavaVM};
use std::{path::PathBuf, sync::{OnceLock, RwLock}};

pub struct Bridge { vm: JavaVM, activity: RwLock<GlobalRef> }
static BRIDGE: OnceLock<Bridge> = OnceLock::new();

/// # Safety
/// Both pointers must identify the current live JavaVM and Activity; the Activity reference must remain valid during this call.
pub unsafe fn initialize(vm: *mut jni::sys::JavaVM, activity: jni::sys::jobject) -> Result<(), String> {
    let vm = JavaVM::from_raw(vm).map_err(|e| e.to_string())?;
    let global = {
        let env = vm.attach_current_thread().map_err(|e| e.to_string())?;
        env.new_global_ref(JObject::from_raw(activity)).map_err(|e| e.to_string())?
    };
    if let Some(bridge) = BRIDGE.get() {
        *bridge.activity.write().unwrap_or_else(|e| e.into_inner()) = global;
        Ok(())
    } else {
        BRIDGE.set(Bridge { vm, activity: RwLock::new(global) }).map_err(|_| "bridge initialization raced".to_string())
    }
}

/// Every invocation has its own local frame, including on long-lived attached worker threads.
pub fn strings(name: &str, args: &[&str]) -> Result<String, String> {
    let bridge = BRIDGE.get().ok_or("Activity not ready")?;
    let activity = bridge.activity.read().unwrap_or_else(|e| e.into_inner()).clone();
    let mut env = bridge.vm.attach_current_thread().map_err(|e| e.to_string())?;
    let result = env.with_local_frame(32, |env| -> jni::errors::Result<String> {
        let objects = args.iter().map(|s| env.new_string(s).map(JObject::from)).collect::<Result<Vec<_>, _>>()?;
        let values: Vec<_> = objects.iter().map(JValue::Object).collect();
        let sig = format!("({})Ljava/lang/String;", "Ljava/lang/String;".repeat(args.len()));
        let object = env.call_method(activity.as_obj(), name, &sig, &values)?.l()?;
        let result: String = env.get_string(&JString::from(object))?.into();
        Ok(result)
    });
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
        return Err(format!("Android {name} failed"));
    }
    result.map_err(|e| e.to_string())
}
pub fn string(name: &str) -> String { strings(name, &[]).unwrap_or_default() }
pub fn data_dir() -> PathBuf { PathBuf::from(string("dataPath")) }
pub fn is_speaker() -> bool { string("speakerState") == "true" }
pub fn sink_volume() -> Option<(u32, bool)> {
    let state = string("volumeState");
    let (volume, muted) = state.split_once(',')?;
    Some((volume.parse().ok()?, muted == "true"))
}

/// Wikipedia goes through the same Android HTTPS transport and bounded worker request as covers.
pub fn wiki_json(url: &str) -> Result<Vec<u8>, String> {
    let result = strings("httpGet", &[url, "", "1048576"])?;
    result.strip_prefix("OK\n").map(|s|s.as_bytes().to_vec()).ok_or_else(||"offline".into())
}

/// A core describes its controls on Android; the standard RetroPad labels are the fallback.
pub fn console_buttons(_system:&str)->&'static [(u8,&'static str)] {&[]}

/// The screenshot action belongs to the host input model; shared box painting stays platform-neutral.
pub fn screenshot_hint() -> &'static str { "No pictures yet. Take one from the game's pause menu." }
