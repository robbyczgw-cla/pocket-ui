//! The companion's threads: a supervisor that opens the listening sockets the address plan asks for (`net::plan`),
//! sleeps in `poll` until a browser connects (or 30 s pass, to notice a new Tailscale address), checks that each
//! connection comes from a Tailscale address and hands it to one of three workers. All four run at the lowest priority
//! (`web::lower_priority`). Stopping wakes the supervisor through a pipe; the sockets close at once, a request under
//! way finishes on its worker.

use std::net::TcpListener;
use std::os::fd::{AsRawFd, RawFd};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::api::{self, Conn, Ctx};
use super::net::{self, Listen};
use super::{lower_priority, Shared, PORT};

/// Workers: one upload and two browsers' page loads at once.
pub const WORKERS: usize = 3;
/// Connections waiting for a worker; more are told to come back.
const QUEUE: usize = 24;
/// How often the supervisor looks at the addresses while nobody connects.
const RECHECK: Duration = Duration::from_secs(30);

pub struct Server {
    stop: Arc<AtomicBool>,
    wake: RawFd,
    thread: Option<std::thread::JoinHandle<()>>,
    /// Says that the supervisor has ended, so `stop` waits for it a bounded time only.
    done: Mutex<Receiver<()>>,
}

/// How long `stop` waits for the supervisor before it lets it end on its own (it never blocks, review F3; this is the
/// bound should it ever).
const STOP_WAIT: Duration = Duration::from_secs(1);

impl Server {
    /// Starts the supervisor and the workers. The listening addresses come from `net::plan` (or `fixed`, for tests
    /// and the development server: those addresses as they are, without interface binding).
    pub fn start(mut ctx: Ctx, fixed: Option<Vec<Listen>>) -> std::io::Result<Server> {
        let mut fds = [0 as libc::c_int; 2];
        // SAFETY: pipe fills two descriptors that this server owns and closes.
        if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        for fd in fds {
            // SAFETY: fcntl on our own descriptors.
            unsafe {
                libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
                libc::fcntl(fd, libc::F_SETFL, libc::fcntl(fd, libc::F_GETFL) | libc::O_NONBLOCK);
            }
        }
        let stop = Arc::new(AtomicBool::new(false));
        // Workers and uploads look at the same flag: switched off, nothing queued starts and an upload stops.
        ctx.stopped = stop.clone();
        let ctx = Arc::new(ctx);
        let (tx, rx) = sync_channel::<(std::net::TcpStream, Conn)>(QUEUE);
        let rx = Arc::new(Mutex::new(rx));
        for i in 0..WORKERS {
            let (ctx, rx) = (ctx.clone(), rx.clone());
            std::thread::Builder::new().name(format!("web-{i}")).spawn(move || worker(&ctx, &rx))?;
        }
        let (stop2, wake_r) = (stop.clone(), fds[0]);
        let (done_tx, done) = std::sync::mpsc::channel();
        let thread = std::thread::Builder::new().name("web".into()).spawn(move || {
            supervise(&ctx, tx, &stop2, wake_r, fixed);
            // SAFETY: the read end belongs to this thread from here on.
            unsafe { libc::close(wake_r) };
            let _ = done_tx.send(());
        })?;
        Ok(Server { stop, wake: fds[1], thread: Some(thread), done: Mutex::new(done) })
    }

