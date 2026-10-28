//! Wi-Fi through ROCKNIX's NetworkManager (iwd backend). Profiles land in
//! /storage/.config/NetworkManager/system-connections, so they survive updates.
//! All functions block; call them from a worker thread.

#[derive(Clone, Debug, PartialEq)]
pub struct Network {
    pub ssid: String,
    pub signal: u8,
    pub secure: bool,
    pub active: bool,
}

/// Splits one line of `nmcli -t` output on unescaped colons and removes the `\:` escapes.
pub fn split_terse(line: &str) -> Vec<String> {
    let mut fields = vec![String::new()];
    let mut chars = line.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => {
                if let Some(next) = chars.next() {
                    fields.last_mut().unwrap().push(next);
                }
            }
            ':' => fields.push(String::new()),
            _ => fields.last_mut().unwrap().push(ch),
        }
    }
    fields
}

/// Visible networks, connected one first, then by signal; one entry per name.
pub fn parse_scan(output: &str) -> Vec<Network> {
    let mut nets: Vec<Network> = Vec::new();
    for line in output.lines() {
        let f = split_terse(line);
        if f.len() < 4 || f[0].is_empty() {
            continue;
        }
        let net = Network {
            ssid: f[0].clone(),
            signal: f[1].parse().unwrap_or(0),
            secure: !f[2].trim().is_empty() && f[2].trim() != "--",
            active: f[3].trim() == "*",
        };
        match nets.iter_mut().find(|n| n.ssid == net.ssid) {
            Some(existing) => {
                existing.signal = existing.signal.max(net.signal);
                existing.active |= net.active;
            }
            None => nets.push(net),
        }
    }
    nets.sort_by(|a, b| b.active.cmp(&a.active).then(b.signal.cmp(&a.signal)));
    nets
}

/// The network in use from `nmcli -t -f ACTIVE,SSID device wifi list --rescan no` ("yes:Home"), None when none is.
pub fn parse_active_ssid(output: &str) -> Option<String> {
    output.lines().map(split_terse).find(|f| f.len() >= 2 && f[0] == "yes" && !f[1].is_empty()).map(|f| f[1].clone())
}

/// Where ROCKNIX's Samba reads its configuration: /run/samba/smb.conf links to
/// /etc/samba/smb.conf, which links here.
pub const SMB_CONF_PATH: &str = "/storage/.config/smb.conf";

/// The only Samba configuration pocket-ui allows. ROCKNIX's default also shares /storage/.update,
/// /storage/.config, /var/log and the themes to anyone on the network, without a password and as
/// root. A .tar dropped into .update is installed at the next start, and the installer flashes the
/// internal bootloader Android depends on; a script dropped into .config runs as root. So only
/// the games folder is shared, and links can't lead out of it.
///
/// Who may write there (K1 in the internal review): only someone with this device's
/// password (shown on the Add games screen), only over Wi-Fi, and only as the unprivileged `nobody`
/// (ROCKNIX's /etc/passwd is on the read-only system image, so no user of our own can be added;
/// `nobody` is the one unprivileged account no service runs as). Folders root runs code or emulator
/// state from are not reachable at all: Ports and PortMaster scripts, BIOS files, savestates.
pub const SMB_CONF: &str = "\
# Written by pocket-ui; ROCKNIX's original is kept next to it as smb.conf.rocknix.
# Only the games are shared: no update, config, log or theme shares.
# Login: user pocket, the password on the Add games screen.
[global]
  server string = Retroid Pocket
  interfaces = wlan0
  bind interfaces only = yes
  unix extensions = no
  printable = no
  deadtime = 30
  mangled names = no
  name resolve order = host bcast
  printcap name = /dev/null
  load printers = no
  disable spoolss = yes
  enable core files = no
  security = user
  passdb backend = smbpasswd
  smb passwd file = /storage/.config/pocket-ui/share/smbpasswd
  username map = /storage/.config/pocket-ui/share/users.map
  map to guest = Never
  restrict anonymous = 2
  invalid users = root
  smb encrypt = desired
  fruit:model = Xserve
  socket options = TCP_NODELAY IPTOS_LOWDELAY
  min receivefile size = 16384
  aio read size = 16384
  aio write size = 16384
  use sendfile = yes
  strict allocate = no
  follow symlinks = no
  wide links = no

[games-roms]
  path = /storage/roms
  available = yes
  browseable = yes
  writeable = yes
  guest ok = no
  valid users = nobody
  force user = nobody
  force group = nogroup
  create mask = 0664
  directory mask = 0775
  veto files = /ports/PortMaster/bios/savestates/
  delete veto files = no
";

/// Name to type on the computer; `users.map` maps it to `SHARE_UNIX_USER`.
pub const SHARE_LOGIN: &str = "pocket";
/// The account smbd writes as (`force user`); present in ROCKNIX's read-only /etc/passwd.
pub const SHARE_UNIX_USER: &str = "nobody";
pub const SHARE_UID: u32 = 65534;
pub const SHARE_GID: u32 = 65534;
/// Password, smbpasswd and username map, all written by pocket-ui.
pub const SHARE_DIR: &str = "/storage/.config/pocket-ui/share";
/// Top-level folders of the share that `veto files` hides (case does not matter to clients).
pub const SHARE_VETOED: [&str; 4] = ["ports", "PortMaster", "bios", "savestates"];

/// MD4 (RFC 1320); only for the NT hash Samba's smbpasswd file stores.
fn md4(input: &[u8]) -> [u8; 16] {
    let mut msg = input.to_vec();
    let bits = (input.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bits.to_le_bytes());
    let mut h: [u32; 4] = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476];
    for block in msg.chunks(64) {
        let x: Vec<u32> = block.chunks(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
        let [mut a, mut b, mut c, mut d] = h;
        let f = |x: u32, y: u32, z: u32| (x & y) | (!x & z);
        let g = |x: u32, y: u32, z: u32| (x & y) | (x & z) | (y & z);
        let hh = |x: u32, y: u32, z: u32| x ^ y ^ z;
        for &i in &[0usize, 4, 8, 12] {
            a = a.wrapping_add(f(b, c, d)).wrapping_add(x[i]).rotate_left(3);
            d = d.wrapping_add(f(a, b, c)).wrapping_add(x[i + 1]).rotate_left(7);
            c = c.wrapping_add(f(d, a, b)).wrapping_add(x[i + 2]).rotate_left(11);
            b = b.wrapping_add(f(c, d, a)).wrapping_add(x[i + 3]).rotate_left(19);
        }
        for &i in &[0usize, 1, 2, 3] {
            let k = 0x5a827999u32;
            a = a.wrapping_add(g(b, c, d)).wrapping_add(x[i]).wrapping_add(k).rotate_left(3);
            d = d.wrapping_add(g(a, b, c)).wrapping_add(x[i + 4]).wrapping_add(k).rotate_left(5);
            c = c.wrapping_add(g(d, a, b)).wrapping_add(x[i + 8]).wrapping_add(k).rotate_left(9);
            b = b.wrapping_add(g(c, d, a)).wrapping_add(x[i + 12]).wrapping_add(k).rotate_left(13);
        }
        for &i in &[0usize, 2, 1, 3] {
            let k = 0x6ed9eba1u32;
            a = a.wrapping_add(hh(b, c, d)).wrapping_add(x[i]).wrapping_add(k).rotate_left(3);
            d = d.wrapping_add(hh(a, b, c)).wrapping_add(x[i + 8]).wrapping_add(k).rotate_left(9);
            c = c.wrapping_add(hh(d, a, b)).wrapping_add(x[i + 4]).wrapping_add(k).rotate_left(11);
            b = b.wrapping_add(hh(c, d, a)).wrapping_add(x[i + 12]).wrapping_add(k).rotate_left(15);
        }
        for (s, v) in h.iter_mut().zip([a, b, c, d]) {
            *s = s.wrapping_add(v);
        }
    }
    let mut out = [0u8; 16];
    for (i, w) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&w.to_le_bytes());
    }
    out
}

/// The NT hash Samba checks a password against: MD4 of the UTF-16LE password, in upper-case hex.
pub fn nt_hash(password: &str) -> String {
    let utf16: Vec<u8> = password.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    md4(&utf16).iter().map(|b| format!("{b:02X}")).collect()
}

/// One line of Samba's smbpasswd file, as `smbpasswd -a` writes it (no LM hash).
pub fn smbpasswd_line(user: &str, uid: u32, password: &str, changed: u64) -> String {
    format!("{user}:{uid}:{}:{}:[U          ]:LCT-{changed:08X}:\n", "X".repeat(32), nt_hash(password))
}

/// Makes sure `dir` holds the share password (created once per device), an smbpasswd file with
/// exactly our user and that password, and the username map. Returns the password.
pub fn ensure_share_login(dir: &std::path::Path) -> std::io::Result<String> {
    let password = crate::secret::load_or_create(&dir.join("password"))?;
    let passdb = dir.join("smbpasswd");
    let current = std::fs::read_to_string(&passdb).unwrap_or_default();
    let fields: Vec<&str> = current.trim_end().split(':').collect();
    let up_to_date = current.lines().count() == 1 && fields.first() == Some(&SHARE_UNIX_USER) && fields.get(3) == Some(&nt_hash(&password).as_str());
    if !up_to_date {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        crate::secret::write_private(&passdb, smbpasswd_line(SHARE_UNIX_USER, SHARE_UID, &password, now).as_bytes())?;
    }
    let map = format!("{SHARE_UNIX_USER} = {SHARE_LOGIN}\n");
    if std::fs::read_to_string(dir.join("users.map")).ok().as_deref() != Some(map.as_str()) {
        crate::secret::write_private(&dir.join("users.map"), map.as_bytes())?;
    }
    Ok(password)
}

/// Everything under `root` that the share user doesn't own yet, except the vetoed folders and
/// symlinks (never followed). Games copied in earlier as root or by `rsync` from a Mac (uid 501)
/// would otherwise be read-only over the share. The share root itself stays root's.
pub fn share_adoption(root: &std::path::Path, uid: u32) -> Vec<std::path::PathBuf> {
    use std::os::unix::fs::MetadataExt;
    let mut out = Vec::new();
    let mut stack: Vec<std::path::PathBuf> = match std::fs::read_dir(root) {
        Ok(rd) => rd
            .flatten()
            .filter(|e| !SHARE_VETOED.iter().any(|v| e.file_name().to_string_lossy().eq_ignore_ascii_case(v)))
            .map(|e| e.path())
            .collect(),
        Err(_) => return out,
    };
    while let Some(path) = stack.pop() {
        let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.uid() != uid {
            out.push(path.clone());
        }
        if meta.is_dir() {
            stack.extend(std::fs::read_dir(&path).into_iter().flatten().flatten().map(|e| e.path()));
        }
    }
    out
}

/// Shares named in a Samba configuration, e.g. ["global", "games-roms"].
pub fn smb_sections(conf: &str) -> Vec<String> {
    conf.lines()
        .filter_map(|l| l.trim().strip_prefix('[')?.strip_suffix(']').map(|s| s.trim().to_lowercase()))
        .collect()
}

/// Puts `SMB_CONF` at `path` unless it is already there, keeping the first original found as
/// `<path>.rocknix`. Returns whether the file changed.
pub fn secure_share_config(path: &std::path::Path) -> std::io::Result<bool> {
    let current = std::fs::read_to_string(path).unwrap_or_default();
    if current == SMB_CONF {
        return Ok(false);
    }
    let backup = path.with_extension("conf.rocknix");
    if !current.is_empty() && !backup.exists() {
        std::fs::write(&backup, &current)?;
    }
    // Written under a temporary name and renamed, so smbd never reads half a file.
    let tmp = path.with_extension("conf.new");
    std::fs::write(&tmp, SMB_CONF)?;
    std::fs::rename(&tmp, path)?;
    Ok(true)
}

/// nmcli refusing a stored profile that has no security section: EmulationStation's wifictl rebuilt it from
/// system.cfg with an empty key. Such a profile is deleted and the connection tried again.
pub fn stale_profile(nmcli_error: &str) -> bool {
    nmcli_error.contains("802-11-wireless-security.key-mgmt: property is missing")
}

/// A scan that nmcli refused: its exit code and the last line it complained with ("Error: No Wi-Fi device found."),
/// without the "Error: " it starts with. Nothing on stderr: the code alone.
pub fn scan_error(code: Option<i32>, stderr: &str) -> crate::net_state::ScanError {
    let line = stderr.lines().rev().map(str::trim).find(|l| !l.is_empty()).unwrap_or("nmcli failed");
    crate::net_state::ScanError { code, detail: line.strip_prefix("Error: ").unwrap_or(line).to_string() }
}

