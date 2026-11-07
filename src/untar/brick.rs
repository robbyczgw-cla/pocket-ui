//! Streaming extraction for the Brick card; the ROCKNIX parser remains in the parent module.
use super::*;
use std::io::Read;

/// Why a tar could not be read or unpacked (`walk`, `unpack_file`).
#[derive(Debug)]
pub enum Refused {
    /// Links, devices, unsafe paths, data that tars would read differently.
    Unsafe,
    /// Not a tar or not a .tar.gz, cut short, or larger than allowed.
    Damaged(&'static str),
    /// The target ran out of space (ENOSPC, or the file size limit).
    Full,
    /// Any other failure writing the target.
    Io(String),
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refused::Unsafe => f.write_str(UNSAFE),
            Refused::Damaged(why) => f.write_str(why),
            Refused::Full => f.write_str("No space left to unpack the download."),
            Refused::Io(e) => f.write_str(e),
        }
    }
}

const CUT_SHORT: &str = "The download is not a valid .tar.gz.";

impl From<std::io::Error> for Refused {
    fn from(e: std::io::Error) -> Self {
        match e.raw_os_error() {
            Some(libc::ENOSPC) | Some(libc::EFBIG) | Some(libc::EDQUOT) => Refused::Full,
            _ if e.kind() == std::io::ErrorKind::UnexpectedEof => Refused::Damaged(CUT_SHORT),
            _ => Refused::Io(e.to_string()),
        }
    }
}

