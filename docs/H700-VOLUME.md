# H700 speaker ceiling and mute

`h700_volume::Keeper` follows the system volume while pocket-ui is running, including pocket-core and standalone
games. It is enabled only on an H700, without `POCKETUI_MOCK`, with the affected H616 UCM Speaker section
(`PlaybackMixerElem "Line Out"`, DAC 63, Speaker on). A corrected UCM does not enable it. Nothing is installed or
rewritten in UCM. The card is addressed by its observed ALSA ID `Codec`, never by a guessed card number.

## Why

The ROCKNIX 20261001 H700 rootfs assigns Speaker's mixer to Line Out, while its enable sequence sets
`DAC Playback Volume` to 63 and `Speaker Switch` on. The captured controls show DAC raw 0..63,
−73.08 dB + raw × 1.16 dB; raw 0 is attenuation, **not** a hardware mute. Line Out is 0..31, with raw 0/1 mute and
2..31 from −43.5 dB in 1.5 dB steps. The backend checks this DAC TLV and the control ranges before writing.
`testdata/audio/h616-muted-amixer.txt` is a control-only capture used by the parser regression test.

ROCKNIX's `volume` helper (used by EmulationStation and the physical keys) changes the default PipeWire sink and
persists `audio.volume`; it contains no separate DAC volume write. RetroArch's shipped configuration documents
`audio_volume = 0.0` as unity gain in dB ([RetroArch reference](https://github.com/libretro/RetroArch/blob/master/retroarch.cfg)). Neither is evidence for a calibrated safe speaker maximum. The workaround
keeps these settings and the PCM/audio pacing path intact; it adds no digital boost.

## Gain, mute and routing

The single ceiling constant `h700_volume::SPEAKER_MAX_RAW` is provisionally **44 = −22.04 dB**. This is the
quiet listening reference reported by the device operator; no higher production ceiling is justified.
The measured −2.4 dBFS digital peak in Alien Hominid/mGBA does not establish the speaker's electrical/mechanical
clipping threshold. The device session was stopped at the operator’s request; no higher ceiling was tested.

Nonzero speaker wishes target that fixed DAC ceiling (or the downward-only diagnostic ceiling). PipeWire alone
applies the system volume through Line Out plus a software residual: `60*log10(percent/100)` dB relative to the
maximum, following [PulseAudio's cubic conversion](https://github.com/pulseaudio/pulseaudio/blob/master/src/pulse/volume.c).
Zero percent still mutes the hardware switches. The DAC is not another percentage control.

The RG35XX Plus's actual DAPM graph connects `Speaker <- LINEOUT <- Line Out Ramp Controller`. The
[H616 codec driver](https://code.googlesource.com/linux/torvalds/linux/+/de03b17ec046d81c4713bc54306579b9024fc513/sound/soc/sunxi/sun4i-codec.c)
routes that output through the Line Out controls. Earlier documentation and the fake ALSA layer incorrectly
assumed a bypass. In 0.4.0 the same percentage reduced both DAC and Line Out: at 50%, DAC 31 (-37.12 dB)
plus Line Out 19 (-18 dB) and software gain 1.0 gave -55.12 dB. With DAC 44 it gives -40.04 dB; 100% stays
-22.04 dB. These are read-back control gains in the serial signal path, not measured speaker SPL or output voltage.
PipeWire's ideal 50% target gives -40.10 dB; the 0.06 dB difference is hardware/software quantization.
A 10..100% control sweep confirmed the fixed-ceiling path on one RG35XX Plus. Two quiet half-second 1 kHz
signals at 50%, source peak -39.99 dBFS, exited successfully with the PCM hardware pointer advancing.
No high-level listening or analog recording was performed.

PipeWire retains control of Line Out: its attenuation and its playback switch. Both UCM devices name "Line Out" as PlaybackMixerElem, so that switch *is* the mute PipeWire shows for the
Speaker and the Headphones sink, and WirePlumber stores it per route, across restarts. pocket-ui never writes it.

At 0 percent or sink mute, Speaker Switch, DAC Playback Switch (both channels) and DAC volume are set to zero. All
independent mute mechanisms are attempted even if one control fails; successful writes are read back before being
owned. Read/write failures cannot prove physical silence.

A muted codec sink is never the player's choice on H700: pocket-ui offers no mute there (0 % is the silent level) and
ROCKNIX's `volume` helper never unmutes. Up to 0.3.1 the 0 % mute also switched Line Out off, which PipeWire showed as a
sink mute and WirePlumber stored; from then on the worker read "muted" and kept every switch off, so the speaker stayed
silent with "muted" in Quick settings, also after unplugging headphones and after a restart (issue #3). Now the worker
lifts such a mute (`pactl set-sink-mute <sink> 0`) when it attaches, on a jack or sink change and on any new wish.
At attach a lifted mute also switches the DAC and speaker back on (DAC ramped up from where it is): the zeros are what
the older version left. A wake repair does not lift a mute, since sleep mutes the sink itself for a game's last moment
before a power off. Silent lab runs therefore cannot rely on a sink mute on H700 (they never could: see the
validation below).

Only the known `5096000.codec` Speaker/Headphones sinks are treated as internal. `Headphone Jack` is read from
ALSA's CARD interface. Either a plugged jack or the Headphones sink keeps Speaker off. Headphones use DAC 63 and
PipeWire's existing Line Out attenuation; there is no speaker ceiling on the headphone path. Jack insertion turns
the speaker off before raising the DAC. Unplug reapplies the speaker ceiling, including after UCM resets DAC to 63.
No default sink is selected by pocket-ui: WirePlumber retains routing policy. Bluetooth, USB, HDMI and unknown sinks
are not assigned the internal volume; when leaving the internal route the codec's speaker and DAC are silenced.

## Ownership and lifetime

One worker starts after the first frame. It validates control ranges/TLV with one amixer capture, then opens a
persistent `libasound.so.2` control handle (`hw:Codec`). The six named controls are read/written directly; the DAC
ramp forks no subprocess and never opens/reopens a PCM stream. ABI signatures follow ALSA `control.h`.

DAC changes when restoring sound or changing routes move **one raw DAC step at most every 20 ms**.
Faster key notifications are coalesced; only the newest target is followed. Ordinary nonzero volume steps keep
the same DAC ceiling and leave the gain change to PipeWire; no mixer switch or route is toggled. Zero/mute bypasses the gain ramp and immediately writes the physical mute controls once
the wish is observed. An external ALSA edit cancels an in-progress ramp. Smooth steps reduce discontinuities but
are not proof of click-free analog output; that still needs the operator's listening A/B.

The default sink and its JSON volume/mute are polled with two bounded pactl helpers: every 500 ms when idle,
100 ms during/after key activity. Each helper has a 3-second timeout. H700 game keys wake this worker instead of
running an OSD polling loop. Sleep/wake repair is distinct from a deliberate volume request and cannot authorize
unmuting an existing hardware mute. This remains eventual hardware mute, not a real-time safety interlock. A silent device smoke observed physical mute within 188 ms (including command/readback overhead).
This is a measured upper bound for one request, not a worst-case deadline. Acoustic behavior remains unmeasured. No helper runs on UI/emulation
threads, and no helper is launched for individual ramp steps.

Unchanged volume/mute/route wishes cause no writes, even when someone uses `amixer`. An external edit of an owned
control yields ownership until a new deliberate volume/mute request. Routing may always turn the speaker off and
reduce an unsafe speaker gain, but never undo a user's hardware mute. Attaching normally never raises an existing DAC level
or unmutes existing switches. One exception is the first 0.4.1 start: with the internal speaker selected, no jack,
nonzero unmuted volume and all playback switches on, a DAC within one raw step of the old 0.4.0 curve is
recognized as inherited and ramped to the fixed ceiling. Other values and hardware mutes are left alone.
The check is stored in `<data>/h700-volume-041` only after it finishes; a later start cannot repeat it on a
manual edit. Diagnostic ceilings disable the migration. Exit never restores an inherited low value.
Headphones keep their existing attach behavior. After an explicit volume action, normal sound can resume. Our own switches can be
restored when returning from headphones or another sink, without undoing an external switch change.

Normal exit stops and joins the worker explicitly (`process::exit` does not run destructors). It rereads the final
sink and controls. Original values are restored only for successfully owned controls, with unchanged volume/mute,
unchanged sink/jack state, no pending failed update and no outside changes to the owned group. Restore never unmutes a final silent wish. If the
player changed volume, or the route/ownership changed, the last safe hardware setting stays: blindly restoring
full-scale DAC would undo volume-down. `SIGKILL`, a crash or power loss cannot run this cleanup. The next UCM device
enable can still reset DAC to 63; this scoped workaround is not an OS-wide repair after pocket-ui has exited.
A same-route UCM reset without a wake notification is indistinguishable from a user's ALSA edit and is deliberately
not fought by a watchdog. Fixing the UCM mapping upstream remains the durable solution.

## H700 game timing and volume display

H700 Battery no longer halves a running game's CPU limit. The own player uses its existing full-speed H700 game
policy; guests retain the governor/limit selected by their launcher. The original menu/idle governor is restored
on exit. Other device profiles retain their Battery cap. UI descriptions and translations state the H700 exception.

With a game running, H700 volume feedback is suppressed: no pocket-core volume texture, no mako volume window,
no hidden-shelf redraw for consumed level-key events or held-key polling. Repeat work still has timer deadlines;
opening/closing Quick settings, the pause menu and notices still draws. Shelf volume feedback is unchanged.
For a silent lab run, `POCKETUI_H700_SPEAKER_MAX_RAW=1` lowers the compiled ceiling for that process only.
It accepts only 1..SPEAKER_MAX_RAW and can never raise the production maximum. Nonzero wishes retain raw 1 at
this lab ceiling. This is not a mute: a separately verified digital-zero source and advancing PCM clock are
required before enabling the physical path. The diagnostic value is never saved as a player setting.

For an owner-operated A/B, set `POCKETUI_H700_VOLUME_OSD=1` before starting the UI to restore the old display path.
Compare the same core/scene/performance/volume sequence with the default; record core/frame time, FPS, late frames
and audio counters. This diagnostic override is not a player setting.

## Validation and release

`cargo test --profile gate --lib h700_volume` uses a fake ALSA layer with the device's serial LINEOUT speaker
path. Its fake mirrors Line Out's switch into the sink mute, as PipeWire does. A regression combines the
DAC gain, measured Line Out steps and PipeWire's software residual: 10..100% must apply the system curve
exactly once, with the same maximum and real mute at zero. It failed against the 0.4.0 DAC curve before the fix. It covers the issue #3 sequences (0 % and back on
headphones, then unplug; a stored mute at start, with and without the jack; a mute appearing on a route change),
volume keys/quick-settings wishes, correcting an old quiet DAC after a deliberate volume action, real hardware mute, independent mute failures, captured amixer parsing,
jack changes in both orders, UCM DAC resets, returning from Bluetooth, external edits, wake repair, failed/clamped
writes, ramp rate/coalescing/mute preemption and conditional exit restoration. Power and Quick settings regressions
cover the no-cap game policy and hidden-display path. The merge gate also checks existing controls, sleep and platform builds.

A controlled silent RG35XX Plus comparison used the same visually checked Alien Hominid gameplay seed per core,
45 seconds per window, public 0.2.2 versus the private candidate, mGBA/gpSP, Auto/Battery, with/without physical
volume-key events. All 16 windows had exact zero sink-monitor RMS and a continuously advancing ALSA hardware
pointer (45/45 sampled transitions). Stream gain was zero before enabling the speaker path; DAC was restricted
to raw 1 by a lab-only lower ceiling and an independent guard. This keeps a real PCM clock without audible audio.

With mGBA/Battery and volume keys, 0.2.2 ran at 56.60 fps, 153 late frames and 591 queue underruns at 720 MHz;
the candidate ran at 60.00 fps, zero late frames and zero queue underruns at 1416 MHz. All four candidate Battery
windows (both cores, keys on/off) held approximately 60 fps with zero queue underruns. No ALSA write/query xrun,
recovery or backend write failure occurred in these windows. PipeWire error observations were partial.

With candidate mGBA/Auto and the old OSD path restored by the diagnostic override, one additional 45-second window
had 59.96 fps, two late frames and two queue underruns versus 60.00 fps and zero with the default suppressed OSD.
This measures the combined old display/redraw/polling path; it does not isolate a GPU compositor cost.
An external Speaker-off edit stayed off for 0.7 seconds until a new explicit volume wish. A subsequent system
mute disabled the Speaker, both DAC switches and both Line Out switches, and set DAC to zero within 188 ms.
Settings/saves were restored by MD5; public 0.2.2 was restored, diagnostic environment removed and the device left muted.

These silent windows establish scheduling and control behavior, not acoustic click/distortion performance.
The sink monitor precedes the analog DAC; digital zeros cannot prove click-free analog steps. Physical headphone
insertion/removal, listening at the production ceiling, and sleep/wake remain hardware checks. The ceiling stays
44 (−22.04 dB), with no claim of a calibrated clipping threshold. No loud listening run is implied by a release.