/// What ROCKNIX's own settings get after a connection: its `wifictl connect` rebuilds the NetworkManager profile from
/// exactly these, so the key has to be there as well (an open network has none). The profile's keyfile holds the same
/// key in the same plain form. On ROCKNIX 20261001 that rebuild happens only in EmulationStation: connecting there,
/// switching Wi-Fi on, changing the country or Local Play (`enableWifi(wifi.ssid, wifi.key)`). Not at boot
/// (`080-network` only runs `wifictl enable`, the radio), not after sleep, not when EmulationStation starts.
pub fn rocknix_settings<'a>(ssid: &'a str, psk: Option<&'a str>) -> [(&'static str, &'a str); 3] {
    [("wifi.enabled", "1"), ("wifi.ssid", ssid), ("wifi.key", psk.unwrap_or(""))]
}

/// ROCKNIX's own Wi-Fi settings (`wifi.enabled`, `wifi.ssid`, `wifi.key` in system.cfg), from which its wifictl
/// rebuilds the NetworkManager profile: what the watchdog reconnects to and repairs a keyless profile from.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RocknixWifi {
    pub enabled: bool,
    pub ssid: String,
    pub key: String,
}

pub fn parse_rocknix_wifi(system_cfg: &str) -> RocknixWifi {
    let mut w = RocknixWifi::default();
    for line in system_cfg.lines() {
        match line.split_once('=') {
            Some(("wifi.enabled", v)) => w.enabled = v.trim() == "1",
            Some(("wifi.ssid", v)) => w.ssid = v.to_string(),
            Some(("wifi.key", v)) => w.key = v.to_string(),
            _ => {}
        }
    }
    w
}

/// A stored profile that lost its security section (`stale_profile`) while ROCKNIX still knows the key: it
/// worked only as long as the first connection lasted, and any reconnect asked for the password again
/// (Mini V2, night of 2026-10-23). Repaired from the key instead.
pub fn profile_needs_key(key_mgmt: &str, rocknix: &RocknixWifi) -> bool {
    key_mgmt.trim().is_empty() && !rocknix.key.is_empty()
}

/// The key to write back into system.cfg's `wifi.key` from the NetworkManager profile of `wifi.ssid` (`profile_psk`,
/// as nmcli prints it, without its line break): when the profile has one and system.cfg has none or another.
/// EmulationStation's "connect to a saved network" writes `wifi.key=` empty, and its next `wifictl connect` would then
/// replace the profile with one without a key (docs/ES-REPLACEMENT-GAPS, 4.2). The profile is what connects, so it
/// wins. None: nothing to write (an open network, or both agree).
pub fn key_to_restore<'a>(profile_psk: &'a str, rocknix: &RocknixWifi) -> Option<&'a str> {
    (!rocknix.ssid.is_empty() && !profile_psk.is_empty() && profile_psk != rocknix.key && !profile_psk.contains('\n')).then_some(profile_psk)
}

/// The saved network the watchdog brings up: ROCKNIX's own network (`preferred`) when it is in range, else
/// the strongest other saved one in range (a phone hotspot when travelling). None when no saved network is
/// in range: NetworkManager connects by itself once one shows up.
pub fn pick_network(saved: &[String], visible: &[Network], preferred: &str) -> Option<String> {
    let in_range = |ssid: &str| visible.iter().any(|n| n.ssid == ssid);
    if !preferred.is_empty() && saved.iter().any(|s| s == preferred) && in_range(preferred) {
        return Some(preferred.to_string());
    }
    visible
        .iter()
        .filter(|n| saved.iter().any(|s| *s == n.ssid))
        .max_by_key(|n| n.signal)
        .map(|n| n.ssid.clone())
}

/// `iw dev wlan0 scan dump`: every access point the radio knows of, as (network name, ms since it was last heard).
/// iw writes the name's bytes that aren't plain text (and a space at either end) as `\xNN`.
pub fn parse_scan_dump(output: &str) -> Vec<(String, u64)> {
    let mut heard: Vec<(String, u64)> = Vec::new();
    let mut age: Option<u64> = None;
    for line in output.lines() {
        if line.starts_with("BSS ") {
            age = None;
        } else if let Some(ms) = line.trim().strip_prefix("last seen: ").and_then(|r| r.strip_suffix(" ms ago")) {
            age = ms.trim().parse().ok();
        } else if let (Some(ssid), Some(ms)) = (line.strip_prefix("\tSSID: "), age) {
            let (mut bytes, raw) = (Vec::new(), ssid.as_bytes());
            let mut i = 0;
            while i < raw.len() {
                let hex = (raw[i] == b'\\' && raw.get(i + 1) == Some(&b'x')).then(|| raw.get(i + 2..i + 4)).flatten();
                match hex.and_then(|h| u8::from_str_radix(std::str::from_utf8(h).ok()?, 16).ok()) {
                    Some(b) => { bytes.push(b); i += 4; }
                    None => { bytes.push(raw[i]); i += 1; }
                }
            }
            heard.push((String::from_utf8_lossy(&bytes).into_owned(), ms));
        }
    }
    heard
}

/// The networks of `visible` the radio really heard within the last `within_ms` (`heard`: `parse_scan_dump`). nmcli's
/// list goes on naming an access point that went away until iwd scans again, and iwd connects to it from that list:
/// a phone's hotspot switched off was still "in range" 20 s later, the watchdog's try failed ("Secrets were required"),
/// and that second failure on the same access point made iwd leave it alone long after it came back (Wi-Fi bench,
/// hotspot, 2026-10-27). Heard since the Wi-Fi dropped means it is really there again.
pub fn heard_within(visible: &[Network], heard: &[(String, u64)], within_ms: u64) -> Vec<Network> {
    visible.iter().filter(|n| heard.iter().any(|(ssid, ms)| *ssid == n.ssid && *ms < within_ms)).cloned().collect()
}

/// A saved network whose connect failed is gone: a fresh scan doesn't list it (`listed`), or the radio hasn't heard it
/// for 20 s (`heard`, None: iw couldn't tell).
pub fn gone(ssid: &str, listed: bool, heard: Option<&[(String, u64)]>) -> bool {
    !listed || heard.is_some_and(|h| !h.iter().any(|(s, ms)| s == ssid && *ms < 20_000))
}

/// Whether the watchdog tries now: a saved network heard again (`back`) at once, else when the pause is over (`due`);
/// never while no saved network is heard (`in_range` false), since a try then only fails. `in_range` None: iw couldn't
/// tell, the pauses alone decide (as before).
pub fn reconnect_now(in_range: Option<bool>, back: bool, due: bool) -> bool {
    match in_range {
        Some(false) => false,
        Some(true) => back || due,
        None => due,
    }
}

/// How long the watchdog waits before its `attempt`-th try (from 0) to bring a dropped Wi-Fi back: NetworkManager
/// gave up after one failed DHCP on the Mini V2 and left it offline for four hours.
pub fn reconnect_delay(attempt: u32) -> std::time::Duration {
    std::time::Duration::from_secs(match attempt {
        0 => 30,
        1 => 60,
        2 => 120,
        _ => 300,
    })
}

/// Resume notification is memory only; the watchdog rechecks it again inside the operation lane.
static RESUME: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn resumed() {
    RESUME.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
}

/// NetworkManager's answer to a key the network refused ("Secrets were required, but not provided": with iwd a wrong
/// password ends like that, after every access point of the network said no).
pub fn wrong_password(e: &str) -> bool {
    let lower = e.to_lowercase();
    lower.contains("secrets") || lower.contains("password") || lower.contains("802-1x")
}

/// nmcli deleting a profile that is already gone ("cannot delete unknown connection(s)"): forgotten all the same.
pub fn already_forgotten(e: &str) -> bool {
    e.to_lowercase().contains("unknown connection")
}

/// Calm, translatable screen text. The diagnostic stays in the log, never in a painted sentence.
pub fn connection_error(e: &str) -> &'static str {
    let lower = e.to_lowercase();
    if wrong_password(e) {
        "Wrong password. Try again."
    } else if lower.contains("no network with ssid") || lower.contains("could not be found") || lower.contains("ssid-not-found") {
        "That network is out of range. Scan again."
    } else if lower.contains("wlan0 not ready") {
        "Wi-Fi isn't ready yet. Try again in a moment."
    } else if lower.contains("timeout") || lower.contains("timed out") {
        "The network took too long to respond. Try again."
    } else if lower.contains("activation failed") || lower.contains("activation: failed") {
        "The connection didn't work. Try again."
    } else {
        "Couldn't connect to the network. Try again."
    }
}

// ---------------------------------------------------------------------------------------------------
// Saved networks, hidden networks and the Wi-Fi country (Settings → Wi-Fi), without EmulationStation. ROCKNIX's own
// `wifictl list_saved|forget_saved|connect_saved` do the same with nmcli (ROCKNIX 20261001); pocket-ui talks to nmcli
// itself, like `connect`, and keeps ROCKNIX's settings in step (`set_rocknix_setting`).

/// ROCKNIX's netplay access point (`wifictl`'s `NETPLAY_AP`): a profile of ROCKNIX's, never one of the user's networks.
pub const NETPLAY_AP: &str = "NETPLAY_AP";

/// A Wi-Fi profile NetworkManager keeps (`/storage/.config/NetworkManager/system-connections`).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Saved {
    /// The profile's name (`connection.id`); usually the network's name.
    pub id: String,
    pub uuid: String,
    /// The network's name (`802-11-wireless.ssid`).
    pub ssid: String,
    /// The profile is the one in use: NetworkManager has it activated, with its address.
    pub active: bool,
    /// NetworkManager is still bringing it up (`activating`: iwd trying the access points, or waiting for an address).
    /// Not connected yet: nothing online works, and the status corner says offline.
    pub connecting: bool,
    /// Saved as a hidden network.
    pub hidden: bool,
}

/// The Wi-Fi profiles in `nmcli -t -f NAME,UUID,TYPE,ACTIVE,STATE connection show`, ROCKNIX's netplay access point left
/// out; `ssid` is the name until `nmcli` was asked for the real one. ACTIVE is "yes" from the first moment of an
/// activation, so only STATE "activated" counts as in use; "activating" is `connecting` (on a fresh card a refused
/// password kept iwd retrying for six minutes while the page said "Connected", 2026-10-27).
pub fn parse_profiles(output: &str) -> Vec<Saved> {
    output
        .lines()
        .map(split_terse)
        .filter(|f| f.len() >= 4 && f[2] == "802-11-wireless" && !f[0].is_empty() && f[0] != NETPLAY_AP)
        .map(|f| {
            let on = f[3] == "yes";
            let state = f.get(4).map_or("activated", String::as_str);
            Saved { id: f[0].clone(), uuid: f[1].clone(), ssid: f[0].clone(), active: on && state == "activated", connecting: on && state == "activating", hidden: false }
        })
        .collect()
}

/// `nmcli -g 802-11-wireless.ssid,802-11-wireless.hidden connection show uuid <uuid>`: the name and whether it is hidden.
pub fn parse_ssid_hidden(output: &str) -> Option<(String, bool)> {
    let mut lines = output.lines();
    let ssid = lines.next()?.replace("\\:", ":").trim_end().to_string();
    let hidden = lines.next().is_some_and(|l| l.trim() == "yes");
    (!ssid.is_empty()).then_some((ssid, hidden))
}

/// One row of the Wi-Fi page: a network in reach, a saved one, or both.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub ssid: String,
    /// None: not in reach (a saved network only).
    pub signal: Option<u8>,
    pub secure: bool,
    pub active: bool,
    pub saved: Option<Saved>,
}

/// The networks in reach and the saved ones as one list: the one in use first, then those in reach by signal (a saved
/// one is marked), then the saved ones out of reach by name.
pub fn merge(nets: &[Network], saved: &[Saved]) -> Vec<Entry> {
    let mut out: Vec<Entry> = nets
        .iter()
        .map(|n| {
            let s = saved.iter().find(|s| s.ssid == n.ssid).cloned();
            // The profile says whether it is really in use: the scan marks the access point from the first moment of an
            // activation that may never finish (a refused password).
            let active = s.as_ref().map_or(n.active, |s| s.active);
            Entry { ssid: n.ssid.clone(), signal: Some(n.signal), secure: n.secure, active, saved: s }
        })
        .collect();
    let mut away: Vec<Entry> = saved
        .iter()
        .filter(|s| !nets.iter().any(|n| n.ssid == s.ssid))
        .map(|s| Entry { ssid: s.ssid.clone(), signal: None, secure: true, active: s.active, saved: Some(s.clone()) })
        .collect();
    away.sort_by_key(|e| e.ssid.to_lowercase());
    out.extend(away);
    out.sort_by(|a, b| b.active.cmp(&a.active).then(b.signal.is_some().cmp(&a.signal.is_some())).then(b.signal.cmp(&a.signal)));
    out
}

