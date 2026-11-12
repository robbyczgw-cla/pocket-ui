#!/usr/bin/env python3
"""Exercise lane ordering/failures with bounded processes; no compilation/device access."""
import importlib.util
import os
import signal
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT=Path(__file__).resolve().parent.parent
spec=importlib.util.spec_from_file_location("gate_cpus", ROOT/"scripts/gate-cpus.py")
cpu=importlib.util.module_from_spec(spec)
spec.loader.exec_module(cpu)
siblings={n:{n%8,n%8+8} for n in range(16)}
checks=0

def check(ok, message):
    global checks
    if not ok: raise AssertionError(message)
    checks+=1

# Exhaustive caps in this small topology: no CPU or SMT sibling can cross lanes.
for mask in range(1,1<<16):
    allowed={n for n in range(16) if mask & (1<<n)}
    p=cpu.plan(allowed,siblings,True,True,True)
    lanes=[p[k] for k in ("normal","launcher","small")]
    assert set.union(*lanes)==allowed
    for i,a in enumerate(lanes):
        for b in lanes[i+1:]:
            assert not(a&b)
            assert all(not(siblings[n]&b) for n in a)
check(True,"every cap keeps whole SMT groups and exactly the allowed CPUs")
for exclusive,enabled,taskset in [(False,True,True),(True,False,True),(True,True,False)]:
    check(not cpu.plan(set(range(16)),siblings,exclusive,enabled,taskset)["parallel"],"shared/disabled/no-taskset fallback")
check(not cpu.plan(set(range(16)),{0:{0}},True,True,True)["parallel"],"unknown topology stays serial")
for value in ["", "1-0", "-1", "1,,2", "1;echo", "0-999999999"]:
    try: cpu.cpus(value)
    except ValueError: check(True,"invalid cap rejected")
    else: raise AssertionError(value)
