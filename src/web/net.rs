//! Where the companion listens and who may connect: the address plan ([`plan`]), the checks of every connection
//! ([`peer_allowed`], [`host_allowed`]), binding to one interface (`SO_BINDTODEVICE`), and tailscaled's LocalAPI
//! (`whois` and this node's own status) over its Unix socket.
//!
//! Binding to the Tailscale address alone is not enough on Linux: with the weak host model a packet for 100.x that
//! arrives on wlan0 is accepted too, so a device on the Wi-Fi with a route to the Pocket could reach it. Each
//! listening socket is therefore tied to its interface (`tailscale0`, `SO_BINDTODEVICE`), and each accepted
//! connection must come from a Tailscale address. There is no switch for any other network.

use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// tailscaled's socket on ROCKNIX (`--socket=` in tailscaled.service).
pub const TAILSCALED_SOCKET: &str = "/run/tailscale/tailscaled.sock";

/// 100.64.0.0/10 (Tailscale's IPv4 addresses, the CGNAT range) or fd7a:115c:a1e0::/48 (its IPv6 range).
pub fn is_tailnet(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            o[0] == 100 && (o[1] & 0xc0) == 64
        }
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => is_tailnet(IpAddr::V4(v4)),
            None => {
                let s = v6.segments();
                s[0] == 0xfd7a && s[1] == 0x115c && s[2] == 0xa1e0
            }
        },
    }
}

/// One address of one network interface.
#[derive(Clone, Debug, PartialEq)]
pub struct Iface {
    pub name: String,
    pub addr: IpAddr,
    /// The kernel's index of the interface: a `tailscale0` made anew (tailscaled restarted) has another one.
    pub index: u32,
}

/// A socket to open: this address, tied to this interface.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Listen {
    pub addr: SocketAddr,
    pub iface: String,
    /// The interface's index when the plan was made: part of the comparison, so that a socket tied to an interface
    /// that was made anew with the same name and address is opened again (review F13). 0 for fixed addresses.
    pub index: u32,
}

/// Where to listen, given the interfaces: only Tailscale's addresses on an interface called `tailscale…` (a CGNAT
/// address handed out by a phone hotspot on wlan0 is in the same range and must not count). Never an unspecified or
/// loopback address, never the Wi-Fi.
pub fn plan(ifaces: &[Iface], port: u16) -> Vec<Listen> {
    let mut out: Vec<Listen> = Vec::new();
    for i in ifaces {
        if i.addr.is_unspecified() || i.addr.is_loopback() || !i.name.starts_with("tailscale") || !is_tailnet(i.addr) {
            continue;
        }
        let l = Listen { addr: SocketAddr::new(i.addr, port), iface: i.name.clone(), index: i.index };
        if !out.contains(&l) {
            out.push(l);
        }
    }
    out
}

/// Whether a connection from `peer` may be served: only from a Tailscale address (a device on the Wi-Fi that
/// routes a packet for 100.x to the Pocket is refused even before the interface binding would drop it).
pub fn peer_allowed(peer: IpAddr) -> bool {
    is_tailnet(peer)
}

/// Whether the browser asked for one of the Pocket's own names or addresses (`host` as `Request::host` gives it).
/// Anything else is a page of another site that made its name point at the Pocket (DNS rebinding).
pub fn host_allowed(host: &str, names: &[String], listening: &[SocketAddr]) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if let Ok(ip) = host.parse::<IpAddr>() {
        return listening.iter().any(|a| a.ip() == ip);
    }
    names.iter().any(|n| !n.is_empty() && n.trim_end_matches('.').eq_ignore_ascii_case(&host))
}

/// The machine's interfaces and their addresses (`getifaddrs`).
pub fn interfaces() -> Vec<Iface> {
    let mut out = Vec::new();
    let mut head: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: getifaddrs fills `head` with a list we only read and then free with freeifaddrs.
    unsafe {
        if libc::getifaddrs(&mut head) != 0 {
            return out;
        }
        let mut cur = head;
        while !cur.is_null() {
            let ifa = &*cur;
            cur = ifa.ifa_next;
            if ifa.ifa_addr.is_null() || ifa.ifa_name.is_null() {
                continue;
            }
            let name = std::ffi::CStr::from_ptr(ifa.ifa_name).to_string_lossy().into_owned();
            let addr = match (*ifa.ifa_addr).sa_family as i32 {
                libc::AF_INET => {
                    let sin = &*(ifa.ifa_addr as *const libc::sockaddr_in);
                    IpAddr::V4(Ipv4Addr::from(u32::from_be(sin.sin_addr.s_addr)))
                }
                libc::AF_INET6 => {
                    let sin6 = &*(ifa.ifa_addr as *const libc::sockaddr_in6);
                    IpAddr::V6(Ipv6Addr::from(sin6.sin6_addr.s6_addr))
                }
                _ => continue,
            };
            // SAFETY: if_nametoindex reads the NUL-terminated name getifaddrs gave.
            let index = libc::if_nametoindex(ifa.ifa_name);
            out.push(Iface { name, addr, index });
        }
        libc::freeifaddrs(head);
    }
    out
}