/// Where ROCKNIX's `iw` package keeps the Wi-Fi country for the next start: its udev rule runs
/// `/usr/lib/iw/setregdomain`, which reads `REGDOMAIN=` from here when the Wi-Fi chip appears. `wifictl` sets
/// `wifi.country` only when it connects, so without this file the country is lost at every start.
pub const REGDOMAIN_CONF: &str = "/storage/.cache/regdomain.conf";

/// A Wi-Fi country code as `iw reg set` takes it: two capital letters ("AT"), not "00" (world).
pub fn valid_country(code: &str) -> bool {
    code.len() == 2 && code.bytes().all(|b| b.is_ascii_uppercase())
}

/// `wifi.country` in ROCKNIX's system.cfg (what EmulationStation's "Wi-Fi country" sets), when it is a valid code.
pub fn country_setting(system_cfg: &str) -> Option<String> {
    let v = system_cfg.lines().find_map(|l| l.strip_prefix("wifi.country="))?.trim();
    valid_country(v).then(|| v.to_string())
}

/// The content of `REGDOMAIN_CONF` for a country.
pub fn regdomain_conf(code: &str) -> String {
    format!("# Written by pocket-ui (Settings → Wi-Fi → Wi-Fi country); read at start by /usr/lib/iw/setregdomain.\nREGDOMAIN={code}\n")
}

/// Every country of zoneinfo's `iso3166.tab` as (code, name), by name.
pub fn countries(iso_tab: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = iso_tab
        .lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_once('\t'))
        .map(|(c, n)| (c.trim().to_string(), n.trim().to_string()))
        .filter(|(c, n)| valid_country(c) && !n.is_empty())
        .collect();
    out.sort_by_key(|(_, n)| n.to_lowercase());
    out
}

/// What the Wi-Fi page knows besides the scan: the saved profiles, the country setting, the chip's country, the address.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Saving {
    pub saved: Vec<Saved>,
    /// `wifi.country` (None: Automatic).
    pub country: Option<String>,
    /// The country the Wi-Fi chip uses now (`iw reg get`).
    pub reg: Option<String>,
    pub ip: Option<String>,
    /// The Wi-Fi chip's own address ("00:00:5E:00:53:7F"), for a front desk that lets a device in by it (`mac`).
    pub mac: Option<String>,
}

/// The interface's address as `/sys/class/net/<iface>/address` has it, in capitals; None for anything that isn't one,
/// or all zeros (no chip, a virtual interface).
pub fn parse_mac(text: &str) -> Option<String> {
    let t = text.trim();
    let parts: Vec<&str> = t.split(':').collect();
    let ok = parts.len() == 6 && parts.iter().all(|p| p.len() == 2 && p.chars().all(|c| c.is_ascii_hexdigit()));
    (ok && parts.iter().any(|p| *p != "00")).then(|| t.to_ascii_uppercase())
}

/// The Wi-Fi interface's address (wlan0, or `POCKETUI_WIFI_IFACE` in the virtual device): one small file, read on the
/// scan's thread. iwd on ROCKNIX keeps the chip's fixed address (no `AddressRandomization`), so it is the one a hotel sees.
pub fn mac() -> Option<String> {
    let iface = std::env::var("POCKETUI_WIFI_IFACE").unwrap_or_else(|_| "wlan0".into());
    if iface.is_empty() || iface.contains('/') || iface.contains("..") {
        return None;
    }
    parse_mac(&std::fs::read_to_string(format!("/sys/class/net/{iface}/address")).ok()?)
}

#[cfg(target_os = "linux")]
mod imp {
    use super::{parse_scan, rocknix_settings, scan_error, stale_profile, Network, Saved, Saving};
    use crate::net_state::ScanError;
    use std::path::Path;
    use std::process::Command;
    use std::sync::atomic::AtomicBool;
    use std::time::{Duration, Instant};

