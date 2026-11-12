#!/usr/bin/env python3
"""Turn a POCKETUI_FRAME_DUMP folder (src/frame_dump.rs, docs/dev/environment.md) into one 60 fps picture track.

    frame-dump-decode.py <dir> --mp4 out.mp4     H.264 through ffmpeg, every tick one frame at exactly 60 fps
    frame-dump-decode.py <dir> --png <outdir>    one PNG per tick (needs ffmpeg too), e.g. with --from 90 --to 140
    frame-dump-decode.py <dir> --raw > out.rgb   raw rgb24 frames on stdout (the size is printed on stderr)
    frame-dump-decode.py <dir> --sound out.mp4   only out.mp4's events.txt and game sound again (no ffmpeg)

pocket-ui's frames are on its fixed clock, one tick per 16.667 ms (the UI's frame, played at 60 fps); a tick without a record shows the frame before. The
games pocket-core ran during the dump are in <dir>/core-<pid>/ (each picture it put on screen is one tick): a run of
them replaces the UI's ticks from the real moment it began, and the UI goes on from the real moment it ended (a pause
of more than 0.1 s, the pause menu, ends a run). Next to the video, <out>.events.txt lists the inputs and UI sounds
at their second on this track (what pocket-ui logged during a game at the game's frame of that moment);
<out>.game-<pid>-<n>.wav is each game run's own sound, with its start second, from its first picture on (the sound the
game made hidden behind the launch animation, silent on the device, is left out).
"""
import argparse, collections, glob, os, struct, subprocess, sys, zlib

TICK = 16_667_000  # ns, frame_dump::TICK_NS


def records(path):
    """(tick, t_ns, real_ns, w, h, xor, zlib bytes) of a frames.pfd, in order."""
    data = open(path, 'rb').read()
    if data[:4] != b'PFD1':
        sys.exit(f'{path}: not a frame dump')
    at = 4
    while at + 37 <= len(data):
        tick, t_ns, real, w, h, n = struct.unpack_from('<QQQIII', data, at)
        xor = data[at + 36] == 1
        at += 37
        if at + n > len(data):
            break  # stopped mid-frame
        yield tick, t_ns, real, w, h, xor, data[at:at + n]
        at += n


def log(path):
    """events.log: start (real ns), end (ticks, real ns) and the event lines (tick, text)."""
    start = end = end_real = rate = None
    events = []
    for line in open(path) if os.path.exists(path) else []:
        w = line.split()
        if not w:
            continue
        if w[0] == 'start':
            start = int(w[1])
        elif w[0] == 'end':
            end = int(w[1])
            if 'real' in w:
                end_real = int(w[w.index('real') + 1])
        elif w[0].isdigit():
            events.append((int(w[0]), ' '.join(w[1:])))
            if len(w) > 2 and w[1] == 'audio':
                rate = float(w[2])
    return start, end, end_real, rate, events


class Track:
    """One frames.pfd: the picture at any tick (the latest record up to it), decoded as needed, in order."""

    def __init__(self, folder):
        self.folder = folder
        self.start, self.end, self.end_real, self.rate, self.events = log(os.path.join(folder, 'events.log'))
        self.recs = list(records(os.path.join(folder, 'frames.pfd')))
        if self.end is None:
            self.end = self.recs[-1][0] + 1 if self.recs else 0
        self.size = (self.recs[0][3], self.recs[0][4]) if self.recs else None
        self.i, self.px = -1, None

    def at(self, tick):
        # Before its first record a track shows that first picture (the frame drawn as the dump began).
        while self.i + 1 < len(self.recs) and (self.recs[self.i + 1][0] <= tick or self.i < 0):
            self.i += 1
            _, _, _, w, h, xor, z = self.recs[self.i]
            raw = zlib.decompress(z)
            if xor and self.px is not None:
                raw = (int.from_bytes(raw, 'little') ^ int.from_bytes(self.px[2], 'little')).to_bytes(len(raw), 'little')
            self.px = (w, h, raw)
        return self.px