/// One 512-byte block; `Ok(false)` at a clean end of the stream, an error when it ends inside a block.
fn block(r: &mut dyn Read, buf: &mut [u8; 512]) -> Result<bool, Refused> {
    let mut got = 0;
    while got < 512 {
        match r.read(&mut buf[got..]) {
            Ok(0) if got == 0 => return Ok(false),
            Ok(0) => return Err(Refused::Unsafe),
            Ok(n) => got += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(true)
}

/// Reads an uncompressed tar from `r` as it comes, with the rules of `entries`, and hands every file and folder to `each` with
/// its mode bits from the header and its data (exactly the entry's bytes; what `each` leaves unread is skipped). One pass, nothing
/// held in memory but small headers.
pub fn walk(r: &mut dyn Read, mut each: impl FnMut(&Entry, u32, &mut dyn Read) -> Result<(), Refused>) -> Result<Vec<Entry>, Refused> {
    let mut out = Vec::new();
    let mut header = [0u8; 512];
    let mut ended = false;
    // Set by a GNU long-name ('L') or pax ('x') header for the entry that follows.
    let mut long_name: Option<String> = None;
    let mut pax_path: Option<String> = None;
    let mut pax_size: Option<u64> = None;
    let unsafe_ = |_| Refused::Unsafe;
    while block(r, &mut header)? {
        if header.iter().all(|&b| b == 0) {
            ended = true;
            continue;
        }
        // Tars stop at the end marker; something hidden behind it is refused rather than ignored.
        if ended {
            return Err(Refused::Unsafe);
        }
        let sum: u64 = header.iter().enumerate().map(|(i, &b)| if (148..156).contains(&i) { 32 } else { b as u64 }).sum();
        // A size in base-256 (files over 8 GB) or a bad checksum isn't anything we publish.
        if header[124] & 0x80 != 0 || octal(&header[148..156]) != Some(sum) {
            return Err(Refused::Unsafe);
        }
        let size = octal(&header[124..136]).ok_or(Refused::Unsafe)?;
        let padding = size.div_ceil(512) * 512 - size;
        let kind = header[156];
        if matches!(kind, b'L' | b'x' | b'g') {
            // Small records; anything big here is not from a tar we made.
            if size > 1 << 20 {
                return Err(Refused::Unsafe);
            }
            let mut data = vec![0u8; size as usize];
            r.read_exact(&mut data).map_err(|_| Refused::Unsafe)?;
            std::io::copy(&mut (&mut *r).take(padding), &mut std::io::sink())?;
            if kind == b'L' {
                if long_name.is_some() {
                    return Err(Refused::Unsafe);
                }
                long_name = Some(c_string(&data).map_err(unsafe_)?);
                continue;
            }
            for (k, v) in pax_records(&data).map_err(unsafe_)? {
                match k.as_str() {
                    // A global path or size would apply to every entry: never in our packages.
                    "path" | "size" if kind == b'g' => return Err(Refused::Unsafe),
                    "path" => pax_path = Some(utf8(v).map_err(unsafe_)?),
                    // Not every tar reads a pax size; the header has to say the same.
                    "size" => pax_size = Some(utf8(v).map_err(unsafe_)?.parse().map_err(|_| Refused::Unsafe)?),
                    _ => {}
                }
            }
            continue;
        }
        if pax_size.take().is_some_and(|s| s != size) {
            return Err(Refused::Unsafe);
        }
        let ustar = &header[257..263] == b"ustar\0";
        let mut name = c_string(&header[..100]).map_err(unsafe_)?;
        if ustar && header[345] != 0 {
            name = format!("{}/{name}", c_string(&header[345..500]).map_err(unsafe_)?);
        }
        let name = match (pax_path.take(), long_name.take()) {
            (Some(_), Some(_)) => return Err(Refused::Unsafe),
            (Some(n), None) | (None, Some(n)) => n,
            (None, None) => name,
        };
        let dir = match kind {
            // Old tars mark folders with a trailing slash only.
            b'0' | 0 | b'7' => name.ends_with('/'),
            b'5' => true,
            // '1' hardlink, '2' symlink, '3'/'4' devices, '6' FIFO, 'K' long link name, 'S' sparse, …
            _ => return Err(Refused::Unsafe),
        };
        if dir && size != 0 {
            return Err(Refused::Unsafe);
        }
        let entry = match clean_path(&name).map_err(unsafe_)? {
            Some(path) => Entry { path, dir },
            None if dir => continue,
            None => return Err(Refused::Unsafe),
        };
        let mode = octal(&header[100..108]).unwrap_or(0) as u32;
        let mut data = (&mut *r).take(size);
        each(&entry, mode, &mut data)?;
        // What `each` did not read, and the padding: a stream that ends early is cut short.
        std::io::copy(&mut data, &mut std::io::sink())?;
        if data.limit() != 0 || std::io::copy(&mut (&mut *r).take(padding), &mut std::io::sink())? != padding {
            return Err(Refused::Unsafe);
        }
        out.push(entry);
    }
    if long_name.is_some() || pax_path.is_some() || pax_size.is_some() {
        return Err(Refused::Unsafe);
    }
    Ok(out)
}

/// A .tar.gz (one or more gzip members, as `gzip -dc` reads them) or a plain .tar, decompressed as it is read.
pub struct Gz<R: Read> {
    inner: std::io::BufReader<R>,
    state: Option<Box<miniz_oxide::inflate::stream::InflateState>>,
    /// Not compressed at all: passed through.
    plain: bool,
    done: bool,
    out: Vec<u8>,
    out_at: usize,
    out_len: usize,
    total: u64,
    limit: u64,
}

impl<R: Read> Gz<R> {
    /// Fails with `Damaged` once more than `limit` bytes came out.
    pub fn new(inner: R, limit: u64) -> std::io::Result<Self> {
        let mut inner = std::io::BufReader::with_capacity(1 << 16, inner);
        let plain = !std::io::BufRead::fill_buf(&mut inner)?.starts_with(&[0x1f, 0x8b]);
        Ok(Gz { inner, state: None, plain, done: false, out: vec![0u8; 1 << 16], out_at: 0, out_len: 0, total: 0, limit })
    }

    fn damaged() -> std::io::Error {
        std::io::Error::new(std::io::ErrorKind::UnexpectedEof, CUT_SHORT)
    }

    /// Reads a member's header; false at the end of the stream (nothing more, or only zero padding).
    fn next_member(&mut self) -> std::io::Result<bool> {
        use std::io::BufRead;
        loop {
            let buf = self.inner.fill_buf()?;
            if buf.is_empty() {
                return Ok(false);
            }
            // gzip ignores zero padding at the end; anything else has to be another member.
            if buf[0] != 0 {
                break;
            }
            let zeros = buf.iter().take_while(|&&b| b == 0).count();
            self.inner.consume(zeros);
        }
        let mut h = [0u8; 10];
        self.inner.read_exact(&mut h).map_err(|_| Self::damaged())?;
        if h[0] != 0x1f || h[1] != 0x8b || h[2] != 8 {
            return Err(Self::damaged());
        }
        let flags = h[3];
        if flags & 4 != 0 {
            let mut xlen = [0u8; 2];
            self.inner.read_exact(&mut xlen)?;
            std::io::copy(&mut (&mut self.inner).take(u16::from_le_bytes(xlen) as u64), &mut std::io::sink())?;
        }
        for bit in [8u8, 16] {
            if flags & bit != 0 {
                let mut skipped = Vec::new();
                self.inner.read_until(0, &mut skipped)?;
                if skipped.last() != Some(&0) {
                    return Err(Self::damaged());
                }
            }
        }
        if flags & 2 != 0 {
            self.inner.read_exact(&mut [0u8; 2])?;
        }
        self.state = Some(miniz_oxide::inflate::stream::InflateState::new_boxed(miniz_oxide::DataFormat::Raw));
        Ok(true)
    }
}

impl<R: Read> Read for Gz<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        use miniz_oxide::inflate::stream::inflate;
        use miniz_oxide::{MZFlush, MZStatus};
        use std::io::BufRead;
        if self.plain {
            return self.inner.read(buf);
        }
        loop {
            if self.out_at < self.out_len {
                let n = buf.len().min(self.out_len - self.out_at);
                buf[..n].copy_from_slice(&self.out[self.out_at..self.out_at + n]);
                self.out_at += n;
                return Ok(n);
            }
            if self.done || buf.is_empty() {
                return Ok(0);
            }
            if self.state.is_none() && !self.next_member()? {
                self.done = true;
                return Ok(0);
            }
            let input = self.inner.fill_buf()?;
            let state = self.state.as_mut().expect("a member is open");
            let r = inflate(state, input, &mut self.out, MZFlush::None);
            self.inner.consume(r.bytes_consumed);
            self.out_at = 0;
            self.out_len = r.bytes_written;
            self.total += r.bytes_written as u64;
            if self.total > self.limit {
                return Err(std::io::Error::other("The download is too big to unpack."));
            }
            match r.status {
                Ok(MZStatus::StreamEnd) => {
                    // CRC32 and length follow the deflate stream (the whole file's sha256 was checked before).
                    self.inner.read_exact(&mut [0u8; 8]).map_err(|_| Self::damaged())?;
                    self.state = None;
                }
                Ok(_) if r.bytes_consumed > 0 || r.bytes_written > 0 => {}
                // No progress: the input ended inside a member, or it is not deflate.
                _ => return Err(Self::damaged()),
            }
        }
    }
}

