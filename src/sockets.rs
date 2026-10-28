//! TCP sockets from /proc/net/tcp and /proc/net/tcp6, without spawning `netstat` or `ss`.
//! File sharing checks that smbd listens on the current Wi-Fi address (src/wifi.rs); remote
//! access counts open SSH sessions for its idle timeout (src/remote.rs).

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

pub const LISTEN: u8 = 0x0a;
pub const ESTABLISHED: u8 = 0x01;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sock {
    pub local: IpAddr,
    pub port: u16,
    pub state: u8,
}

/// Parses one /proc/net/tcp or tcp6 table. Addresses are printed as host-order 32-bit words; every
/// target (aarch64, x86-64, Apple silicon) is little-endian.
pub fn parse(table: &str) -> Vec<Sock> {
    table
        .lines()
        .skip(1)
        .filter_map(|line| {
            let mut f = line.split_whitespace();
            let (addr, port) = f.nth(1)?.split_once(':')?;
            let state = u8::from_str_radix(f.nth(1)?, 16).ok()?;
            Some(Sock { local: address(addr)?, port: u16::from_str_radix(port, 16).ok()?, state })
        })
        .collect()
}

fn address(hex: &str) -> Option<IpAddr> {
    let words: Vec<u32> = (0..hex.len() / 8).map(|i| u32::from_str_radix(&hex[i * 8..i * 8 + 8], 16)).collect::<Result<_, _>>().ok()?;
    let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    match bytes.len() {
        4 => Some(IpAddr::V4(Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3]))),
        16 => {
            let v6 = Ipv6Addr::from(<[u8; 16]>::try_from(bytes).ok()?);
            Some(v6.to_ipv4_mapped().map_or(IpAddr::V6(v6), IpAddr::V4))
        }
        _ => None,
    }
}

/// The sockets of both tables on this machine; empty where /proc is missing.
pub fn read() -> Vec<Sock> {
    ["/proc/net/tcp", "/proc/net/tcp6"].iter().flat_map(|p| parse(&std::fs::read_to_string(p).unwrap_or_default())).collect()
}

/// True when something listens on `port` at `ip` (or on every address).
pub fn listening(socks: &[Sock], ip: IpAddr, port: u16) -> bool {
    socks.iter().any(|s| s.state == LISTEN && s.port == port && (s.local == ip || s.local.is_unspecified()))
}

/// Open connections to our `port`.
pub fn established(socks: &[Sock], port: u16) -> usize {
    socks.iter().filter(|s| s.state == ESTABLISHED && s.port == port).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Trimmed from the device (ROCKNIX 20261001, `cat /proc/net/tcp /proc/net/tcp6`).
    const TCP: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 00000000:0016 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 1 1 0000000000000000 100 0 0 10 0
   1: 390200C0:01BD 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 2 1 0000000000000000 100 0 0 10 0
   2: 390200C0:0016 0A0200C0:D6C2 01 00000000:00000000 02:0009D8A4 00000000     0        0 3 2 0000000000000000 20 4 30 10 -1
";
    const TCP6: &str = "  sl  local_address                         remote_address                        st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 00000000000000000000000000000000:0016 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 4 1 0000000000000000 100 0 0 10 0
   1: 0000000000000000FFFF0000390200C0:0016 0000000000000000FFFF00000A0200C0:D6C4 01 00000000:00000000 02:0009D8A4 00000000     0        0 5 2 0000000000000000 20 4 30 10 -1
";

    #[test]
    fn reads_listeners_and_sessions() {
        let socks: Vec<Sock> = parse(TCP).into_iter().chain(parse(TCP6)).collect();
        let wlan: IpAddr = "192.0.2.57".parse().unwrap();
        assert_eq!(socks[1], Sock { local: wlan, port: 445, state: LISTEN });
        assert!(listening(&socks, wlan, 445));
        assert!(!listening(&socks, "192.0.2.58".parse().unwrap(), 445), "smbd still bound to an old address");
        assert!(listening(&socks, "192.0.2.58".parse().unwrap(), 22), "sshd listens on every address");
        assert_eq!(established(&socks, 22), 2, "one IPv4 and one v4-mapped IPv6 session");
        assert_eq!(established(&socks, 445), 0);
        assert!(parse("header\ngarbage line\n").is_empty());
    }
}