try: cpu.plan(set(),siblings,True,True,True)
except ValueError: check(True,"empty intersection rejected")
else: raise AssertionError("empty cap accepted")
check(cpu.cpus("0-2,8")=={0,1,2,8},"taskset syntax parsed")
bad_env=dict(os.environ,GATE_CPUSET="not-a-list")
bad=subprocess.run([sys.executable,str(ROOT/"scripts/gate-cpus.py")],env=bad_env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
check(bad.returncode!=0 and "CPU allocation FAILED" in bad.stdout,"CLI rejects invalid cap before heavy work")

with tempfile.TemporaryDirectory(prefix="gate-schedule-") as tmp:
    root=Path(tmp); tree=root/"tree"; bins=root/"bin"; logs=root/"logs"; events=root/"events"
    for d in (tree/"scripts",bins,logs,events):d.mkdir(parents=True,exist_ok=True)
    def put(path,text):
        path.parent.mkdir(parents=True,exist_ok=True);path.write_text(text);path.chmod(0o755)
    for name in ("gate.sh","gate-shell.sh","check-compile.sh"):
        text=(ROOT/"scripts"/name).read_text().replace("/tmp/gate-",str(logs/"gate-"))
        if name=="gate.sh": text=text.replace("export PATH=$HOME/.cargo/bin:$HOME/opt/zig:$PATH",'export PATH="$FIXTURE_BIN:$PATH"')
        put(tree/"scripts"/name,text)
    put(tree/"scripts/gate-cpus.py",'''import os
parallel=os.environ.get("GATE_EXCLUSIVE")=="1" and os.environ.get("GATE_PARALLEL")!="0"
print("gate_allowed='0,1,2,3,4,5,6,7' gate_normal='0,1,2,3,4,5' gate_launcher='6' gate_small='7' gate_cores='6' gate_parallel='%d'"%parallel)
''')
    put(bins/"taskset",'''#!/bin/sh
if [ "$1" = -pc ]; then exit 0; fi
[ "$1" = -c ] || exit 2
export FIXTURE_CPUS=$2
shift 2
exec "$@"
''')
    # The first command of each lane rendezvous with Rust, proving real overlap.
    stage='''#!/usr/bin/env python3
import os,pathlib,time,sys
root=pathlib.Path(os.environ['FIXTURE_EVENTS']);kind=sys.argv[1]
if os.environ.get('GATE_EXCLUSIVE')=='1' and not(root/'build').exists():sys.exit('lane started before compilation')
(root/(kind+'.start')).write_text(os.environ.get('FIXTURE_CPUS','all'))
(root/(kind+'.pid')).write_text(str(os.getpid()))
if os.environ.get('FIXTURE_BLOCK')=='1':time.sleep(120)
if os.environ.get('FIXTURE_OVERLAP')=='1':
 until=time.monotonic()+5
 while not all((root/(s+'.start')).exists() for s in ['rust','launcher','small']):
  if time.monotonic()>until:sys.exit('lanes never overlapped')
  time.sleep(.005)
if kind=='small' and os.environ.get('FIXTURE_KILL')=='1':
 os.kill(os.getppid(),9)
 time.sleep(.02)
if kind=='small' and os.environ.get('GATE_EXCLUSIVE')=='1':time.sleep(.05)
(root/(kind+'.end')).touch()
if kind==os.environ.get('FIXTURE_FAIL'):sys.exit(1)
'''
    put(bins/"stage",stage)
    put(bins/"cargo",'''#!/bin/sh
case " $* " in
 *"check "*)
  stage=linux
  case " $* " in *" --features brick "*) stage=brick ;; esac
  touch "$FIXTURE_EVENTS/check-$stage"
  [ "$FIXTURE_FAIL" != "$stage" ] || exit 1
  echo "compile check"; exit 0 ;;
 *" --no-run "*) touch "$FIXTURE_EVENTS/build"; exit 0 ;;
 *" --features brick "*)
  if [ "$GATE_EXCLUSIVE" = 1 ]; then
   for lane in rust launcher small; do [ -f "$FIXTURE_EVENTS/$lane.end" ] || exit 1; done
  fi
  touch "$FIXTURE_EVENTS/brick"; echo 'test result: ok. 1 passed'; exit 0 ;;
 *" --doc "*) echo 'test result: ok. 1 passed'; exit 0 ;;
 *) stage rust || exit $? ;;
esac
echo 'test result: ok. 1 passed'
''')
    put(tree/"scripts/check-platforms.sh",'''#!/bin/sh
for part in "$@"; do
 touch "$FIXTURE_EVENTS/check-$part"
 [ "$FIXTURE_FAIL" != "$part" ] || exit 1
done
''')
    put(tree/"scripts/test-timing.sh",'''#!/bin/sh
for lane in rust launcher small; do
 if [ "$GATE_EXCLUSIVE" = 1 ]; then [ -f "$FIXTURE_EVENTS/$lane.end" ] || exit 1; fi
done
touch "$FIXTURE_EVENTS/timing"
echo 'test result: ok. 1 passed'
''')
    for suite in ['deploy/test-run-pocket-ui.sh','scripts/test-verify-device.sh','scripts/test-check-core-licences.sh','deploy/package/test-package.sh','deploy/test-grub-quiet.sh','scripts/image/test-grub-checks.sh','scripts/image/test-rocknix-source.sh','scripts/image/test-h700-pins.sh','scripts/image/test-storage-budget.sh','deploy/fat/test-hook.sh','deploy/image/test-mount-storage.sh']:
        kind='launcher' if 'test-run-pocket-ui' in suite else 'small' if 'verify-device' in suite else ''
        put(tree/suite, '#!/bin/sh\n'+(f'exec stage {kind}\n' if kind else 'exit 0\n'))
    for script in ['scripts/test-device-portability.py','scripts/test-device-transfer.py','scripts/image/test-receipts.py','scripts/android/test-release-check.py']: put(tree/script,'pass\n')
    (tree/"src").mkdir(); (tree/"tests").mkdir()
    (tree/"src/input_corpus_tests.rs").touch()
    (tree/"Cargo.toml").write_text("[features]\nbrick = []\n")
    (tree/"android").mkdir()
    (tree/"android/Cargo.toml").touch()
    for suite in ["brick_settings_walk","text_size","long_titles","search_titles","nav_explorer"]:(tree/f"tests/{suite}.rs").touch()
    (tree/".gate-changed").write_text("src/player.rs\n")
    env={k:v for k,v in os.environ.items() if not k.startswith(('GATE_','POCKETUI_SNAPSHOT_CACHE'))}
    env.update(FIXTURE_BIN=str(bins),FIXTURE_EVENTS=str(events),GATE_RUNNER='cargo',TMPDIR=str(root))
    def run(label,exclusive='1',overlap='1',fail='',kill='0'):
        for p in events.iterdir():p.unlink()
        current=dict(env,GATE_EXCLUSIVE=exclusive,FIXTURE_OVERLAP=overlap,FIXTURE_FAIL=fail,FIXTURE_KILL=kill)
        out=subprocess.run(['sh',str(tree/'scripts/gate.sh'),str(tree)],env=current,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=20)
        (logs/f'{label}.log').write_text(out.stdout)
        return out
    for part in ('linux','brick','android'):
        out=run('compile-red-'+part,fail=part)
        check(out.returncode==1 and 'compile-first FAILED' in out.stdout,out.stdout)
        check((events/('check-'+part)).exists(),'selected compiler ran')
        check(not(events/'build').exists(),'compile error stops before test codegen')
        check(not any((events/(lane+'.start')).exists() for lane in ('rust','launcher','small')),'compile error starts no lane')
    out=run('parallel')
    check(out.returncode==0,out.stdout)
    check((events/'build').exists(),"precompile before lanes")
    check((events/'timing').exists(),"timing follows complete lanes")
    check((events/'brick').exists(),"Brick builds only after every lane finishes")
    check([(events/f'{lane}.start').read_text() for lane in ('rust','launcher','small')]==['0,1,2,3,4,5','6','7'],"lane affinity")
    check('GATE: ok' in out.stdout,"parallel success")
    out=run('shared','0','0')
    check(out.returncode==0 and not(events/'build').exists(),out.stdout)
    check(all((events/f'{lane}.start').read_text()=='all' for lane in ('rust','launcher','small')),"serial fallback affinity")
    for lane, verdict in [('launcher','launcher tests FAILED'),('small','verify-device tests FAILED'),('rust','tests FAILED')]:
        out=run('red-'+lane,fail=lane)
        check(out.returncode==1 and verdict in out.stdout and 'GATE: ok' not in out.stdout,out.stdout)
    out=run('killed',kill='1')
    check(out.returncode==1 and 'shell test coordinator FAILED' in out.stdout and 'GATE: ok' not in out.stdout,out.stdout)
    check(not(events/'timing').exists(),"incomplete lane never permits timing")
    # Production aborts the inherited process group; no separate session/cgroup
    # may let a lane escape. This is only our own isolated fixture group.
    for p in events.iterdir():p.unlink()
    abort_env=dict(env,GATE_EXCLUSIVE='1',FIXTURE_OVERLAP='1',FIXTURE_BLOCK='1')
    with (logs/'abort.log').open('w') as stream:
        process=subprocess.Popen(['sh',str(tree/'scripts/gate.sh'),str(tree)],env=abort_env,stdout=stream,stderr=subprocess.STDOUT,start_new_session=True)
        try:
            import time
            until=time.monotonic()+5
            while not all((events/f'{lane}.pid').exists() for lane in ('rust','launcher','small')):
                if time.monotonic()>until:raise AssertionError('abort fixture never started every lane')
                time.sleep(.005)
            pids=[int((events/f'{lane}.pid').read_text()) for lane in ('rust','launcher','small')]
            os.killpg(process.pid,signal.SIGTERM)
            check(process.wait(timeout=5)!=0,'group abort cannot pass')
            def active():
                for pid in pids:
                    try: state=Path(f'/proc/{pid}/stat').read_text().split(') ')[1]
                    except FileNotFoundError: continue
                    if not state.startswith('Z'):return True
                return False
            until=time.monotonic()+2
            while active() and time.monotonic()<until:time.sleep(.005)
            check(not active(),'aborted lane survived')
            check(not list(root.glob('gate-rc.*')),'aborted gate removes fresh result/cache directory')
        finally:
            if process.poll() is None:
                os.killpg(process.pid,signal.SIGKILL)
                process.wait()
print(f'{checks} gate scheduling checks passed (including all 65535 nonempty CPU caps)')