/// A listening socket on `l.addr`, tied to `l.iface` so that nothing arriving on another interface reaches it.
#[cfg(target_os = "linux")]
pub fn bind(l: &Listen) -> std::io::Result<TcpListener> {
    use std::os::fd::FromRawFd;
    let family = if l.addr.is_ipv4() { libc::AF_INET } else { libc::AF_INET6 };
    // SAFETY: plain socket calls on a descriptor this function owns until it is handed to TcpListener (or closed).
    unsafe {
        let fd = libc::socket(family, libc::SOCK_STREAM | libc::SOCK_CLOEXEC, 0);
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        let fail = |fd: i32| {
            let e = std::io::Error::last_os_error();
            libc::close(fd);
            Err(e)
        };
        let one: libc::c_int = 1;
        let opt = |level, name, v: &libc::c_int| libc::setsockopt(fd, level, name, v as *const _ as *const libc::c_void, std::mem::size_of::<libc::c_int>() as libc::socklen_t);
        if opt(libc::SOL_SOCKET, libc::SO_REUSEADDR, &one) != 0 {
            return fail(fd);
        }
        if family == libc::AF_INET6 && opt(libc::IPPROTO_IPV6, libc::IPV6_V6ONLY, &one) != 0 {
            return fail(fd);
        }
        let dev = std::ffi::CString::new(l.iface.as_bytes()).map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
        if libc::setsockopt(fd, libc::SOL_SOCKET, libc::SO_BINDTODEVICE, dev.as_ptr() as *const libc::c_void, dev.as_bytes_with_nul().len() as libc::socklen_t) != 0 {
            return fail(fd);
        }
        let rc = match l.addr {
            SocketAddr::V4(a) => {
                let mut sin: libc::sockaddr_in = std::mem::zeroed();
                sin.sin_family = libc::AF_INET as libc::sa_family_t;
                sin.sin_port = a.port().to_be();
                sin.sin_addr.s_addr = u32::from(*a.ip()).to_be();
                libc::bind(fd, &sin as *const _ as *const libc::sockaddr, std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t)
            }
            SocketAddr::V6(a) => {
                let mut sin6: libc::sockaddr_in6 = std::mem::zeroed();
                sin6.sin6_family = libc::AF_INET6 as libc::sa_family_t;
                sin6.sin6_port = a.port().to_be();
                sin6.sin6_addr.s6_addr = a.ip().octets();
                libc::bind(fd, &sin6 as *const _ as *const libc::sockaddr, std::mem::size_of::<libc::sockaddr_in6>() as libc::socklen_t)
            }
        };
        if rc != 0 || libc::listen(fd, 16) != 0 {
            return fail(fd);
        }
        Ok(TcpListener::from_raw_fd(fd))
    }
}

/// Elsewhere (a desktop, for development) the address alone.
#[cfg(not(target_os = "linux"))]
pub fn bind(l: &Listen) -> std::io::Result<TcpListener> {
    TcpListener::bind(l.addr)
}

// --- tailscaled's LocalAPI ----------------------------------------------------------------------------------------

/// This node as tailscaled describes it (`/localapi/v0/status?peers=false`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SelfNode {
    /// The owner's user id; 0 when unknown.
    pub user_id: i64,
    /// The owner's login ("alex@example.com").
    pub login: String,
    /// A tagged node belongs to no user: nobody is its owner, so only pairing gets in.
    pub tagged: bool,
    pub hostname: String,
    /// The MagicDNS name without the trailing dot ("pocket.tail1234.ts.net").
    pub dns_name: String,
}

impl SelfNode {
    pub fn parse(json: &str) -> Option<SelfNode> {
        let v: serde_json::Value = serde_json::from_str(json).ok()?;
        let s = v.get("Self")?;
        let user_id = s.get("UserID").and_then(|u| u.as_i64()).unwrap_or(0);
        let login = v
            .get("User")
            .and_then(|u| u.get(user_id.to_string()))
            .and_then(|u| u.get("LoginName"))
            .and_then(|l| l.as_str())
            .unwrap_or_default()
            .to_string();
        Some(SelfNode {
            user_id,
            login,
            tagged: s.get("Tags").and_then(|t| t.as_array()).is_some_and(|t| !t.is_empty()),
            hostname: s.get("HostName").and_then(|h| h.as_str()).unwrap_or_default().to_string(),
            dns_name: s.get("DNSName").and_then(|h| h.as_str()).unwrap_or_default().trim_end_matches('.').to_string(),
        })
    }

