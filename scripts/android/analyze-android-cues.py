#!/usr/bin/env python3
"""Compare actual CartIn microphone delivery with insert-frame presentation logs; requires numpy."""
import csv,json,re,pathlib,sys,wave,statistics
import numpy as np
folder=pathlib.Path(sys.argv[1]);repo=pathlib.Path(__file__).resolve().parents[2]
text=(folder/'app-log.txt').read_text();log=(folder/'microphone.csv').read_text()
rows=[{k:int(v) for k,v in r.items()} for r in csv.DictReader(l for l in log.splitlines() if not l.startswith('#'))]
ends=np.array([r['frame'] for r in rows]);times=np.array([r['read_end_ns'] for r in rows],dtype=np.int64)
x=np.fromfile(folder/'microphone.s16le',dtype='<i2').astype(float)
with wave.open(str(pathlib.Path(sys.argv[2]) if len(sys.argv)>2 else repo/'assets/ui-sounds/cart-in.wav')) as w:ref_full=np.frombuffer(w.readframes(w.getnframes()),dtype='<i2').astype(float)
hit=int(np.argmax(abs(ref_full)));offset=max(0,hit-480);ref=ref_full[offset:hit+960].copy();ref-=ref.mean()
n=1<<(len(x)+len(ref)-1).bit_length();conv=np.fft.irfft(np.fft.rfft(x,n)*np.fft.rfft(ref[::-1],n),n)[len(ref)-1:len(x)]
energy=np.r_[0,np.cumsum(x*x)];energy=energy[len(ref):]-energy[:-len(ref)]
coef=conv/np.sqrt(np.maximum(energy,1)*sum(ref*ref))
starts=[float(t) for t in re.findall(r'insert: \S+ start cozy=true at ([\d.]+)',text)]
frames=[tuple(map(float,t)) for t in re.findall(r'insert-frame ([\d.]+) ([\d.]+)',text)]
def delivery_time(frame):
 j=min(np.searchsorted(ends,frame,side='right'),len(ends)-1)
 return (int(times[j])-(int(ends[j])-frame)*1e9/48000)/1e6
results=[]
for start in starts:
 local=[(at,elapsed) for at,elapsed in frames if start-100<=at<=start+2500 and abs(at-elapsed-start)<100]
 targets=[f for f in local if f[1]>=1200]
 if not targets:continue
 frame_time,elapsed=min(targets,key=lambda f:f[1])
 a=max(0,int(ends[min(np.searchsorted(times,int((start+200)*1e6)),len(ends)-1)])-192)
 b=min(len(coef),int(ends[min(np.searchsorted(times,int((start+1800)*1e6)),len(ends)-1)]))
 peak=int(a+np.argmax(abs(coef[a:b])));hit_frame=peak+hit-offset;at=delivery_time(hit_frame)
 null=np.r_[coef[a:max(a,peak-2400)],coef[min(b,peak+2400):b]]
 r={'launch_start_ms':start,'frame_present_ms':frame_time,'frame_elapsed_ms':elapsed,'mic_hit_delivery_ms':at,'mic_hit_minus_frame_present_ms':at-frame_time,'mic_hit_after_launch_ms':at-start,'correlation':float(coef[peak]),'peak_to_null_rms':float(abs(coef[peak])/np.sqrt(np.mean(null*null))),'microphone_peak_pcm':float(np.max(abs(x[peak:peak+len(ref)])))};results.append(r)
accepted=len(results)>=3 and all(r['peak_to_null_rms']>=6 and abs(r['correlation'])>=0.05 for r in results)
print(json.dumps({'status':'measured' if accepted else 'insufficient-signal','scope':'real cart-in waveform through built-in microphone, including input delivery latency, versus insert-frame buffer.present MONOTONIC; no AAudio output timestamp used','reference_hit_ms':hit/48,'results':results,'median_offset_ms':statistics.median([r['mic_hit_minus_frame_present_ms'] for r in results]) if accepted else None,'std_offset_ms':statistics.pstdev([r['mic_hit_minus_frame_present_ms'] for r in results]) if accepted else None},indent=2))