def rgb(px, size):
    """0x00RRGGBB little endian (B G R 0) to rgb24 at `size`; a picture of another size (a window that became full
    screen after its first frames) in the middle, black around it."""
    w, h = size
    if px is None:
        return bytes(w * h * 3)
    pw, ph, raw = px
    src = bytearray(pw * ph * 3)
    src[0::3], src[1::3], src[2::3] = raw[2::4], raw[1::4], raw[0::4]
    if (pw, ph) == size:
        return bytes(src)
    out = bytearray(w * h * 3)
    cw, ch, ox, oy, sx, sy = min(w, pw), min(h, ph), max(0, (w - pw) // 2), max(0, (h - ph) // 2), max(0, (pw - w) // 2), max(0, (ph - h) // 2)
    for y in range(ch):
        a, b = ((oy + y) * w + ox) * 3, ((sy + y) * pw + sx) * 3
        out[a:a + cw * 3] = src[b:b + cw * 3]
    return bytes(out)


def runs(core, gap=0.1e9):
    """A game's pictures in runs without a pause: (first tick, ticks, real start, real end)."""
    out = []
    for tick, _, real, *_ in core.recs:
        if out and real - out[-1][3] <= gap:
            first, n, start, _ = out[-1]
            out[-1] = (first, n + 1, start, real)
        else:
            out.append((tick, 1, real, real))
    return out


def timeline(ui, cores):
    """(track, tick) per output frame, the events at their output frame, and each game run's (core, first tick,
    ticks, output frame)."""
    plan, events, games, k = [], [], [], 0
    parts = sorted((r[2], core, r) for core in cores for r in runs(core))
    ui_tick = lambda real: round((real - ui.start) / TICK)
    for _, core, (first, n, start, end) in parts:
        if ui.start is None:
            break
        until = max(k, min(ui.end, ui_tick(start)))
        events += [(len(plan) + t - k, e) for t, e in ui.events if k <= t < until]
        plan += [(ui, t) for t in range(k, until)]
        events += [(len(plan) + t - first, 'game ' + e) for t, e in core.events if first <= t < first + n]
        games.append((core, first, n, len(plan)))
        after = max(until, ui_tick(end) + 1)
        # What pocket-ui logged while the game was on screen: a press at the game's frame of that moment (both clocks
        # run in real time, so tick t is the run's frame t - until), a UI sound (the pause menu's on HOME) with the
        # UI's first picture after the game, the menu it stands for.
        events += [(len(plan) + (n if e.startswith('sound ') else min(n - 1, t - until)), e) for t, e in ui.events if until <= t < after]
        plan += [(core, t) for t in range(first, first + n)]
        k = after
    events += [(len(plan) + t - k, e) for t, e in ui.events if t >= k]
    plan += [(ui, t) for t in range(k, ui.end)]
    return plan, sorted(events), games


def lead(core):
    """Stereo frames of audio.s16 from before the game's first picture: pocket-core runs the game hidden behind
    pocket-ui's launch animation (the hold, `startup::stands_still`), silent on the speaker but with its sound in the
    dump, and draws nothing then. Everything after it is one tick's share per picture, so the lead is what is left
    over. Without skipping it the game's sound came 0.6-1.5 s after its pictures (Retroid Mini V2 takes, 10.11.2026)."""
    raw = os.path.join(core.folder, 'audio.s16')
    if not core.rate or not os.path.exists(raw):
        return 0
    return max(0, os.path.getsize(raw) // 4 - int(round(core.end * core.rate / 60)))


def game_sound(core, first, n, path):
    """The samples of ticks first .. first + n of a game (audio.s16 in tick order after the hold's `lead`, a tick's
    share by the rate)."""
    import wave
    raw = os.path.join(core.folder, 'audio.s16')
    if not core.rate or not os.path.exists(raw):
        return False
    per = core.rate / 60  # stereo frames per tick, at 60 pictures a second
    data = open(raw, 'rb').read()
    skip = lead(core)
    a, b = (skip + int(round(first * per))) * 4, (skip + int(round((first + n) * per))) * 4
    with wave.open(path, 'wb') as w:
        w.setnchannels(2)
        w.setsampwidth(2)
        w.setframerate(int(round(core.rate)))
        w.writeframes(data[a:b])
    return True


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('dir')
    out = p.add_mutually_exclusive_group(required=True)
    out.add_argument('--mp4')
    out.add_argument('--png')
    out.add_argument('--raw', action='store_true')
    out.add_argument('--sound', metavar='MP4', help="only <MP4>.events.txt and the game sound of an earlier --mp4 run (the pictures don't change)")
    p.add_argument('--from', dest='first', type=int, default=0, help='first output frame')
    p.add_argument('--to', dest='last', type=int, default=None, help='last output frame (included)')
    p.add_argument('--crf', default='14')
    a = p.parse_args()
    ui = Track(a.dir)
    cores = [Track(d) for d in sorted(glob.glob(os.path.join(a.dir, 'core-*'))) if os.path.exists(os.path.join(d, 'frames.pfd'))]
    if not ui.size:
        sys.exit('no frames')
    plan, events, games = timeline(ui, cores)
    last = len(plan) - 1 if a.last is None else min(a.last, len(plan) - 1)
    # The size most pictures have (the screen's).
    sizes = collections.Counter((r[3], r[4]) for t in [ui, *cores] for r in t.recs)
    ui.size = sizes.most_common(1)[0][0]
    w, h = ui.size
    sys.stderr.write(f'{len(plan)} frames at 60 fps ({len(plan) / 60:.2f} s), {w}x{h}, {len(cores)} game part(s); writing {a.first}-{last}\n')
    if a.sound:
        sound(a.sound, events, games, a.first, last)
        return
    if a.raw:
        sink, proc = sys.stdout.buffer, None
    else:
        target = a.mp4 or os.path.join(a.png, 'f%05d.png')
        if a.png:
            os.makedirs(a.png, exist_ok=True)
        enc = ['-c:v', 'libx264', '-preset', 'medium', '-crf', a.crf, '-pix_fmt', 'yuv420p'] if a.mp4 else ['-start_number', str(a.first)]
        proc = subprocess.Popen(['ffmpeg', '-v', 'error', '-y', '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-s', f'{w}x{h}', '-r', '60', '-i', '-', *enc, target], stdin=subprocess.PIPE)
        sink = proc.stdin
    cache = {}
    for n in range(a.first, last + 1):
        track, tick = plan[n]
        px = track.at(tick)
        key = (id(track), track.i)
        if key not in cache:
            cache = {key: rgb(px, ui.size)}
        sink.write(cache[key])
    if proc:
        sink.close()
        proc.wait()
        sound(a.mp4 or os.path.join(a.png, 'events'), events, games, a.first, last)


def sound(base, events, games, first_out, last):
    """<base>.events.txt (presses and UI sounds by second on the track) and each game run's <base>.game-*.wav."""
    with open(base + '.events.txt', 'w') as f:
        for n, e in events:
            if first_out <= n <= last:
                f.write(f'{(n - first_out) / 60:.4f} {n} {e}\n')
        for i, (core, first, n, at) in enumerate(games):
            wav = f'{base}.game-{os.path.basename(core.folder)}-{i}.wav'
            if game_sound(core, first, n, wav):
                f.write(f'{(at - first_out) / 60:.4f} {at} game sound {os.path.basename(wav)}\n')


if __name__ == '__main__':
    main()
