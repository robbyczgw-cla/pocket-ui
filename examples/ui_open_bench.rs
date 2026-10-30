//! Where the first UI sound's time goes when the output opens: dlopen of libasound, snd_pcm_open of the `pipewire`
//! device, snd_pcm_set_params, and the first write returning; each opening after `GAP` seconds of quiet (the sink
//! suspends meanwhile). `KEEP_LIB=1` keeps libasound (and with it the pipewire plugin) loaded between openings.
//!     ui_open_bench [ROUNDS] [GAP_S]
use libloading::Library;
use std::ffi::{c_char, c_int, c_long, c_uint, c_ulong, c_void};
use std::time::{Duration, Instant};

type Open = unsafe extern "C" fn(*mut *mut c_void, *const c_char, c_int, c_int) -> c_int;
type SetParams = unsafe extern "C" fn(*mut c_void, c_int, c_int, c_uint, c_uint, c_int, c_uint) -> c_int;
type Writei = unsafe extern "C" fn(*mut c_void, *const c_void, c_ulong) -> c_long;
type Close = unsafe extern "C" fn(*mut c_void) -> c_int;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let rounds: usize = args.get(1).and_then(|v| v.parse().ok()).unwrap_or(5);
    let gap: f64 = args.get(2).and_then(|v| v.parse().ok()).unwrap_or(4.0);
    let keep = std::env::var_os("KEEP_LIB").is_some();
    let held = keep.then(|| unsafe { Library::new("libasound.so.2") }.unwrap());
    let chunk = vec![0i16; 256];
    for r in 0..rounds {
        std::thread::sleep(Duration::from_secs_f64(gap));
        let t0 = Instant::now();
        let lib = unsafe { Library::new("libasound.so.2") }.unwrap();
        let (open, set, writei, close) = unsafe {
            (*lib.get::<Open>(b"snd_pcm_open").unwrap(), *lib.get::<SetParams>(b"snd_pcm_set_params").unwrap(), *lib.get::<Writei>(b"snd_pcm_writei").unwrap(), *lib.get::<Close>(b"snd_pcm_close").unwrap())
        };
        let t1 = t0.elapsed();
        let mut pcm = std::ptr::null_mut();
        let rc = unsafe { open(&mut pcm, c"pipewire".as_ptr(), 0, 0) };
        let t2 = t0.elapsed();
        let rc2 = unsafe { set(pcm, 2, 3, 2, 48000, 1, 10_000) };
        let t3 = t0.elapsed();
        let mut firsts = Vec::new();
        for _ in 0..6 {
            unsafe { writei(pcm, chunk.as_ptr() as *const c_void, 128) };
            firsts.push(t0.elapsed().as_secs_f64() * 1000.0);
        }
        unsafe { close(pcm) };
        let t5 = t0.elapsed();
        drop(lib);
        println!(
            "round {r}: dlopen {:.1} ms, snd_pcm_open {:.1} ms, set_params {:.1} ms, writes returned at {} ms, close {:.1} ms (rc {rc} {rc2})",
            t1.as_secs_f64() * 1e3,
            (t2 - t1).as_secs_f64() * 1e3,
            (t3 - t2).as_secs_f64() * 1e3,
            firsts.iter().map(|v| format!("{v:.1}")).collect::<Vec<_>>().join("/"),
            (t5 - t3).as_secs_f64() * 1e3 - firsts.last().unwrap() + t3.as_secs_f64() * 1e3,
        );
    }
    drop(held);
}
