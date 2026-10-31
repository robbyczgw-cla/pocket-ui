#!/usr/bin/env python3
"""Measure our square-wave GB diagnostic after the real core/resampler, separately from physical output."""
import json,sys,wave
from pathlib import Path
import numpy as np
p=Path(sys.argv[1]);meta=json.loads(p.with_suffix('.json').read_text());rate=meta['rate']
x=np.fromfile(p,dtype='<i2').reshape(-1,2).mean(axis=1).astype(float)
assert len(x)>rate*10,'Need at least 10 seconds of the diagnostic stream'
peak=np.max(np.abs(x));assert peak>100,'No audible-level signal in the client capture'
active=np.flatnonzero(np.abs(x)>peak*.02);a,b=int(active[0]),int(active[-1]+1)
# Keep start/stop fades outside the steady pitch window, as the existing Linux bench does.
y=x[a+rate*2:b-rate//2];assert len(y)>rate*5
cross=np.flatnonzero((y[:-1]<=0)&(y[1:]>0));cross=cross-y[cross]/(y[cross+1]-y[cross])
assert len(cross)>100,'No stable tone'
freq=rate*20/(cross[20:]-cross[:-20]);median=float(np.median(freq));cents=1200*np.log2(freq/median)
# A square-wave crossing is short. A near-zero run of >=1 ms inside the signal is a dropout candidate.
quiet=np.abs(y)<peak*.005;edges=np.diff(np.r_[False,quiet,False].astype(int));starts=np.flatnonzero(edges==1);ends=np.flatnonzero(edges==-1)
gaps=[{'start_s':float((s+a+rate*2)/rate),'duration_ms':float((e-s)*1000/rate)}for s,e in zip(starts,ends)if e-s>=rate//1000]
# An xrun already present before the first recorded block is startup evidence, not a later increment.
xruns=meta.get('xruns',[]);baseline=xruns[0][1]if xruns else 0;steady_xruns=max([n-baseline for _,n in xruns]+[0])
result={'capture':p.name,'scope':'client PCM after shared resampling; emulator AAudio counters, no physical speaker capture','seconds':len(x)/rate,'steady_seconds':len(y)/rate,'frequency_hz':median,'expected_hz':131072/(2048-1750),'pitch_rms_cents':float(np.sqrt(np.mean(cents*cents))),'pitch_max_cents':float(np.max(np.abs(cents))),'dropout_candidates':gaps,'aaudio_startup_xruns':baseline,'aaudio_later_xruns':steady_xruns,'aaudio_buffer_frames':meta['buffer_frames'],'aaudio_burst_frames':meta['frames_per_burst']}
result['pass']=not gaps and steady_xruns==0 and abs(median-result['expected_hz'])<1 and result['pitch_rms_cents']<1 and result['pitch_max_cents']<3
p.with_suffix('.analysis.json').write_text(json.dumps(result,indent=2)+'\n')
with wave.open(str(p.with_suffix('.wav')),'wb')as out:out.setnchannels(2);out.setsampwidth(2);out.setframerate(rate);out.writeframes(p.read_bytes())
print(json.dumps(result,indent=2));sys.exit(0 if result['pass']else 1)