    struct Activation<'a> {
        nmcli: &'a Path,
        started: Instant,
    }

    impl Activation<'_> {
        fn command(&self, args: &[&str]) -> Result<String, String> {
            let out =
                crate::proc::run_with_timeout(Command::new(self.nmcli).env("LC_ALL", "C").args(args), Duration::from_secs(3)).map_err(|e| e.to_string())?;
            if !out.status.success() {
                return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
            }
            Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
        }
    }

    impl crate::wifi_activation::Ops for Activation<'_> {
        fn device(&mut self) -> Result<crate::wifi_activation::Device, String> {
            let out = self.command(&["-g", "GENERAL.STATE,GENERAL.CON-UUID,GENERAL.REASON", "device", "show", "wlan0"])?;
            let mut lines = out.lines();
            let state = lines
                .next()
                .and_then(|l| l.split_whitespace().next())
                .and_then(|n| n.parse().ok())
                .ok_or_else(|| "Couldn't read the Wi-Fi device state".to_string())?;
            let uuid = lines.next().unwrap_or("").trim();
            let reason = lines.next().unwrap_or_default().to_string();
            let ssid = if uuid.is_empty() || uuid == "--" {
                None
            } else {
                // connection.id is a profile label, not necessarily its SSID; use the UUID to read the real name.
                Some(self.command(&["-e", "no", "-g", "802-11-wireless.ssid", "connection", "show", "uuid", uuid])?)
            };
            Ok(crate::wifi_activation::Device { state, ssid, reason })
        }
        fn disconnect(&mut self) -> Result<(), String> {
            self.command(&["-w", "2", "device", "disconnect", "wlan0"]).map(|_| ())
        }
        fn now(&self) -> Duration {
            self.started.elapsed()
        }
        fn wait(&mut self) {
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    fn prepare_connection(nmcli: &Path, ssid: &str) -> Result<bool, String> {
        crate::wifi_activation::prepare(
            &mut Activation {
                nmcli,
                started: Instant::now(),
            },
            ssid,
        )
    }

    /// The name of the network in use, from what NetworkManager last saw (no new scan; at most 5 s).
    pub fn connected_ssid() -> Option<String> {
        let mut cmd = Command::new("nmcli");
        cmd.args(["-t", "-f", "ACTIVE,SSID", "device", "wifi", "list", "--rescan", "no"]);
        let out = crate::proc::run_with_timeout(&mut cmd, Duration::from_secs(5)).ok()?;
        super::parse_active_ssid(&String::from_utf8_lossy(&out.stdout))
    }

    pub fn scan() -> Vec<Network> {
        scan_checked(Path::new("nmcli")).unwrap_or_default()
    }

    /// A scan that says whether it worked: the program did not run, did not finish in `SCAN_TIMEOUT`, or exited with an
    /// error (its code and last line of complaint) are errors, so an empty list is only ever an empty airspace.
    pub fn scan_checked(nmcli: &Path) -> Result<Vec<Network>, ScanError> {
        scan_until(nmcli, &AtomicBool::new(false))
    }

    /// `scan_checked` that gives up (kills nmcli) as soon as `stop` is set: a switch of the radio is waiting for its turn.
    pub fn scan_until(nmcli: &Path, stop: &AtomicBool) -> Result<Vec<Network>, ScanError> {
        let mut cmd = Command::new(nmcli);
        cmd.args(["-t", "-f", "SSID,SIGNAL,SECURITY,IN-USE", "dev", "wifi", "list", "--rescan", "yes"]);
        let out = crate::proc::run_until(&mut cmd, SCAN_TIMEOUT, stop).map_err(|e| match e.kind() {
            std::io::ErrorKind::TimedOut => ScanError { code: None, detail: format!("nmcli did not answer in {} s", SCAN_TIMEOUT.as_secs()) },
            std::io::ErrorKind::Interrupted => ScanError { code: None, detail: "stopped".into() },
            _ => ScanError { code: None, detail: format!("nmcli: {e}") },
        })?;
        if !out.status.success() {
            return Err(scan_error(out.status.code(), &String::from_utf8_lossy(&out.stderr)));
        }
        Ok(parse_scan(&String::from_utf8_lossy(&out.stdout)))
    }

    const SCAN_TIMEOUT: Duration = Duration::from_secs(40);

    /// Connects and stores the profile; ROCKNIX's own settings learn about it too.
    pub fn connect(ssid: &str, psk: Option<&str>) -> Result<(), String> {
        connect_with(ssid, psk, false)
    }

    /// `connect`, for a hidden network too (`hidden`: NetworkManager asks for it by name instead of waiting to see it).
    pub fn connect_with(ssid: &str, psk: Option<&str>, hidden: bool) -> Result<(), String> {
        if connect_network(Path::new("nmcli"), ssid, psk, hidden)? { return Ok(()); }
        // The new address for `ip` (this runs on a thread).
        ip_now();
        // Same as ROCKNIX's wifictl: keep the password in the keyfile so it reconnects at boot.
        let _ = Command::new("nmcli").args(["connection", "modify", ssid, "wifi-sec.psk-flags", "0"]).output();
        // NetworkManager's default gives up after a few failed tries; this network is retried forever.
        let _ = Command::new("nmcli")
            .args(["connection", "modify", ssid, "connection.autoconnect-retries", "0"])
            .output();
        // And in ROCKNIX's settings, which wifictl rebuilds the profile from (under ROCKNIX's lock, `system_cfg`).
        for (key, value) in rocknix_settings(ssid, psk) {
            set_rocknix_setting(key, value);
        }
        Ok(())
    }

    /// Returns true when an existing connection was joined; commands are injectable for the collision tests.
    fn connect_network(nmcli: &Path, ssid: &str, psk: Option<&str>, hidden: bool) -> Result<bool, String> {
        let mut activation = Activation { nmcli, started: Instant::now() };
        let joined = if psk.is_some() {
            crate::wifi_activation::prepare_password(&mut activation, ssid)?
        } else {
            crate::wifi_activation::prepare(&mut activation, ssid)?
        };
        if joined { return Ok(true); }
        let attempt = || {
            let mut cmd = Command::new(nmcli);
            cmd.env("LC_ALL", "C");
            cmd.args(["-w", "60", "device", "wifi", "connect", ssid]);
            if let Some(psk) = psk {
                cmd.args(["password", psk]);
            }
            if hidden {
                cmd.args(["hidden", "yes"]);
            }
            cmd.args(["ifname", "wlan0"]);
            let out = cmd.output().map_err(|e| e.to_string())?;
            if out.status.success() {
                return Ok(());
            }
            let err = String::from_utf8_lossy(&out.stderr);
            Err(err.lines().last().unwrap_or("connection failed").trim().to_string())
        };
        if let Err(e) = attempt() {
            if psk.is_some() && super::wrong_password(&e) {
                drop_refused(&activation, ssid);
                return Err(e);
            }
            if !stale_profile(&e) {
                return Err(e);
            }
            // A profile of this name without security (see `stale_profile`): drop it and connect afresh.
            eprintln!("wifi: the stored profile for this network has no security settings; replacing it");
            let _ = Command::new(nmcli).args(["connection", "delete", "id", ssid]).output();
            attempt()?;
        }
        Ok(false)
    }

    /// A typed password the network refused: nmcli has stored it in the network's profile all the same (a new profile, or
    /// the old one updated), and NetworkManager mirrors that into iwd's `<SSID>.psk`. Left there, iwd keeps auto-connecting
    /// with the wrong key, every access point in turn, without end, and the page offered the network as "Saved"
    /// (fresh card, 2026-10-27). Every profile of that network goes, which also deletes iwd's file; the next password typed
    /// starts from nothing. iwd reads the file afresh at each connect, so nothing of the refused key stays behind.
    fn drop_refused(activation: &Activation, ssid: &str) {
        let Ok(list) = activation.command(&["-t", "-f", "UUID,TYPE", "connection", "show"]) else { return };
        for f in list.lines().map(super::split_terse) {
            if f.len() < 2 || f[1] != "802-11-wireless" {
                continue;
            }
            if activation.command(&["-e", "no", "-g", "802-11-wireless.ssid", "connection", "show", "uuid", &f[0]]).is_ok_and(|s| s == ssid) {
                match activation.command(&["connection", "delete", "uuid", &f[0]]) {
                    Ok(_) => eprintln!("wifi: {ssid} refused the password; its profile is removed so the next one typed is used"),
                    Err(e) => eprintln!("wifi: couldn't remove the refused profile of {ssid}: {e}"),
                }
            }
        }
    }

    /// Writes one key of ROCKNIX's system.cfg through pocket-ui's locked writer (`system_cfg::set`, ROCKNIX's
    /// `/tmp/.system.cfg.lock` and `set_setting` semantics; "default" removes the key). Every Wi-Fi write goes through here.
    pub fn set_rocknix_setting(key: &str, value: &str) {
        crate::system_cfg::set(key, value);
    }

    /// The saved Wi-Fi profiles (ROCKNIX's `wifictl list_saved`, with the network names and the one in use), the
    /// country setting, the chip's country and the address: what the Wi-Fi page shows besides the scan.
    pub fn saving() -> Saving {
        saving_until(&AtomicBool::new(false))
    }

    /// `saving` that stops asking nmcli once `stop` is set (a switch of the radio is waiting for its turn).
    pub fn saving_until(stop: &AtomicBool) -> Saving {
        let mut saved = nmcli_until(&["-t", "-f", "NAME,UUID,TYPE,ACTIVE,STATE", "connection", "show"], stop).map(|o| super::parse_profiles(&o)).unwrap_or_default();
        for s in &mut saved {
            if stop.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
            if let Some((ssid, hidden)) = nmcli_until(&["-g", "802-11-wireless.ssid,802-11-wireless.hidden", "connection", "show", "uuid", &s.uuid], stop).and_then(|o| super::parse_ssid_hidden(&o)) {
                s.ssid = ssid;
                s.hidden = hidden;
            }
        }
        let reg = crate::proc::run_with_timeout(Command::new("iw").args(["reg", "get"]), IW_TIMEOUT)
            .ok()
            .and_then(|o| crate::timezone::parse_reg_country(&String::from_utf8_lossy(&o.stdout)));
        Saving { saved, country: super::country_setting(&std::fs::read_to_string(SYSTEM_CFG).unwrap_or_default()), reg, ip: ip_now(), mac: super::mac() }
    }

    const IW_TIMEOUT: Duration = Duration::from_secs(3);

    /// Connects to a saved network without asking for its password (ROCKNIX's `wifictl connect_saved`). ROCKNIX's
    /// settings follow: the name, and the key read back from the profile, so EmulationStation's `wifictl connect` never
    /// rebuilds the profile from an empty key (docs: ES-REPLACEMENT-GAPS 4.2). A secured profile whose key can't be
    /// read leaves them as they were.
    pub fn connect_saved(s: &Saved) -> Result<(), String> {
        connect_saved_with(s, None)
    }

    fn connect_saved_with(s: &Saved, stop: Option<&AtomicBool>) -> Result<(), String> {
        let joined = if stop.is_some() {
            crate::wifi_activation::prepare_reconnect(&mut Activation { nmcli: Path::new("nmcli"), started: Instant::now() }, &s.ssid)?
        } else {
            prepare_connection(Path::new("nmcli"), &s.ssid)?
        };
        // A skipped background activation must not copy another network's settings into ROCKNIX either.
        if stop.is_some() && joined { return Ok(()); }
        if !joined {
            activate_saved(Path::new("nmcli"), &s.uuid, stop).map_err(|e| if stop.is_none() { gone_or(e, &s.ssid) } else { e })?;
        }
        ip_now();
        let _ = nmcli(&["connection", "modify", "uuid", &s.uuid, "connection.autoconnect-retries", "0"]);
        let key_mgmt = nmcli(&["-g", "802-11-wireless-security.key-mgmt", "connection", "show", "uuid", &s.uuid]).unwrap_or_default();
        let psk = nmcli(&["-s", "-g", "802-11-wireless-security.psk", "connection", "show", "uuid", &s.uuid]).unwrap_or_default();
        match (key_mgmt.trim().is_empty(), psk.is_empty()) {
            (true, _) => rocknix_settings(&s.ssid, None).iter().for_each(|(k, v)| set_rocknix_setting(k, v)),
            (false, false) => rocknix_settings(&s.ssid, Some(&psk)).iter().for_each(|(k, v)| set_rocknix_setting(k, v)),
            (false, true) => eprintln!("wifi: connected to {}, but its key can't be read; ROCKNIX's settings left as they were", s.ssid),
        }
        Ok(())
    }

    /// A saved network that worked before and now fails with "Secrets were required": with iwd that is also what a
    /// connect to an access point that has just gone away ends with (iwd still lists it until its next scan). When a
    /// fresh scan doesn't hear the network, the player is told it is out of range instead of asked for its password.
    fn gone_or(e: String, ssid: &str) -> String {
        if !super::wrong_password(&e) {
            return e;
        }
        let listed = scan_checked(Path::new("nmcli")).map(|n| n.iter().any(|n| n.ssid == ssid)).unwrap_or(true);
        if !super::gone(ssid, listed, heard().as_deref()) {
            return e;
        }
        eprintln!("wifi: {ssid} refused the saved profile ({e}), but it isn't heard any more: out of range");
        format!("No network with SSID '{ssid}' found.")
    }

    /// Background activation yields to any player operation; killing nmcli leaves NM's activation for the next guard.
    fn activate_saved(nmcli: &Path, uuid: &str, stop: Option<&AtomicBool>) -> Result<(), String> {
        if stop.is_some_and(|s| s.load(std::sync::atomic::Ordering::SeqCst)) { return Err("stopped".into()); }
        let mut cmd = Command::new(nmcli);
        cmd.env("LC_ALL", "C").args(["-w", if stop.is_some() { "30" } else { "45" }, "connection", "up", "uuid", uuid, "ifname", "wlan0"]);
        let out = match stop {
            Some(stop) => crate::proc::run_until(&mut cmd, Duration::from_secs(30), stop),
            None => crate::proc::run_with_timeout(&mut cmd, Duration::from_secs(50)),
        }.map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
        }
        Ok(())
    }

    /// Deletes a saved network (ROCKNIX's `wifictl forget_saved`). When ROCKNIX's settings name it, they forget it too,
    /// so neither the watchdog nor EmulationStation brings it back, and its key doesn't stay in system.cfg. NetworkManager
    /// deletes iwd's copy (`<SSID>.psk`) with the profile. A profile that is already gone (a second Y on a list from
    /// before) counts as forgotten.
    pub fn forget(s: &Saved) -> Result<(), String> {
        forget_with(Path::new("nmcli"), s)
    }

    fn forget_with(nmcli: &Path, s: &Saved) -> Result<(), String> {
        let out = Command::new(nmcli).env("LC_ALL", "C").args(["connection", "delete", "uuid", &s.uuid]).output().map_err(|e| e.to_string())?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            let err = err.lines().last().unwrap_or("could not delete it").trim().to_string();
            if !super::already_forgotten(&err) {
                return Err(err);
            }
            eprintln!("wifi: {} was already forgotten ({err})", s.ssid);
        }
        let w = super::parse_rocknix_wifi(&std::fs::read_to_string(SYSTEM_CFG).unwrap_or_default());
        if w.ssid == s.ssid {
            set_rocknix_setting("wifi.ssid", "default");
            set_rocknix_setting("wifi.key", "default");
        }
        Ok(())
    }

    /// Sets the Wi-Fi country (None: Automatic, the chip's own) now, for EmulationStation's `wifictl` (`wifi.country`)
    /// and for the next start (`REGDOMAIN_CONF`).
    pub fn set_country(code: Option<&str>) -> Result<(), String> {
        if code.is_some_and(|c| !super::valid_country(c)) {
            return Err("That is not a country code.".into());
        }
        set_rocknix_setting("wifi.country", code.unwrap_or("default"));
        let conf = Path::new(super::REGDOMAIN_CONF);
        let kept = match code {
            Some(c) => conf.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|_| crate::fsutil::write_atomic(conf, super::regdomain_conf(c).as_bytes())),
            None => std::fs::remove_file(conf).or_else(|e| if e.kind() == std::io::ErrorKind::NotFound { Ok(()) } else { Err(e) }),
        };
        kept.map_err(|e| format!("{}: {e}", conf.display()))?;
        let out = crate::proc::run_with_timeout(Command::new("iw").args(["reg", "set", code.unwrap_or("00")]), IW_TIMEOUT).map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(format!("iw reg set: {}", String::from_utf8_lossy(&out.stderr).trim()));
        }
        Ok(())
    }

    /// wlan0's IPv4 address as last looked at, from memory: never waits. Asked from the UI thread (the game card, setup,
    /// the cover downloads' "online"), so a look older than `IP_KEPT` starts a new one on a thread (`ip`, at most
    /// `IP_TIMEOUT`) and this returns what the last one found; None until the first is in, as offline. The watchdog
    /// looks every 15 s anyway (`ip_now`), so the value is there long before anybody asks.
    pub fn ip() -> Option<String> {
        IP.refresh(IP_KEPT, ask_ip, || {});
        IP.get().flatten()
    }

    /// Looks now and keeps the answer for `ip`: threads only (a process, up to `IP_TIMEOUT`).
    pub fn ip_now() -> Option<String> {
        let ip = ask_ip();
        IP.set(ip.clone());
        ip
    }

    const SYSTEM_CFG: &str = "/storage/.config/system/configs/system.cfg";
    const NMCLI_TIMEOUT: Duration = Duration::from_secs(40);

    fn nmcli(args: &[&str]) -> Option<String> {
        nmcli_until(args, &AtomicBool::new(false))
    }

    fn nmcli_until(args: &[&str], stop: &AtomicBool) -> Option<String> {
        let out = crate::proc::run_until(Command::new("nmcli").env("LC_ALL", "C").args(args), NMCLI_TIMEOUT, stop).ok()?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// The stored key of the profile `ssid` (`-s`: with its secrets; `-e no`: a ':' or '\\' in it unescaped), with only
    /// nmcli's line break taken off: spaces at either end belong to the key.
    fn profile_psk(ssid: &str) -> Option<String> {
        let out = crate::proc::run_with_timeout(
            Command::new("nmcli").args(["-e", "no", "-s", "-g", "802-11-wireless-security.psk", "connection", "show", "id", ssid]),
            NMCLI_TIMEOUT,
        )
        .ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        out.status.success().then(|| text.strip_suffix('\n').unwrap_or(&text).to_string())
    }

    /// What the radio heard and how long ago (`iw dev wlan0 scan dump`, no new scan); None when iw can't say.
    fn heard() -> Option<Vec<(String, u64)>> {
        let out = crate::proc::run_with_timeout(Command::new("iw").args(["dev", "wlan0", "scan", "dump"]), IW_TIMEOUT).ok()?;
        out.status.success().then(|| super::parse_scan_dump(&String::from_utf8_lossy(&out.stdout)))
    }

    /// How long after a drop the watchdog asks for a scan at each look; after that iwd's own scans (every 30 s) do.
    const PROBE_SCANS_FOR: Duration = Duration::from_secs(600);

    /// Keeps Wi-Fi up on its own thread (`watchdog`): repairs a keyless profile once, then, while ROCKNIX's
    /// Wi-Fi is on, brings a dropped connection back with growing pauses (`reconnect_delay`), to the saved
    /// network in range (`pick_network`: ROCKNIX's own first, else the strongest saved one, e.g. a phone
    /// hotspot elsewhere). Never cuts in while NetworkManager is connecting. One `ip` call every 15 s while online.
    pub fn watchdog(net: std::sync::Arc<crate::net_state::NetState>) {
        std::thread::Builder::new()
            .name("wifi-watchdog".into())
            .spawn(move || {
                // The address for `ip`, so the UI has it in memory from the first moments.
                ip_now();
                let rocknix = || super::parse_rocknix_wifi(&std::fs::read_to_string(SYSTEM_CFG).unwrap_or_default());
                let w = rocknix();
                if !w.ssid.is_empty() {
                    let key_mgmt = nmcli(&["-g", "802-11-wireless-security.key-mgmt", "connection", "show", "id", &w.ssid]);
                    if key_mgmt.as_deref().is_some_and(|k| super::profile_needs_key(k, &w)) {
                        println!("wifi: the profile for this network had lost its key; restored from ROCKNIX's settings");
                        let _ = Command::new("nmcli")
                            .args(["connection", "modify", "id", &w.ssid, "wifi-sec.key-mgmt", "wpa-psk", "wifi-sec.psk"])
                            .arg(&w.key)
                            .args(["wifi-sec.psk-flags", "0", "connection.autoconnect-retries", "0"])
                            .output();
                    }
                    // The other way round: the profile has the key and system.cfg lost it (EmulationStation), so that
                    // wifictl can never rebuild a profile without one from it (`key_to_restore`).
                    if let Some(psk) = profile_psk(&w.ssid) {
                        if let Some(key) = super::key_to_restore(&psk, &w) {
                            let ok = crate::system_cfg::set("wifi.key", key);
                            println!(
                                "wifi: ROCKNIX's settings had lost this network's key; restored from its profile: {}",
                                if ok { "done" } else { "failed" }
                            );
                        }
                    }
                }
                let origin = Instant::now();
                let generation = || super::RESUME.load(std::sync::atomic::Ordering::SeqCst);
                let mut seen_resume = generation();
                let mut settle = crate::wifi_activation::Settle::new(Duration::ZERO, seen_resume);
                let (mut down_since, mut attempt, mut next): (Option<Instant>, u32, Option<Instant>) = (None, 0, None);
                // Whether a saved network was heard since the drop at the last look (None: not looked, or iw couldn't
                // tell), and the saved networks' names, read once per time offline.
                let (mut in_range_before, mut saved_names): (Option<bool>, Option<Vec<String>>) = (None, None);
                loop {
                    std::thread::sleep(Duration::from_secs(15));
                    let current_resume = generation();
                    if current_resume != seen_resume {
                        seen_resume = current_resume;
                        (down_since, attempt, next, in_range_before, saved_names) = (None, 0, None, None, None);
                    }
                    if ip_now().is_some() {
                        if attempt > 0 {
                            println!("wifi: back after {attempt} reconnect(s)");
                        }
                        (down_since, attempt, next, in_range_before, saved_names) = (None, 0, None, None, None);
                        continue;
                    }
                    use crate::wifi_activation::Ops;
                    let device = Activation {
                        nmcli: Path::new("nmcli"),
                        started: Instant::now(),
                    }
                    .device();
                    let ready = device.as_ref().is_ok_and(|d| settle.ready(origin.elapsed(), current_resume, d));
                    if !ready {
                        continue;
                    }
                    let w = rocknix();
                    let radio_on = nmcli(&["radio", "wifi"]).is_some_and(|r| r == "enabled");
                    if !w.enabled || !radio_on {
                        (down_since, attempt, next, in_range_before, saved_names) = (None, 0, None, None, None);
                        continue;
                    }
                    let now = Instant::now();
                    let since = *down_since.get_or_insert(now);
                    let due = *next.get_or_insert(since + super::reconnect_delay(0));
                    // Is a saved network really there (heard since the drop)? For the first minutes offline the radio
                    // is asked to scan (iwd's own scans come every 30 s), so a hotspot that is back is noticed within
                    // about 20 s and connected at once: iwd itself leaves an access point alone for a while after a
                    // failed connect. Without one heard no try is made: it could only fail.
                    if since.elapsed() < PROBE_SCANS_FOR {
                        let _ = crate::proc::run_with_timeout(Command::new("nmcli").args(["device", "wifi", "rescan"]), Duration::from_secs(5));
                        std::thread::sleep(Duration::from_secs(5));
                    }
                    let names = saved_names.get_or_insert_with(|| saving().saved.into_iter().map(|s| s.ssid).collect());
                    let offline_ms = since.elapsed().as_millis() as u64;
                    let in_range = heard().map(|h| h.iter().any(|(ssid, ms)| *ms < offline_ms && names.contains(ssid)));
                    let back = in_range == Some(true) && in_range_before == Some(false);
                    in_range_before = in_range.or(in_range_before);
                    if !super::reconnect_now(in_range, back, now >= due) {
                        continue;
                    }
                    if back {
                        println!("wifi: a saved network is in range again after {} s offline", since.elapsed().as_secs());
                    }
                    attempt += 1;
                    let queued_resume = current_resume;
                    net.reconnect(move |stop| {
                        // A queued job can outlive a user's switch/connect or a suspend. Check afresh in the lane.
                        if super::RESUME.load(std::sync::atomic::Ordering::SeqCst) != queued_resume {
                            return;
                        }
                        let mut activation = Activation {
                            nmcli: Path::new("nmcli"),
                            started: Instant::now(),
                        };
                        let Ok(d) = activation.device() else {
                            return;
                        };
                        if !matches!(d.state, 30 | 120) {
                            return;
                        }
                        if !super::parse_rocknix_wifi(&std::fs::read_to_string(SYSTEM_CFG).unwrap_or_default()).enabled {
                            return;
                        }
                        let saved = saving_until(&stop).saved;
                        let names: Vec<String> = saved.iter().map(|s| s.ssid.clone()).collect();
                        let Ok(mut networks) = scan_until(Path::new("nmcli"), &stop) else { return; };
                        if let Some(h) = heard() {
                            networks = super::heard_within(&networks, &h, since.elapsed().as_millis() as u64);
                        }
                        let Some(ssid) = super::pick_network(&names, &networks, &w.ssid) else {
                            println!("wifi: no saved network in range");
                            return;
                        };
                        // The scan may have taken a while; NM may have started an activation meanwhile.
                        let Ok(d) = activation.device() else {
                            return;
                        };
                        if !matches!(d.state, 30 | 120) {
                            return;
                        }
                        if super::RESUME.load(std::sync::atomic::Ordering::SeqCst) != queued_resume {
                            return;
                        }
                        if stop.load(std::sync::atomic::Ordering::SeqCst) { return; }
                        let Some(profile) = saved.iter().find(|s| s.ssid == ssid) else {
                            return;
                        };
                        println!(
                            "wifi: offline for {} s; connecting to saved network {ssid:?} (try {attempt})",
                            since.elapsed().as_secs()
                        );
                        let result = connect_saved_with(profile, Some(&stop));
                        match result {
                            Ok(()) => println!("wifi: reconnect worked"),
                            Err(e) => eprintln!("wifi: reconnect failed: {e}"),
                        }
                    });
                    next = Some(Instant::now() + super::reconnect_delay(attempt));
                }
            })
            .ok();
    }

    #[cfg(test)]
    mod activation_tests {
        use super::*;
        use crate::testutil::{write_executable, TestDir};
        use crate::wifi_activation::Ops;

        #[test]
        fn a_typed_password_replaces_the_old_activation_and_is_tried_once() {
            let dir = TestDir::new("wifi-new-key");
            let nmcli = dir.join("nmcli");
            std::fs::write(dir.join("state"), "50 (config)\nuuid-home\n0 (none)\n").unwrap();
            write_executable(&nmcli, r#"#!/bin/sh
base=$(dirname "$0")
printf '%s\n' "$*" >> "$base/log"
case "$*" in
  '-g GENERAL.STATE,GENERAL.CON-UUID,GENERAL.REASON device show wlan0') cat "$base/state" ;;
  '-e no -g 802-11-wireless.ssid connection show uuid uuid-home') echo 'Home' ;;
  '-w 2 device disconnect wlan0') printf '120 (failed)\nuuid-home\n7 (Secrets were required, but not provided)\n' > "$base/state" ;;
  '-w 60 device wifi connect Home password new-key ifname wlan0') echo 'Secrets were required, but not provided' >&2; exit 1 ;;
  *) exit 42 ;;
