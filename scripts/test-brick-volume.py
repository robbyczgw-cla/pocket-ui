#!/usr/bin/env python3
"""Player regressions: useful key steps, mute restores exactly, no amp enable at live gain.
Runs the real card helpers against a stateful mixer; no audio or device access.
"""
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
HELPERS = ROOT / 'deploy/brick/hwroot/usr/bin'

with tempfile.TemporaryDirectory(prefix='brick-volume-') as tmp:
    d = Path(tmp)
    mixer = d / 'amixer'
    mixer.write_text('''#!/bin/sh
case " $* " in *" -s "*) while IFS= read -r line; do eval '"$0" -c 0 -q '"$line" || exit 1; done; exit ;; esac
case "$*" in
 *"name=digital volume"*) control=digital ;;
 *"name=DAC volume"*) control=dac ;;
 *"name=HpSpeaker Switch"*) control=speaker ;;
 *"name=Headphone Switch"*) control=headphone ;;
 *"name=BT Volume"*) control=bt ;;
 *) exit 1 ;;
esac
case "$*" in
 *cget*) echo "  : values=$(cat "$FAKE_MIXER/$control")" ;;
 *cset*) for v; do :; done
  echo "$control $v" >>"$FAKE_MIXER/trace"
  echo "$v" >"$FAKE_MIXER/$control" ;;
esac
''')
    mixer.chmod(0o755)
    for control, value in [('digital', '0'), ('dac', '160,160'), ('speaker', 'on'), ('headphone', 'on'), ('bt', '255')]:
        (d / control).write_text(value)
    env = dict(os.environ, BRICK_AMIXER=str(mixer), BRICK_STATE=str(d), FAKE_MIXER=str(d), BRICK_VOLUME_RAMP_DELAY='0', BRICK_VOLUME_COMPAT_CURVE='new')
    env.pop('BRICK_HW', None)
    def run(*args):
        return subprocess.check_output([str(HELPERS / args[0]), *map(str, args[1:])], env=env, text=True)
    def val(c):
        return (d / c).read_text().strip()
    def trace():
        return [line.split(' ', 1) for line in (d / 'trace').read_text().splitlines()]

    # A power loss can remove/truncate the format tag while leaving percentages
    # from any predecessor. Never interpret an old encoded level as a louder curve.
    for predecessor in ['old', 'new', 'low']:
        env['BRICK_VOLUME_COMPAT_CURVE'] = predecessor
        for damaged in [None, '', 'broken']:
            for f in d.glob('brick-volume*'): f.unlink()
            run('volume', 44)
            before_gain = int(val('digital')), int(val('bt'))
            tag = d / 'brick-volume-compat'
            if damaged is None: tag.unlink()
            else: tag.write_text(damaged)
            run('volume', 'restore')
            assert int(val('digital')) >= before_gain[0] and int(val('bt')) <= before_gain[1], ('lost format amplified a stored level', predecessor, damaged, before_gain, val('digital'), val('bt'))
    env['BRICK_VOLUME_COMPAT_CURVE'] = 'new'
    print('PASS: missing, empty or damaged format tags never amplify either output')

    run('volume', 100)
    levels = []
    for pct in range(100, 0, -5):
        run('volume', pct)
        raw = int(val('digital'))
        bt = int(val('bt'))
        levels.append((pct, raw, bt))
        assert f' {pct}% ' in run('pactl', 'get-sink-volume')
        assert val('dac') == '160,160', 'ordinary steps must not jump the DAC or amplifier paths'
    expected = [(100,0),(95,2),(90,4),(85,6),(80,8),(75,10),(70,12),(65,14),(60,16),(55,18),
                (50,20),(45,21),(40,22),(35,24),(30,25),(25,26),(20,27),(15,28),(10,30),(5,31)]
    print('LEVELS', levels, flush=True)
    assert [(p,r) for p,r,_ in levels] == expected, levels
    assert levels[0] == (100,0,255), 'the existing maximum must not increase'
    for (p,raw,bt), (_,next_raw,next_bt) in zip(levels,levels[1:]):
        assert 1 <= next_raw-raw <= 2, 'each key step must be audible and gentler at the bottom'
        assert bt > next_bt
    for pct in [1,5,10,20,30]:
        run('volume',pct)
        assert int(val('digital')) < (100-pct)*40//100, 'the requested lower levels must get louder'
    print('PASS: every key step rises; only the lower half is louder; 0 and the maximum stay fixed')

    # Non-key percentages (e.g. a saved 44%) retain their exact percentage and hardware endpoint.
    run('volume', 44)
    before = {c: val(c) for c in ['digital', 'dac', 'bt']}
    (d / 'trace').write_text('')
    run('volume', 'mute')
    for pct in [0, 100, 20, 70]:
        run('volume', pct)
    assert val('brick-volume-curve3') == '44', 'muted keys must not change the hidden restore level'
    assert 'Mute: yes' in run('pactl', 'get-sink-mute'), 'OSD must follow the actual slider state'
    assert ' 44% ' in run('pactl', 'get-sink-volume')
    assert val('speaker') == val('headphone') == 'off'
    assert val('dac') == '0,0' and val('bt') == '0'
    run('volume', 'unmute')
    assert {c: val(c) for c in before} == before, 'unmute must restore both routes exactly'
    assert 'Mute: no' in run('pactl', 'get-sink-mute')
    # Repeated slider position / settings poll must not retrigger the first-enable sequence.
    (d / 'trace').write_text((d / 'trace').read_text())
    n = len(trace())
    run('volume', 'unmute')
    assert len(trace()) == n
    print('PASS: muted keys do nothing, mute feedback agrees, exact level restored, repeated unmute has no writes')

    # Reconstruct the signal gain at every write, not just final mixer snapshots.
    digital, dac = int(before['digital']), 160
    switches = 0
    for c, v in trace():
        if c == 'digital':
            v = int(v)
            assert abs(v - digital) <= 1, ('hardware gain discontinuity', digital, v)
            digital = v
        elif c == 'dac':
            dac = int(v.split(',')[0])
            assert digital == 63, 'DAC changes only at maximum attenuation'
        elif c in ['speaker', 'headphone']:
            switches += 1
            assert dac == 0 and digital == 63, 'amplifier switched with a live signal'
    assert switches == 4
    print('PASS: complete mute/unmute trace: single raw ticks; every amplifier switch at DAC=0 and digital=63')

    # Zero as a real volume setting also silences both paths and recovers when raised.
    run('volume', 0)
    assert val('dac') == '0,0' and val('speaker') == val('headphone') == 'off'
    assert val('bt') == '0'
    run('volume', 5)
    assert val('digital') == '31' and val('dac') == '160,160'
    assert val('speaker') == val('headphone') == 'on'
    print('PASS: 0% is silent, next step enables quietly and ends at the lowest useful gain')

    # Cold start must inspect the slider before enabling any amp, including the first use.
    hw = d / 'hw'
    hw.write_text('#!/bin/sh\necho 1\n')
    hw.chmod(0o755)
    env['BRICK_HW'] = str(hw)
    env['BRICK_SLIDER_CFG'] = str(d / 'slider-cfg')
    (d / 'trace').write_text('')
    run('volume', 'restore')
    assert (d / 'brick-muted').exists()
    assert all(v != 'on' for c, v in trace() if c in ['speaker', 'headphone'])
    print('PASS: first restore with a muted physical slider never enables an amplifier')

    # Upgrade from either shipped curve: no surprise increase on the codec OR Bluetooth.
    env.pop('BRICK_HW')
    (d / 'brick-muted').unlink(missing_ok=True)
    import shutil
    def clear_state():
        for f in d.glob('brick-volume*'): f.unlink()
    def gain(p, curve):
        if p == 0: return 63,0
        if curve == 'old': return 63-(p*63+50)//100,p*255//100
        return ((100-p)*40+50)//100,255-((100-p)*232+50)//100
    for predecessor in ['old','new']:
        env['BRICK_VOLUME_COMPAT_CURVE'] = predecessor
        for old_pct in range(101):
            clear_state()
            (d / 'brick-volume').write_text(str(old_pct))
            if predecessor == 'new':
                # Both the empty marker shipped by brick.9 and valid exact markers must import.
                (d / 'brick-volume-curve2').write_text('' if old_pct%2 else str(old_pct))
            (d / 'brick-volume-compat').write_text(predecessor)
            old_raw,old_bt = gain(old_pct,predecessor)
            run('volume','restore')
            target,raw,bt = int(val('brick-volume-curve3')),int(val('digital')),int(val('bt'))
            assert raw >= old_raw and bt <= old_bt, (predecessor,old_pct,target,raw,bt)
            assert (old_pct == 0) == (target == 0) or old_raw >= 32, 'only levels below the new floor may mute'
            run('volume','restore')
            assert (target,raw,bt) == tuple(int(val(c)) for c in ['brick-volume-curve3','digital','bt']), 'no migration ratchet'
            # The actual rollback receives a safe percentage for its own curve.
            legacy=int(val('brick-volume'))
            rollback_raw,rollback_bt=gain(legacy,predecessor)
            assert rollback_raw >= raw and rollback_bt <= bt, (predecessor,old_pct,'rollback')
        print('PASS: all 101 '+predecessor+' stored levels import once without amplifying either output; rollback safe')

    clear_state()
    env['BRICK_VOLUME_COMPAT_CURVE']='new'
    run('volume',44)
    assert val('brick-volume-curve3') == '44' and val('brick-volume-curve2') != '44'
    # A curve-2 rollback changes its own remembered level, then the new helper imports it.
    (d / 'brick-volume').write_text('50')
    (d / 'brick-volume-curve2').write_text('')
    run('volume','restore')
    assert int(val('digital')) >= 20 and int(val('bt')) <= gain(50,'new')[1]
    before = tuple(val(c) for c in ['digital','bt'])
    env['BRICK_VOLUME_COMPAT_CURVE']='old'
    run('volume','restore')
    assert before == tuple(val(c) for c in ['digital','bt']), 'format changes must not alter the current level'

    # Detect the real predecessor package, not the running package's version.
    package=d/'card/app/hwroot/usr/bin'
    shutil.copytree(HELPERS,package)
    previous_profile=d/'card/app.previous/hwroot/usr/bin/brick-volume-profile'
    previous_profile.parent.mkdir(parents=True)
    previous_profile.write_text('')
    HELPERS=package
    env.pop('BRICK_VOLUME_COMPAT_CURVE')
    run('volume',44)
    assert val('brick-volume-compat') == 'new'
    previous_profile.write_text('VOLUME_CURVE=3\n')
    run('volume','restore')
    assert val('brick-volume-compat') == 'low' and val('brick-volume') == '44'
    assert val('brick-volume-curve3') == '44'
    previous_profile.unlink()
    run('volume','restore')
    assert val('brick-volume-compat') == 'old'
    # Interrupted format conversions fail silent even with an old exact target left behind.
    for curve in ['new','old','low']:
        (d/'brick-volume-compat').write_text(curve)
        (d/'brick-volume').write_text('0')
        (d/'brick-volume-curve3').write_text('44')
        run('volume','restore')
        assert val('digital') == '63' and val('dac') == '0,0'
    print('PASS: changed rollback values import safely, real package detection, interrupted format conversion stays silent')

    for damaged in ['', 'broken', '101']:
        (d/'brick-volume').write_text(damaged)
        (d/'brick-volume-curve3').write_text('10')
        run('volume','restore')
        assert val('digital') == '63' and val('dac') == '0,0', 'unknown stored gain must not turn into the 50% default'
    print('PASS: damaged or partially written stored levels start silent')
    clear_state()
    run('volume','restore')
    assert val('brick-volume-curve3') == '50' and val('digital') == '20', 'a fresh card keeps its existing default'

    # The speaker-only amp override must not freeze the shared level of other outputs.
    speaker_override = d / 'speaker-override'
    speaker_override.write_text('1')
    env['BRICK_SPEAKER'] = str(speaker_override)
    assert 'Mute: no' in run('pactl', 'get-sink-mute')
    run('volume', 50)
    run('volume', 55)
    assert val('brick-volume-curve3') == '55' and val('digital') == '18'
    run('volume', 'mute')
    assert 'Mute: yes' in run('pactl', 'get-sink-mute')
    run('volume', 100)
    assert val('brick-volume-curve3') == '55'
    print('PASS: speaker-only amp mute permits headphone/Bluetooth level changes; the slider still freezes all outputs')