    /// Closes the sockets and waits for the supervisor, at most [`STOP_WAIT`] (never for a worker: a request under
    /// way ends at its next read, an upload at its next chunk, without keeping anything).
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // SAFETY: a one-byte write to our own pipe.
        unsafe { libc::write(self.wake, b"x".as_ptr() as *const libc::c_void, 1) };
        if let Some(t) = self.thread.take() {
            if self.done.lock().unwrap_or_else(|e| e.into_inner()).recv_timeout(STOP_WAIT).is_ok() {
                let _ = t.join();
            } else {
                eprintln!("web: the server did not stop within {STOP_WAIT:?}; it ends by itself");
            }
        }
        if self.wake >= 0 {
            // SAFETY: our write end, closed once.
            unsafe { libc::close(self.wake) };
            self.wake = -1;
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn worker(ctx: &Ctx, rx: &Mutex<Receiver<(std::net::TcpStream, Conn)>>) {
    lower_priority();
    loop {
        let job = rx.lock().unwrap_or_else(|e| e.into_inner()).recv();
        match job {
            // Switched off: what still waits in the queue is closed unanswered (review F7).
            Ok(_) if ctx.stopped.load(Ordering::Relaxed) => {}
            Ok((stream, conn)) => api::handle(ctx, &conn, stream),
            Err(_) => return,
        }
    }
}

/// "Busy" to a connection the queue has no room for, from the supervisor: one try without waiting, then the socket
/// closes. A client that reads nothing can't hold the supervisor (review F3).
pub fn reply_busy(mut stream: std::net::TcpStream) {
    let _ = stream.set_nonblocking(true);
    let _ = super::http::write_response(&mut stream, &super::http::Response::error(503, "Busy. Try again in a moment.").with("Retry-After", "2"), false);
}

/// Why a listening socket could not be opened, as Settings > Connections > Web companion says it (the next try is 30 s later).
pub fn why_not(e: &std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::AddrInUse => "Something else on the Pocket is using the address.".to_string(),
        std::io::ErrorKind::PermissionDenied => format!("The system did not allow port {PORT}."),
        _ => format!("It could not listen: {e}."),
    }
}

fn supervise(ctx: &Ctx, tx: SyncSender<(std::net::TcpStream, Conn)>, stop: &AtomicBool, wake: RawFd, fixed: Option<Vec<Listen>>) {
    lower_priority();
    let shared: &Shared = &ctx.shared;
    let mut open: Vec<(Listen, TcpListener)> = Vec::new();
    let mut failed: Vec<Listen> = Vec::new();
    let mut refused = 0u64;
    while !stop.load(Ordering::Relaxed) {
        let want = fixed.clone().unwrap_or_else(|| net::plan(&net::interfaces(), PORT));
        let now: Vec<Listen> = open.iter().map(|(l, _)| l.clone()).collect();
        if now != want {
            open.retain(|(l, _)| want.contains(l));
            for l in &want {
                if open.iter().any(|(o, _)| o == l) {
                    continue;
                }
                let bound = if fixed.is_some() { TcpListener::bind(l.addr) } else { net::bind(l) };
                match bound.and_then(|s| s.set_nonblocking(true).map(|()| s)) {
                    Ok(s) => {
                        println!("web: listening on {} ({})", l.iface, if l.addr.is_ipv4() { "IPv4" } else { "IPv6" });
                        open.push((l.clone(), s));
                    }
                    Err(e) if !failed.contains(l) => {
                        eprintln!("web: can't listen on {}: {e}", l.iface);
                        failed.push(l.clone());
                        *shared.problem.lock().unwrap() = Some(why_not(&e));
                    }
                    Err(_) => {}
                }
            }
            failed.retain(|l| want.contains(l));
            if !open.is_empty() || want.is_empty() {
                *shared.problem.lock().unwrap() = None;
            }
            let mut addrs: Vec<std::net::SocketAddr> = open.iter().filter_map(|(_, s)| s.local_addr().ok()).collect();
            addrs.sort();
            *shared.listening.lock().unwrap() = addrs;
            shared.notify();
        }
        // The Pocket's tailnet name for the Settings page, once.
        if shared.tailnet_name.lock().unwrap().is_none() && !open.is_empty() {
            let _ = ctx.me();
            shared.notify();
        }
        let mut fds: Vec<libc::pollfd> = open.iter().map(|(_, s)| libc::pollfd { fd: s.as_raw_fd(), events: libc::POLLIN, revents: 0 }).collect();
        fds.push(libc::pollfd { fd: wake, events: libc::POLLIN, revents: 0 });
        // SAFETY: poll over descriptors that stay open for the call.
        let n = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, RECHECK.as_millis() as libc::c_int) };
        if n <= 0 {
            continue;
        }
        for (i, (l, s)) in open.iter().enumerate() {
            if fds[i].revents & libc::POLLIN == 0 {
                continue;
            }
            while let Ok((stream, peer)) = s.accept() {
                if !net::peer_allowed(peer.ip()) && fixed.is_none() {
                    refused += 1;
                    if refused.is_power_of_two() {
                        eprintln!("web: refused {refused} connection(s) from outside the tailnet");
                    }
                    continue;
                }
                let _ = stream.set_nonblocking(false);
                let local = stream.local_addr().unwrap_or(l.addr);
                match tx.try_send((stream, Conn { peer, local, accepted: std::time::Instant::now() })) {
                    Ok(()) => {}
                    Err(TrySendError::Full((stream, _))) => reply_busy(stream),
                    Err(TrySendError::Disconnected(_)) => return,
                }
            }
        }
    }
    shared.listening.lock().unwrap().clear();
}