    /// The names a browser may use for the Pocket: the host name, the first part of the MagicDNS name, the full name.
    pub fn names(&self) -> Vec<String> {
        let mut out = vec![self.hostname.to_ascii_lowercase(), self.dns_name.to_ascii_lowercase()];
        if let Some(first) = self.dns_name.split('.').next() {
            out.push(first.to_ascii_lowercase());
        }
        let mut seen = Vec::new();
        out.retain(|n| !n.is_empty() && !seen.contains(n) && {
            seen.push(n.clone());
            true
        });
        out
    }
}

/// Who is at the other end of a tailnet connection (`/localapi/v0/whois`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Whois {
    pub user_id: i64,
    pub login: String,
    /// The peer device's name ("alexs-iphone").
    pub device: String,
    /// The node's stable id, for its CSRF token.
    pub node: String,
    pub tagged: bool,
}

impl Whois {
    pub fn parse(json: &str) -> Option<Whois> {
        let v: serde_json::Value = serde_json::from_str(json).ok()?;
        let node = v.get("Node")?;
        let profile = v.get("UserProfile");
        let user_id = node.get("User").and_then(|u| u.as_i64()).or_else(|| profile.and_then(|p| p.get("ID")).and_then(|i| i.as_i64())).unwrap_or(0);
        let name = node.get("ComputedName").or_else(|| node.get("Name")).and_then(|n| n.as_str()).unwrap_or_default();
        Some(Whois {
            user_id,
            login: profile.and_then(|p| p.get("LoginName")).and_then(|l| l.as_str()).unwrap_or_default().to_string(),
            device: name.split('.').next().unwrap_or_default().to_string(),
            node: node.get("StableID").and_then(|s| s.as_str()).map(str::to_string).or_else(|| node.get("ID").map(|i| i.to_string())).unwrap_or_default(),
            tagged: node.get("Tags").and_then(|t| t.as_array()).is_some_and(|t| !t.is_empty()),
        })
    }
}

/// The owner's own devices get in: same user as this node, neither of them tagged, a known user.
pub fn owner_allows(me: &SelfNode, peer: &Whois) -> bool {
    !me.tagged && !peer.tagged && me.user_id != 0 && peer.user_id == me.user_id && !peer.node.is_empty()
}

/// tailscaled's LocalAPI over its Unix socket. HTTP/1.0, so the answer comes whole and the connection closes; every
/// call is bounded by two seconds.
#[derive(Clone, Debug)]
pub struct LocalApi {
    pub socket: PathBuf,
}

impl Default for LocalApi {
    fn default() -> Self {
        LocalApi { socket: PathBuf::from(TAILSCALED_SOCKET) }
    }
}

/// Answers of more than this are not tailscaled's.
const LOCALAPI_MAX: u64 = 2 << 20;

impl LocalApi {
    pub fn get(&self, path: &str) -> Result<String, String> {
        get_unix(&self.socket, path)
    }

    pub fn self_node(&self) -> Result<SelfNode, String> {
        SelfNode::parse(&self.get("/localapi/v0/status?peers=false")?).ok_or_else(|| "tailscaled: no status".into())
    }

    pub fn whois(&self, peer: SocketAddr) -> Result<Whois, String> {
        let addr = match peer {
            SocketAddr::V4(a) => format!("{}:{}", a.ip(), a.port()),
            SocketAddr::V6(a) => format!("%5B{}%5D:{}", a.ip(), a.port()),
        };
        Whois::parse(&self.get(&format!("/localapi/v0/whois?addr={}", addr.replace(':', "%3A")))?).ok_or_else(|| "tailscaled: unknown peer".into())
    }
}

#[cfg(unix)]
fn get_unix(socket: &Path, path: &str) -> Result<String, String> {
    let mut s = std::os::unix::net::UnixStream::connect(socket).map_err(|e| format!("tailscaled: {e}"))?;
    let limit = Some(Duration::from_secs(2));
    let _ = s.set_read_timeout(limit);
    let _ = s.set_write_timeout(limit);
    // tailscaled refuses LocalAPI requests that carry an Origin or Referer, and checks the Host.
    s.write_all(format!("GET {path} HTTP/1.0\r\nHost: local-tailscaled.sock\r\n\r\n").as_bytes()).map_err(|e| format!("tailscaled: {e}"))?;
    let mut raw = Vec::new();
    s.take(LOCALAPI_MAX).read_to_end(&mut raw).map_err(|e| format!("tailscaled: {e}"))?;
    let text = String::from_utf8_lossy(&raw);
    let (head, body) = text.split_once("\r\n\r\n").ok_or("tailscaled: no answer")?;
    let status = head.split_whitespace().nth(1).unwrap_or("");
    if status != "200" {
        return Err(format!("tailscaled: {status}"));
    }
    Ok(body.to_string())
}