esac
"#);
            let err = connect_network(&nmcli, "Home", Some("new-key"), false).unwrap_err();
            assert_eq!(super::super::connection_error(&err), "Wrong password. Try again.");
            let log = std::fs::read_to_string(dir.join("log")).unwrap();
            assert_eq!(log.matches("device wifi connect").count(), 1, "{log}");
            assert!(log.find("device disconnect").unwrap() < log.find("device wifi connect").unwrap(), "{log}");
        }

        /// A stand-in for NetworkManager with iwd as ROCKNIX runs it, for one network "Wifi" whose router takes the key in
        /// `router`. Profiles are files in `profiles/` (name: uuid; lines: SSID, key); NetworkManager mirrors each into
        /// iwd's `iwd/<SSID>.psk` and deletes that with the profile. A profile whose key the router refuses keeps iwd
        /// auto-connecting forever: the device stays in `config` (50) on it, as on the fresh card from 2026-10-27 436 s to 801 s.
        fn nm_with_iwd(dir: &TestDir) -> std::path::PathBuf {
            let nmcli = dir.join("nmcli");
            std::fs::create_dir_all(dir.join("profiles")).unwrap();
            std::fs::create_dir_all(dir.join("iwd")).unwrap();
            std::fs::write(dir.join("router"), "right-key\n").unwrap();
            write_executable(&nmcli, r#"#!/bin/sh
base=$(dirname "$0")
printf '%s\n' "$*" >> "$base/log"
router=$(cat "$base/router")
refused() { for f in "$base"/profiles/*; do [ -f "$f" ] && [ "$(sed -n 2p "$f")" != "$router" ] && basename "$f" && return; done; }
case "$*" in
  '-g GENERAL.STATE,GENERAL.CON-UUID,GENERAL.REASON device show wlan0')
    if [ -s "$base/connected" ]; then printf '100 (connected)\n%s\n0 (No reason given)\n' "$(cat "$base/connected")"
    elif [ -n "$(refused)" ]; then printf '50 (connecting (configuring))\n%s\n0 (No reason given)\n' "$(refused)"
    else printf '30 (disconnected)\n--\n0 (No reason given)\n'; fi ;;
  '-e no -g 802-11-wireless.ssid connection show uuid '*)
    f="$base/profiles/$8"; [ -f "$f" ] || { echo "Error: $8 - no such connection profile." >&2; exit 10; }; sed -n 1p "$f" ;;
  '-t -f UUID,TYPE connection show') for f in "$base"/profiles/*; do [ -f "$f" ] && echo "$(basename "$f"):802-11-wireless"; done; true ;;
  '-w 2 device disconnect wlan0') : > "$base/connected" ;;
  '-w 60 device wifi connect Wifi password '*' ifname wlan0')
    uuid=""; for f in "$base"/profiles/*; do [ -f "$f" ] && uuid=$(basename "$f"); done
    [ -n "$uuid" ] || { n=$(($(cat "$base/made" 2>/dev/null || echo 0) + 1)); echo $n > "$base/made"; uuid="uuid-$n"; }
    printf 'Wifi\n%s\n' "$8" > "$base/profiles/$uuid"; printf '[Security]\nPassphrase=%s\n' "$8" > "$base/iwd/Wifi.psk"
    if [ "$8" = "$router" ]; then echo "$uuid" > "$base/connected"; echo "Device 'wlan0' successfully activated with '$uuid'."
    else echo 'Error: Connection activation failed: Secrets were required, but not provided.' >&2; exit 4; fi ;;
  'connection delete uuid '*)
    f="$base/profiles/${4}"
    [ -f "$f" ] || { echo "Error: cannot delete unknown connection(s): uuid '${4}'." >&2; exit 10; }
    rm -f "$f" "$base/iwd/Wifi.psk"
    [ "$(cat "$base/connected" 2>/dev/null)" = "${4}" ] && : > "$base/connected"
    echo "Connection 'Wifi' (${4}) successfully deleted." ;;
  *) echo "unexpected: $*" >&2; exit 42 ;;
esac
"#);
            nmcli
        }

        fn profiles(dir: &TestDir) -> Vec<String> {
            let mut names: Vec<String> = std::fs::read_dir(dir.join("profiles")).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
            names.sort();
            names
        }

        #[test]
        fn a_refused_password_leaves_no_profile_and_the_next_one_typed_connects() {
            // The fresh card of 2026-10-27: a wrong password, then (the owner believes) the right one.
            let dir = TestDir::new("wifi-refused-then-right");
            let nmcli = nm_with_iwd(&dir);
            let err = connect_network(&nmcli, "Wifi", Some("wrong-key"), false).unwrap_err();
            assert_eq!(super::super::connection_error(&err), "Wrong password. Try again.");
            assert!(profiles(&dir).is_empty(), "the refused profile is gone, nothing for iwd to retry or the page to offer as saved");
            assert!(!dir.join("iwd/Wifi.psk").exists(), "and iwd's copy of the refused key with it");
            assert!(std::fs::read_to_string(dir.join("log")).unwrap().contains("connection delete uuid uuid-1"));
            // Nothing is trying in the background: the next password goes straight in, and it is the one used.
            let mut act = Activation { nmcli: &nmcli, started: Instant::now() };
            assert_eq!(crate::wifi_activation::Ops::device(&mut act).unwrap().state, 30);
            assert!(!connect_network(&nmcli, "Wifi", Some("right-key"), false).unwrap());
            assert_eq!(profiles(&dir), ["uuid-2"]);
            assert_eq!(std::fs::read_to_string(dir.join("iwd/Wifi.psk")).unwrap(), "[Security]\nPassphrase=right-key\n");
            assert_eq!(std::fs::read_to_string(dir.join("connected")).unwrap().trim(), "uuid-2");
        }

        #[test]
        fn a_second_wrong_password_is_refused_again_at_once() {
            // The owner wasn't sure the second password was right: if it was wrong too, it says so again, without the
            // six minutes of "connecting" the card showed.
            let dir = TestDir::new("wifi-refused-twice");
            let nmcli = nm_with_iwd(&dir);
            for key in ["wrong-key", "still-wrong"] {
                let started = Instant::now();
                let err = connect_network(&nmcli, "Wifi", Some(key), false).unwrap_err();
                assert_eq!(super::super::connection_error(&err), "Wrong password. Try again.");
                assert!(started.elapsed() < Duration::from_secs(5), "no wait for an activation in the background");
                assert!(profiles(&dir).is_empty());
            }
            let log = std::fs::read_to_string(dir.join("log")).unwrap();
            assert!(!log.contains("device disconnect"), "nothing was left running to tear down: {log}");
        }

        #[test]
        fn a_refused_password_replaces_the_key_of_an_old_profile_and_that_profile_goes_too() {
            // nmcli writes a typed key into the existing profile of the network (`device wifi connect` reuses it).
            let dir = TestDir::new("wifi-refused-old-profile");
            let nmcli = nm_with_iwd(&dir);
            std::fs::write(dir.join("profiles/uuid-old"), "Wifi\nright-key\n").unwrap();
            assert!(connect_network(&nmcli, "Wifi", Some("typo-key"), false).is_err());
            assert!(profiles(&dir).is_empty(), "a profile that now holds the refused key would loop like a new one");
        }

        #[test]
        fn forgetting_a_profile_that_is_already_gone_counts_as_forgotten() {
            let dir = TestDir::new("wifi-forget-twice");
            let nmcli = nm_with_iwd(&dir);
            std::fs::write(dir.join("profiles/072aa1e0"), "Wifi\nright-key\n").unwrap();
            let saved = Saved { id: "Wifi".into(), uuid: "072aa1e0".into(), ssid: "Wifi".into(), ..Saved::default() };
            assert_eq!(forget_with(&nmcli, &saved), Ok(()));
            assert!(profiles(&dir).is_empty());
            // A second Y on a list from before the delete (808 s on the fresh card): no error for the player.
            assert_eq!(forget_with(&nmcli, &saved), Ok(()));
            // Any other refusal is still an error.
            assert!(super::super::already_forgotten("Error: cannot delete unknown connection(s): uuid 'x'."));
            assert!(!super::super::already_forgotten("Error: Connection deletion failed: Insufficient privileges."));
        }

        #[test]
        fn radio_off_cancels_a_running_watchdog_activation_within_one_second() {
            use std::io::Read;
            use std::os::fd::AsRawFd;
            use std::os::unix::net::UnixListener;
            use crate::net_state::{NetState, Radio, fake::FakeNet};
            let dir = TestDir::new("wifi-up-stop");
            let listener = UnixListener::bind(dir.join("ready")).unwrap();
            let nmcli = dir.join("nmcli");
            write_executable(&nmcli, r#"#!/usr/bin/env python3
import pathlib, socket, sys
assert sys.argv[1:] == ['-w', '30', 'connection', 'up', 'uuid', 'uuid-home', 'ifname', 'wlan0']
s = socket.socket(socket.AF_UNIX)
s.connect(str(pathlib.Path(__file__).with_name('ready')))
s.sendall(b'R')
s.recv(1)
"#);
            let net = NetState::new(FakeNet::new(Radio::On));
            let (finished, result) = std::sync::mpsc::channel();
            net.reconnect(move |stop| { finished.send(activate_saved(&nmcli, "uuid-home", Some(&stop))).unwrap(); });
            let mut poll = libc::pollfd { fd: listener.as_raw_fd(), events: libc::POLLIN, revents: 0 };
            // Wait for the fake's readiness socket, never for an assumed startup duration.
            assert_eq!(unsafe { libc::poll(&mut poll, 1, 5000) }, 1, "nmcli did not reach its activation barrier");
            let (mut ready, _) = listener.accept().unwrap();
            ready.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            ready.read_exact(&mut [0]).unwrap();
            net.set_enabled(false);
            assert!(result.recv_timeout(Duration::from_secs(1)).unwrap().unwrap_err().contains("stopped"));
            net.wait_idle();
            assert!(net.snapshot().radio.is_off());
        }

        #[test]
        fn nmcli_connected_ssid_comes_from_uuid_not_profile_label() {
            let dir = TestDir::new("wifi-nm-state");
            let nmcli = dir.join("nmcli");
            write_executable(
                &nmcli,
                r#"#!/bin/sh
case "$*" in
  '-g GENERAL.STATE,GENERAL.CON-UUID,GENERAL.REASON device show wlan0') printf '100 (verbunden)\nuuid-phone\n' ;;
  '-e no -g 802-11-wireless.ssid connection show uuid uuid-phone') printf 'Lab:5G\n' ;;
  *) exit 1 ;;
esac
"#,
            );
            assert!(prepare_connection(&nmcli, "Lab:5G").unwrap(), "already connected, no activation command");
        }

        #[test]
        fn nmcli_switch_disconnects_the_other_ssid_before_one_activation() {
            let dir = TestDir::new("wifi-nm-switch");
            let nmcli = dir.join("nmcli");
            std::fs::write(dir.join("state"), "100 (connected)\nuuid-office\n").unwrap();
            write_executable(
                &nmcli,
                r#"#!/bin/sh
base=$(dirname "$0")
printf '%s\n' "$*" >> "$base/log"
case "$*" in
  '-g GENERAL.STATE,GENERAL.CON-UUID,GENERAL.REASON device show wlan0') cat "$base/state" ;;
  '-e no -g 802-11-wireless.ssid connection show uuid uuid-office') echo 'Office' ;;
  '-w 2 device disconnect wlan0') printf '30 (disconnected)\n--\n' > "$base/state" ;;
  '-w 45 connection up uuid uuid-phone ifname wlan0') : ;;
  *) exit 1 ;;
esac
"#,
            );
            assert!(!prepare_connection(&nmcli, "Xiaomi 15").unwrap());
            Activation {
                nmcli: &nmcli,
                started: Instant::now(),
            }
            .command(&["-w", "45", "connection", "up", "uuid", "uuid-phone", "ifname", "wlan0"])
            .unwrap();
            let log = std::fs::read_to_string(dir.join("log")).unwrap();
            assert_eq!(log.matches("connection up").count(), 1);
            assert!(log.find("device disconnect").unwrap() < log.find("connection up").unwrap(), "{log}");
        }

        #[test]
        fn nmcli_activating_same_ssid_waits_for_success_without_a_second_up() {
            // Use the real nmcli reader with a condition controlled wait, not a wall-clock sleep.
            struct Controlled<'a> {
                inner: Activation<'a>,
                state: std::path::PathBuf,
                clock: Duration,
            }
            impl Ops for Controlled<'_> {
                fn device(&mut self) -> Result<crate::wifi_activation::Device, String> {
                    self.inner.device()
                }
                fn disconnect(&mut self) -> Result<(), String> {
                    panic!("must join the existing activation")
                }
                fn now(&self) -> Duration {
                    self.clock
                }
                fn wait(&mut self) {
                    self.clock += Duration::from_secs(1);
                    std::fs::write(&self.state, "100 (connected)\nuuid-phone\n").unwrap();
                }
            }
            let dir = TestDir::new("wifi-nm-join");
            let nmcli = dir.join("nmcli");
            std::fs::write(dir.join("state"), "50 (config)\nuuid-phone\n").unwrap();
            write_executable(
                &nmcli,
                r#"#!/bin/sh
base=$(dirname "$0")
printf '%s\n' "$*" >> "$base/log"
case "$*" in
  '-g GENERAL.STATE,GENERAL.CON-UUID,GENERAL.REASON device show wlan0') cat "$base/state" ;;
  '-e no -g 802-11-wireless.ssid connection show uuid uuid-phone') echo 'Xiaomi 15' ;;
  *) exit 1 ;;
esac
"#,
            );
            let mut ops = Controlled {
                inner: Activation {
                    nmcli: &nmcli,
                    started: Instant::now(),
                },
                state: dir.join("state"),
                clock: Duration::ZERO,
            };
            assert!(crate::wifi_activation::prepare(&mut ops, "Xiaomi 15").unwrap());
            let log = std::fs::read_to_string(dir.join("log")).unwrap();
            assert_eq!(log.matches("GENERAL.STATE").count(), 2, "the activation was observed until connected");
            assert!(!log.contains("connection up") && !log.contains("disconnect"), "{log}");
        }
    }

    static IP: std::sync::LazyLock<std::sync::Arc<crate::kept::Kept<Option<String>>>> = std::sync::LazyLock::new(|| crate::kept::Kept::new("wifi-ip"));
    const IP_KEPT: Duration = Duration::from_secs(3);
    const IP_TIMEOUT: Duration = Duration::from_secs(1);

    fn ask_ip() -> Option<String> {
        // The virtual device has no Wi-Fi chip: POCKETUI_WIFI_IFACE=eth0 there stands in for it (docs/VIRTUAL-DEVICE.md).
        let iface = std::env::var("POCKETUI_WIFI_IFACE").unwrap_or_else(|_| "wlan0".into());
        let out = crate::proc::run_with_timeout(Command::new("ip").args(["-4", "-o", "addr", "show", &iface]), IP_TIMEOUT).ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        let cidr = text.split_whitespace().skip_while(|w| *w != "inet").nth(1)?;
        Some(cidr.split('/').next()?.to_string())
    }

    /// Turns on the Samba share (`games-roms` → /storage/roms) for this session only, after our
    /// restricted configuration and login are in place. ROCKNIX's `samba.enabled` stays 0, so the
    /// share is off again after a restart.
    pub fn enable_sharing() -> bool {
        if let Err(e) = secure_sharing() {
            eprintln!("file sharing: {e}");
            return false;
        }
        // Games copied earlier as root (or rsync'd from a Mac) become writable for the share user.
        for path in super::share_adoption(Path::new("/storage/roms"), super::SHARE_UID) {
            if let Err(e) = std::os::unix::fs::lchown(&path, Some(super::SHARE_UID), Some(super::SHARE_GID)) {
                eprintln!("file sharing: chown {}: {e}", path.display());
            }
        }
        // A restart, not a start: smbd only binds to the Wi-Fi address it finds when it starts.
        Command::new("systemctl").args(["restart", "nmbd", "smbd"]).status().is_ok_and(|s| s.success())
    }

    pub fn disable_sharing() -> bool {
        samba_setting_off();
        Command::new("systemctl").args(["stop", "smbd", "nmbd"]).status().is_ok_and(|s| s.success())
    }

    /// ROCKNIX starts Samba at boot when this setting is 1; pocket-ui switches it on per session. Written under
    /// ROCKNIX's lock (`system_cfg`), and only when it is on.
    fn samba_setting_off() {
        if crate::system_cfg::get("samba.enabled").is_some_and(|v| v.trim() == "1") {
            crate::system_cfg::set("samba.enabled", "0");
        }
    }

    /// Makes sure only the games are shared and only with the password, also when sharing was
    /// turned on in ROCKNIX's own settings or a system update brought the default configuration
    /// back. A running smbd reloads.
    pub fn secure_sharing() -> std::io::Result<()> {
        let changed = super::secure_share_config(Path::new(super::SMB_CONF_PATH))?;
        super::ensure_share_login(Path::new(super::SHARE_DIR))?;
        samba_setting_off();
        if changed && sharing_enabled() {
            let _ = Command::new("systemctl").args(["reload", "smbd"]).status();
        }
        Ok(())
    }

    pub fn sharing_enabled() -> bool {
        crate::proc::status_with_timeout(Command::new("systemctl").args(["is-active", "--quiet", "smbd"]), crate::proc::HELPER).is_ok_and(|s| s.success())
    }

    /// The share password, once file sharing has been set up on this device.
    pub fn share_password() -> Option<String> {
        crate::secret::load(&Path::new(super::SHARE_DIR).join("password"))
    }

    /// smbd stays on the address it started with (`bind interfaces only`, verified: not even
    /// SIGHUP adds a new one); after a new Wi-Fi address it is restarted so the share is reachable.
    pub fn rebind_sharing() {
        let Some(ip) = ip_now().and_then(|ip| ip.parse().ok()) else { return };
        if sharing_enabled() && !crate::sockets::listening(&crate::sockets::read(), ip, 445) {
            let _ = Command::new("systemctl").args(["restart", "nmbd", "smbd"]).status();
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    //! Desktop preview with a fake neighbourhood.
    use super::{demo_saved, parse_scan, Network, Saved, Saving};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    static SHARING: AtomicBool = AtomicBool::new(false);

    pub fn connected_ssid() -> Option<String> {
        Some("Home".into())
    }

    pub fn scan() -> Vec<Network> {
        std::thread::sleep(Duration::from_millis(800));
        parse_scan("Home:78:WPA2:*\nCafe guest:41::\nNeighbour\\:5G:30:WPA2: \n")
    }

    pub fn scan_checked(_nmcli: &std::path::Path) -> Result<Vec<Network>, crate::net_state::ScanError> {
        Ok(scan())
    }

    pub fn scan_until(nmcli: &std::path::Path, _stop: &std::sync::atomic::AtomicBool) -> Result<Vec<Network>, crate::net_state::ScanError> {
        scan_checked(nmcli)
    }

    pub fn saving_until(_stop: &std::sync::atomic::AtomicBool) -> Saving {
        saving()
    }

    pub fn connect(ssid: &str, psk: Option<&str>) -> Result<(), String> {
        connect_with(ssid, psk, false)
    }

    pub fn connect_with(_ssid: &str, psk: Option<&str>, _hidden: bool) -> Result<(), String> {
        std::thread::sleep(Duration::from_secs(1));
        if psk == Some("wrong") { Err("Secrets were required, but not provided".into()) } else { Ok(()) }
    }

    pub fn set_rocknix_setting(_key: &str, _value: &str) {}

    pub fn saving() -> Saving {
        Saving { saved: demo_saved(), country: None, reg: Some("AT".into()), ip: ip(), mac: Some("00:00:5E:00:53:7F".into()) }
    }

    pub fn connect_saved(_s: &Saved) -> Result<(), String> {
        std::thread::sleep(Duration::from_secs(1));
        Ok(())
    }

    pub fn forget(_s: &Saved) -> Result<(), String> {
        Ok(())
    }

    pub fn set_country(code: Option<&str>) -> Result<(), String> {
        if code.is_some_and(|c| !super::valid_country(c)) { Err("That is not a country code.".into()) } else { Ok(()) }
    }

    pub fn ip() -> Option<String> {
        Some("192.0.2.23".into())
    }

    pub fn ip_now() -> Option<String> {
        ip()
    }

    pub fn enable_sharing() -> bool {
        std::thread::sleep(Duration::from_millis(500));
        SHARING.store(true, Ordering::Relaxed);
        true
    }

    pub fn disable_sharing() -> bool {
        SHARING.store(false, Ordering::Relaxed);
        true
    }

    pub fn secure_sharing() -> std::io::Result<()> {
        Ok(())
    }

    pub fn sharing_enabled() -> bool {
        SHARING.load(Ordering::Relaxed)
    }

    pub fn share_password() -> Option<String> {
        Some("maple-river-stone-482".into())
    }

    pub fn rebind_sharing() {}

    pub fn watchdog(_net: std::sync::Arc<crate::net_state::NetState>) {}
}

pub use imp::{connected_ssid, watchdog, connect, disable_sharing, enable_sharing, ip, ip_now, rebind_sharing, scan, scan_checked, secure_sharing, share_password, sharing_enabled};
pub use imp::{connect_saved, connect_with, forget, saving, saving_until, scan_until, set_country, set_rocknix_setting};

/// Two saved networks for the desktop preview and `--snapshot`: the one in use, and one out of reach.
pub fn demo_saved() -> Vec<Saved> {
    vec![
        Saved { id: "Home".into(), uuid: "0b5c3b1e-home".into(), ssid: "Home".into(), active: true, connecting: false, hidden: false },
        Saved { id: "Office".into(), uuid: "7d1e2f30-office".into(), ssid: "Office".into(), active: false, connecting: false, hidden: false },
        Saved { id: "Phone hotspot".into(), uuid: "a1b2c3d4-phone".into(), ssid: "Phone hotspot".into(), active: false, connecting: false, hidden: true },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    #[test]
    fn the_scan_dump_gives_each_access_points_name_and_age() {
        let dump = "BSS 00:00:5e:00:53:01(on wlan0)\n\tTSF: 0 usec\n\tlast seen: 355026.077s [boottime]\n\tlast seen: 6088 ms ago\n\
                    \tSSID: Phone hotspot\n\t\t * SSID List\nBSS 00:00:5e:00:53:02(on wlan0) -- associated\n\tlast seen: 120 ms ago\n\
                    \tSSID: \\x20Caf\\xc3\\xa9\\x5c5G\nBSS 00:00:5e:00:53:03(on wlan0)\n\tSSID: no age\n";
        assert_eq!(
            parse_scan_dump(dump),
            vec![("Phone hotspot".to_string(), 6088), (" Café\\5G".to_string(), 120)],
            "iw's escapes undone; an entry without an age is left out"
        );
        assert!(parse_scan_dump("").is_empty());
    }

    #[test]
    fn only_networks_heard_since_the_drop_count_as_in_range() {
        let net = |ssid: &str| Network { ssid: ssid.into(), signal: 80, secure: true, active: false };
        let listed = [net("Phone hotspot"), net("Home")];
        // The hotspot went away 20 s ago; nmcli still lists it, the radio last heard it 21 s ago.
        let heard = [("Phone hotspot".to_string(), 21_000), ("Home".to_string(), 3_000)];
        assert_eq!(heard_within(&listed, &heard, 20_000), vec![net("Home")]);
        assert!(heard_within(&listed, &[], 20_000).is_empty(), "nothing heard");
        assert_eq!(heard_within(&listed, &heard, 30_000).len(), 2, "both heard since a drop 30 s ago");
    }

    #[test]
    fn the_watchdog_tries_at_once_when_a_saved_network_is_back_and_never_without_one() {
        assert!(reconnect_now(Some(true), true, false), "the hotspot is back: now, not after the pause");
        assert!(!reconnect_now(Some(true), false, false), "still there, pause not over");
        assert!(reconnect_now(Some(true), false, true), "pause over");
        assert!(!reconnect_now(Some(false), false, true), "nothing heard: a try could only fail (and make iwd shun it)");
        assert!(reconnect_now(None, false, true), "iw can't tell: the pauses decide, as before");
        assert!(!reconnect_now(None, false, false));
    }

    #[test]
    fn a_saved_network_that_isnt_heard_any_more_is_out_of_range_not_a_wrong_password() {
        let heard = [("Home".to_string(), 2_000), ("Phone hotspot".to_string(), 25_000)];
        assert!(gone("Phone hotspot", true, Some(&heard)), "still listed by nmcli, but not heard for 25 s");
        assert!(gone("Phone hotspot", false, None), "a fresh scan doesn't list it");
        assert!(!gone("Home", true, Some(&heard)), "there: the password really was refused");
        assert!(!gone("Home", true, None), "iw can't tell and nmcli lists it: as before");
        assert_eq!(connection_error("No network with SSID 'Phone hotspot' found."), "That network is out of range. Scan again.");
    }

    #[test]
    fn the_network_in_use_is_the_active_line() {
        assert_eq!(parse_active_ssid("no:Cafe guest\nyes:Home net\nno:Neighbour\n").as_deref(), Some("Home net"));
        assert_eq!(parse_active_ssid("no:Cafe\\:5G\nyes:Lab\\:5G\n").as_deref(), Some("Lab:5G"), "escaped colons");
        assert_eq!(parse_active_ssid("no:Cafe guest\n"), None, "nothing in use");
        assert_eq!(parse_active_ssid("yes:\n"), None, "a hidden network without a name");
        assert_eq!(parse_active_ssid(""), None);
    }

    #[test]
    fn a_key_emulationstation_emptied_comes_back_from_the_profile() {
        let emptied = parse_rocknix_wifi("wifi.enabled=1\nwifi.ssid=Home net\nwifi.key=\n");
        assert_eq!(key_to_restore("sec ret:\\x ", &emptied), Some("sec ret:\\x "), "the profile's key, spaces and all");
        let other = RocknixWifi { key: "old".into(), ..emptied.clone() };
        assert_eq!(key_to_restore("new", &other), Some("new"), "the profile is what connects: it wins");
        let same = RocknixWifi { key: "same".into(), ..emptied.clone() };
        assert_eq!(key_to_restore("same", &same), None, "both agree: nothing written");
        assert_eq!(key_to_restore("", &emptied), None, "an open network (or a keyless profile) gives nothing back");
        assert_eq!(key_to_restore("x", &RocknixWifi::default()), None, "no network in system.cfg");
        assert_eq!(key_to_restore("a\nb", &emptied), None, "never a line break into system.cfg");
        // And then the keyless-profile repair has the key to work with.
        let restored = parse_rocknix_wifi(&crate::system_cfg::with_setting("wifi.enabled=1\nwifi.ssid=Home net\nwifi.key=\n", "wifi.key", "secret"));
        assert!(profile_needs_key("", &restored));
    }

    #[test]
    fn reads_rocknix_wifi_settings_and_spots_a_keyless_profile() {
        let w = parse_rocknix_wifi("audio.volume=0\nwifi.enabled=1\nwifi.ssid=Home net\nwifi.key=a=b c\n");
        assert_eq!(w, RocknixWifi { enabled: true, ssid: "Home net".into(), key: "a=b c".into() });
        assert!(profile_needs_key("", &w), "no security section, key known: repair");
        assert!(profile_needs_key("  \n", &w));
        assert!(!profile_needs_key("wpa-psk", &w), "a working profile is left alone");
        assert!(!profile_needs_key("", &RocknixWifi { key: String::new(), ..w.clone() }), "an open network has no key");
        assert!(!parse_rocknix_wifi("wifi.enabled=0\n").enabled);
    }

    #[test]
    fn picks_home_first_then_the_strongest_saved_network_in_range() {
        let net = |ssid: &str, signal: u8| Network { ssid: ssid.into(), signal, secure: true, active: false };
        let saved: Vec<String> = ["Wifi", "Xiaomi 15", "hifiteameg"].iter().map(|s| s.to_string()).collect();
        let home = [net("Wifi", 40), net("Xiaomi 15", 90), net("Cafe", 99)];
        assert_eq!(pick_network(&saved, &home, "Wifi").as_deref(), Some("Wifi"), "ROCKNIX's own network first");
        let away = [net("Cafe", 99), net("Xiaomi 15", 60), net("hifiteameg", 30)];
        assert_eq!(pick_network(&saved, &away, "Wifi").as_deref(), Some("Xiaomi 15"), "the strongest saved one");
        assert_eq!(pick_network(&saved, &[net("Cafe", 99)], "Wifi"), None, "never an unknown network");
        assert_eq!(pick_network(&saved, &away, "").as_deref(), Some("Xiaomi 15"));
    }

    #[test]
    fn reconnects_soon_then_every_five_minutes() {
        let secs: Vec<u64> = (0..6).map(|a| reconnect_delay(a).as_secs()).collect();
        assert_eq!(secs, [30, 60, 120, 300, 300, 300]);
    }

    #[test]
    fn a_profile_without_security_is_replaced_not_reused() {
        // What nmcli said on the Mini V2 after EmulationStation had rebuilt the profile from system.cfg without a key.
        assert!(stale_profile("Error: Failed to modify connection 'Home': 802-11-wireless-security.key-mgmt: property is missing."));
        assert!(stale_profile("Error: Connection activation failed: 802-11-wireless-security.key-mgmt: property is missing"));
        assert!(!stale_profile("Error: Connection activation failed: Secrets were required, but not provided."), "a wrong password stays an error");
        assert!(!stale_profile("Error: No network with SSID 'Home' found."));
    }

    #[test]
    fn rocknix_learns_the_key_too_so_it_can_rebuild_the_profile() {
        // ROCKNIX's wifictl recreates the NetworkManager profile from wifi.ssid and wifi.key; with the name alone
        // it made an open-network profile that NetworkManager then refused.
        assert_eq!(rocknix_settings("Home", Some("secret pass")), [("wifi.enabled", "1"), ("wifi.ssid", "Home"), ("wifi.key", "secret pass")]);
        assert_eq!(rocknix_settings("Cafe", None), [("wifi.enabled", "1"), ("wifi.ssid", "Cafe"), ("wifi.key", "")]);
    }

    #[test]
    fn shares_only_the_games() {
        assert_eq!(smb_sections(SMB_CONF), ["global", "games-roms"]);
        assert!(!SMB_CONF.to_lowercase().contains("/storage/.update"));
        assert!(!SMB_CONF.to_lowercase().contains("/storage/.config\n"));
    }

    /// `[section] -> key -> value`, keys and values lowercased with single spaces, comments dropped.
    fn parse_conf(conf: &str) -> std::collections::HashMap<String, std::collections::HashMap<String, String>> {
        let norm = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
        let mut out = std::collections::HashMap::new();
        let mut section = String::new();
        for line in conf.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with(';')) {
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                section = norm(name);
            } else if let Some((k, v)) = line.split_once('=') {
                out.entry(section.clone()).or_insert_with(std::collections::HashMap::new).insert(norm(k), norm(v));
            }
        }
        out
    }

    /// K1 (the internal review): nobody on the Wi-Fi may write as root, without a password,
    /// into folders root runs code from.
    #[test]
    fn share_is_not_root_and_needs_a_password() {
        let conf = parse_conf(SMB_CONF);
        let global = &conf["global"];
        let share = &conf["games-roms"];
        let get = |m: &std::collections::HashMap<String, String>, k: &str| m.get(k).cloned().unwrap_or_default();

        assert_ne!(get(global, "guest account"), "root", "guest mapped to root");
        let user = get(share, "force user");
        assert!(!user.is_empty() && user != "root" && user != "0", "share must write as a non-root user, not {user:?}");
        assert!(!get(share, "force group").is_empty() && get(share, "force group") != "root");

        // A password is required: no guest fallback, no public share, only our user, our own passdb.
        assert_eq!(get(global, "map to guest"), "never");
        assert_eq!(get(global, "restrict anonymous"), "2");
        for key in ["public", "guest ok"] {
            assert_ne!(get(share, key), "yes", "{key} = yes lets anyone in");
        }
        assert!(!get(share, "valid users").is_empty(), "only the share user may log in");
        let passdb = get(global, "smb passwd file");
        assert!(passdb.starts_with("/storage/.config/pocket-ui/"), "our own smbpasswd, not ROCKNIX's: {passdb:?}");
        assert!(get(global, "invalid users").contains("root"));

        // Code-bearing folders are not reachable over the share.
        let veto = get(share, "veto files");
        for dir in ["ports", "portmaster", "bios", "savestates"] {
            assert!(veto.split('/').any(|v| v == dir), "{dir}/ must be vetoed, veto files = {veto:?}");
        }
        assert_ne!(get(share, "delete veto files"), "yes");

        assert_eq!(get(global, "interfaces"), "wlan0");
        assert_eq!(get(global, "bind interfaces only"), "yes");
        assert_eq!(get(global, "smb encrypt"), "desired");
    }

    #[test]
    fn nt_hash_matches_samba() {
        let hex = |b: [u8; 16]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
        assert_eq!(hex(md4(b"")), "31d6cfe0d16ae931b73c59d7e0c089c0");
        assert_eq!(hex(md4(b"abc")), "a448017aaf21d8525fc10ae87aa6729d");
        assert_eq!(hex(md4(b"12345678901234567890123456789012345678901234567890123456789012345678901234567890")), "e33b4ddc9c38f2199c3e7b164fcc0536");
        assert_eq!(nt_hash("password"), "8846F7EAEE8FB117AD06BDD830B7586C");
        // Written by ROCKNIX's own `smbpasswd -s -a nobody` (Samba 4.21) for this password.
        assert_eq!(nt_hash("test-pass-1234"), "3ABD62146CD2729B01299DFC3E45ED81");
        assert_eq!(
            smbpasswd_line("nobody", 65534, "test-pass-1234", 0x6AD9BC67),
            "nobody:65534:XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX:3ABD62146CD2729B01299DFC3E45ED81:[U          ]:LCT-6AD9BC67:\n"
        );
    }

    #[test]
    fn creates_the_share_login_once() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TestDir::new("share");
        let pw = ensure_share_login(&dir).unwrap();
        let passdb = std::fs::read_to_string(dir.join("smbpasswd")).unwrap();
        assert!(passdb.starts_with(&format!("nobody:65534:{}:{}:[U", "X".repeat(32), nt_hash(&pw))), "{passdb}");
        assert_eq!(passdb.lines().count(), 1, "only our user can log in");
        assert_eq!(std::fs::read_to_string(dir.join("users.map")).unwrap(), "nobody = pocket\n");
        for f in ["password", "smbpasswd", "users.map"] {
            assert_eq!(std::fs::metadata(dir.join(f)).unwrap().permissions().mode() & 0o777, 0o600, "{f}");
        }
        assert_eq!(ensure_share_login(&dir).unwrap(), pw, "same password on every start");
        assert_eq!(std::fs::read_to_string(dir.join("smbpasswd")).unwrap(), passdb, "left alone when up to date");
        // Someone added root (ROCKNIX's setrootpass does that to its own file): rewritten.
        std::fs::write(dir.join("smbpasswd"), format!("{passdb}root:0:X:Y:[U          ]:LCT-0:\n")).unwrap();
        ensure_share_login(&dir).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("smbpasswd")).unwrap().lines().count(), 1);
    }

    #[test]
    fn adopts_games_but_not_code_folders_or_links() {
        let dir = TestDir::new("adopt");
        for d in ["gba/Hacks", "ports/PortMaster", "PortMaster", "BIOS", "savestates/gba"] {
            std::fs::create_dir_all(dir.join(d)).unwrap();
        }
        std::fs::write(dir.join("gba/a.gba"), "x").unwrap();
        std::fs::write(dir.join("ports/evil.sh"), "x").unwrap();
        std::os::unix::fs::symlink("/etc", dir.join("gba/link")).unwrap();
        let mut got: Vec<String> =
            share_adoption(&dir, u32::MAX).iter().map(|p| p.strip_prefix(&dir).unwrap().to_string_lossy().into_owned()).collect();
        got.sort();
        assert_eq!(got, ["gba", "gba/Hacks", "gba/a.gba"]);
        assert!(share_adoption(&dir, unsafe { libc::getuid() }).is_empty(), "nothing left once owned");
    }

    #[test]
    fn replaces_rocknix_config_and_keeps_the_original() {
        let dir = TestDir::new("smb");
        let path = dir.join("smb.conf");
        std::fs::write(&path, "[global]\n[update]\n  path = /storage/.update\n[config]\n").unwrap();
        assert!(secure_share_config(&path).unwrap());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), SMB_CONF);
        assert!(std::fs::read_to_string(dir.join("smb.conf.rocknix")).unwrap().contains("[update]"));
        assert!(!secure_share_config(&path).unwrap(), "nothing to do the second time");
        // A system update that restores the default is undone, the first backup stays.
        std::fs::write(&path, "[global]\n[logs]\n").unwrap();
        assert!(secure_share_config(&path).unwrap());
        assert!(std::fs::read_to_string(dir.join("smb.conf.rocknix")).unwrap().contains("[update]"));
    }

    #[test]
    fn saved_profiles_come_from_nmcli_without_rocknixs_netplay_ap() {
        // `nmcli -t -f NAME,UUID,TYPE,ACTIVE connection show` on ROCKNIX (names with colons are escaped).
        let out = "Home:0b5c-1:802-11-wireless:yes\nlo:9a8b:loopback:yes\nNETPLAY_AP:77aa:802-11-wireless:no\nCafe\\:5G:c0ff:802-11-wireless:no\n";
        let saved = parse_profiles(out);
        assert_eq!(saved.len(), 2);
        assert_eq!((saved[0].id.as_str(), saved[0].uuid.as_str(), saved[0].active), ("Home", "0b5c-1", true));
        assert_eq!((saved[1].ssid.as_str(), saved[1].active), ("Cafe:5G", false));
        assert_eq!(parse_ssid_hidden("Home net\nyes\n"), Some(("Home net".into(), true)));
        assert_eq!(parse_ssid_hidden("Home\nno\n"), Some(("Home".into(), false)));
        assert_eq!(parse_ssid_hidden(""), None);
    }

    #[test]
    fn only_an_activated_profile_is_in_use_an_activating_one_is_connecting() {
        let out = "Wifi:072aa1e0:802-11-wireless:yes:activating\nCafe:4:802-11-wireless:no:\nHome:1:802-11-wireless:yes:activated\n";
        let saved = parse_profiles(out);
        assert_eq!(saved.iter().map(|s| (s.ssid.as_str(), s.active, s.connecting)).collect::<Vec<_>>(), [("Wifi", false, true), ("Cafe", false, false), ("Home", true, false)]);
        // The scan marks the access point from the first moment of the activation; the profile decides.
        let nets = parse_scan("Wifi:70:WPA2:*\n");
        let entries = merge(&nets, &saved[..1]);
        assert!(!entries[0].active, "a refused password retried by iwd is not a connection");
        // Without the profile list (nmcli didn't answer) the scan's mark is all there is.
        assert!(merge(&nets, &[])[0].active);
    }

    #[test]
    fn the_list_has_the_network_in_use_first_then_reach_then_saved_ones_away() {
        let nets = parse_scan("Cafe:41::\nHome:78:WPA2:*\nNeighbour:60:WPA2: \n");
        let saved = vec![
            Saved { id: "Home".into(), uuid: "1".into(), ssid: "Home".into(), active: true, connecting: false, hidden: false },
            Saved { id: "Zoo".into(), uuid: "2".into(), ssid: "Zoo".into(), ..Saved::default() },
            Saved { id: "Attic".into(), uuid: "3".into(), ssid: "Attic".into(), ..Saved::default() },
            Saved { id: "Cafe".into(), uuid: "4".into(), ssid: "Cafe".into(), ..Saved::default() },
        ];
        let list = merge(&nets, &saved);
        let names: Vec<&str> = list.iter().map(|e| e.ssid.as_str()).collect();
        assert_eq!(names, ["Home", "Neighbour", "Cafe", "Attic", "Zoo"]);
        assert!(list[0].active && list[0].saved.is_some() && list[0].signal == Some(78));
        assert!(list[1].saved.is_none());
        assert!(list[2].saved.is_some() && !list[2].secure, "an open network that was saved");
        assert_eq!(list[3].signal, None, "out of reach");
    }

    #[test]
    fn the_wifi_country_is_a_real_code_and_kept_for_the_next_start() {
        assert!(valid_country("AT") && valid_country("US"));
        for bad in ["00", "at", "AUT", "", "A", "A;", "../"] {
            assert!(!valid_country(bad), "{bad}");
        }
        assert_eq!(country_setting("wifi.enabled=1\nwifi.country=DE\n"), Some("DE".into()));
        assert_eq!(country_setting("wifi.country=\n"), None);
        assert_eq!(country_setting("wifi.country=; reboot\n"), None);
        assert!(regdomain_conf("AT").lines().any(|l| l == "REGDOMAIN=AT"), "the line ROCKNIX's setregdomain sources");
        let list = countries("# comment\nDE\tGermany\nAT\tAustria\nXX\t\nzz\tBad\n");
        assert_eq!(list, [("AT".to_string(), "Austria".to_string()), ("DE".to_string(), "Germany".to_string())]);
    }

    #[test]
    fn parses_escaped_and_duplicate_networks() {
        let nets = parse_scan("Home:78:WPA2:*\nCafe guest:41::\nNeighbour\\:5G:30:WPA2: \nHome:60:WPA2: \n:12:WPA2: \n");
        let names: Vec<_> = nets.iter().map(|n| n.ssid.as_str()).collect();
        assert_eq!(names, ["Home", "Cafe guest", "Neighbour:5G"]);
        assert!(nets[0].active && nets[0].secure && nets[0].signal == 78);
        assert!(!nets[1].secure);
    }

    #[test]
    fn the_wifi_address_is_read_as_the_chip_has_it_and_nothing_else() {
        assert_eq!(parse_mac("00:00:5e:00:53:7f\n"), Some("00:00:5E:00:53:7F".into()));
        assert_eq!(parse_mac("00:00:00:00:00:00\n"), None, "no chip");
        assert_eq!(parse_mac(""), None);
        assert_eq!(parse_mac("00:00:5e:00:53"), None);
        assert_eq!(parse_mac("00:00:5e:00:53:zz"), None);
        assert_eq!(parse_mac("00:00:5e:00:53:7f:00"), None);
    }
}
