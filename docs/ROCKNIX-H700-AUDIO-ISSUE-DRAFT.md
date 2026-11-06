# Draft only: H700 UCM Speaker uses Line Out mixer while DAC remains at unity

Not submitted. Suggested title: **H700 / H616 UCM: Speaker volume and mute assigned to Line Out instead of DAC**

Environment: Anbernic RG35XX Plus, ROCKNIX 20261001 H700 image. Findings are from an extracted rootfs and previously
captured ALSA/PipeWire state, plus control checks and silent gameplay sessions on that image; this is not a claim about current upstream main.

In `/usr/share/alsa/ucm2/Allwinner/sun4i-h616/HiFi.conf`, Speaker's enable sequence sets `DAC Playback Volume` to 63
and `Speaker Switch` on, but `PlaybackMixerElem` is `Line Out`. Headphones also use Line Out, with `Headphone Jack`
for jack detection. The card's observed ID is `Codec`.

Captured after setting the system volume to zero: Line Out 0/off; Speaker on; DAC volume 63. DAC volume's TLV is
−73.08 dB plus 1.16 dB per raw step, so 63 is 0 dB and 0 is not a mute step. DAC Playback Switch is a separate stereo
control. A user heard sound despite a PipeWire mute/zero check. We have not recorded a waveform or reproduced that
exact audible moment causally. Overdrive is a plausible contributor to reported crackly loud game audio, not a
proven explanation for every audio glitch. Separate audio-clock/pacing problems have not been ruled out.

Expected: speaker volume changes should attenuate the actual speaker path; 0/mute should silence it. Headphones
should retain their separate Line Out attenuation and jack routing.

Candidate direction for investigation: assign Speaker's playback mixer to the DAC and ensure its real playback
switch participates in mute; keep Headphones on Line Out, with correct mutually exclusive Speaker routing. Please
validate the mixer names/switch association, gain curves and jack transitions in PipeWire/WirePlumber before applying
that change. We have not tested this proposed UCM edit. Speaker maximum calibration may need a separate policy;
our application's provisional DAC 44 (−22.04 dB) ceiling is the clean listening reference reported by its operator,
not an upstream hardware limit or a measured distortion threshold. No higher calibrated speaker limit has been established.

A focused device reproduction should record `amixer -c Codec contents` together with the active PipeWire sink,
volume and mute before/after changing volume, then compare the actual speaker level and repeat with headphones.
The existing capture supports the mapping mismatch; a controlled speaker-level measurement is still needed.

A separate controlled silent gameplay comparison also reproduced a digital scheduling problem: with the same
visually checked mGBA scene, 45 seconds of volume-key activity under the application's 720 MHz Battery cap had
56.60 fps and 591 new software queue underruns. Removing that cap and suppressing the application's in-game
volume display gave 60.00 fps and zero queue underruns. Both runs had advancing ALSA hardware pointers and digital
zeros at the source. This application-level scheduling issue does not prove analog overdrive, and must not be
attributed to UCM alone. The UCM mapping mismatch remains independently supported by the control data.