/// Unpacks the .tar.gz (or .tar) at `archive` into `into`, which must not exist yet, as it reads it: for the Brick card's program
/// folder (about 150 MB on a device with 1 GB of memory). The rules of `entries`; a file gets its mode when it is created (0755
/// when the tar marks it executable, else 0644) and is never chmod'ed afterwards, because the card's FAT refuses mode changes.
/// At most `limit` bytes come out. On any failure `into` is removed again; `Refused::Full` when the target ran out of space.
pub fn unpack_file(archive: &Path, into: &Path, limit: u64) -> Result<Vec<Entry>, Refused> {
    unpack_inner(archive, into, limit, None)
}

/// Bulk SD writes are bounded to 2 MiB/s, with at most 256 KiB between fsyncs.
pub fn unpack_file_throttled(archive: &Path, into: &Path, limit: u64) -> Result<Vec<Entry>, Refused> {
    unpack_inner(archive, into, limit, Some(crate::update::brick_io::Pacer::new()))
}

fn unpack_inner(archive: &Path, into: &Path, limit: u64, mut pacer: Option<crate::update::brick_io::Pacer>) -> Result<Vec<Entry>, Refused> {
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    let mut gz = Gz::new(std::fs::File::open(archive)?, limit)?;
    std::fs::DirBuilder::new().mode(0o755).create(into)?;
    let result = walk(&mut gz, |e, mode, data| {
        let path = into.join(&e.path);
        if e.dir {
            std::fs::DirBuilder::new().recursive(true).mode(0o755).create(&path)?;
            return Ok(());
        }
        if let Some(parent) = path.parent() {
            std::fs::DirBuilder::new().recursive(true).mode(0o755).create(parent)?;
        }
        let mode = if mode & 0o111 != 0 { 0o755 } else { 0o644 };
        let mut f = std::fs::OpenOptions::new().write(true).create_new(true).mode(mode).open(&path)?;
        if let Some(pacer) = pacer.as_mut() {
            pacer.copy(data, &mut f)?;
        } else {
            std::io::copy(data, &mut f)?;
        }
        f.sync_all()?;
        Ok(())
    });
    if result.is_err() {
        let _ = std::fs::remove_dir_all(into);
    }
    result
}
