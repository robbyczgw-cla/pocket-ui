#!/usr/bin/env python3
"""Run each production Java reader with bounded, pressure-aware supervision."""
import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
spec=importlib.util.spec_from_file_location('reader_watchdog',Path(__file__).with_name('reader_watchdog.py'))
watchdog=importlib.util.module_from_spec(spec);sys.modules[spec.name]=watchdog;spec.loader.exec_module(watchdog)
classes,corpus=sys.argv[1:]
base=['java','-Xmx128m','-Dfile.encoding=UTF-8','--add-opens=java.base/java.io=ALL-UNNAMED','-cp',classes,'xyz.pocketui.app.JavaReaderRule']
sources=subprocess.check_output(base+['--sources'],text=True,timeout=30).splitlines()
selected=os.environ.get('JAVA_READER_ONLY')
if selected:
    sources=[s for s in sources if s==selected]
    if not sources: raise SystemExit(f'No registered Java reader matched {selected!r}')
start=time.monotonic();count=0;failures=[];startup_max=reader_max=grace_max=0.0
for source in sources:
    directory=Path(corpus)/source
    for label in (directory/'labels').read_text().splitlines():
        with tempfile.TemporaryDirectory(prefix='java-reader-') as root:
            try:
                result=watchdog.run_probe(base+['--probe',source,str(directory/label),label],cwd=root)
            except watchdog.ProbeTimeout as error:
                raise SystemExit(f'{source}/{label}: {error}')
        startup_max=max(startup_max,result.startup_seconds);reader_max=max(reader_max,result.reader_seconds);grace_max=max(grace_max,result.pressure_grace)
        count+=1
        if result.returncode:
            failures.append(f'{source}/{label}:\n{result.stdout}')
            if len(failures)==5:break
    if len(failures)==5:break
print(f'Java reader faults: {count} probes in {time.monotonic()-start:.2f}s',flush=True)
print(f'Java probe maxima: startup={startup_max:.3f}s reader={reader_max:.3f}s pressure_grace={grace_max:.3f}s',flush=True)
if failures:raise SystemExit('\n'.join(failures))
