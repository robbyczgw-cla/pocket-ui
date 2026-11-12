#!/usr/bin/env python3
"""Known-click A/B correlation; requires numpy. Absolute numbers include input delivery latency."""
import csv,re,json,pathlib,statistics,sys
import numpy as np
if len(sys.argv)!=3: sys.exit('usage: analyze-aaudio-acoustic.py LOG.csv MICROPHONE.s16le')
text=pathlib.Path(sys.argv[1]).read_text()
rows=[{k:int(v) for k,v in r.items()} for r in csv.DictReader(l for l in text.splitlines() if not l.startswith('#'))]
events=[tuple(map(int,x)) for x in re.findall(r'# click=(\d+) block=(\d+) generated_ns=(\d+) output_frame=(\d+)',text)]
x=np.fromfile(sys.argv[2],dtype='<i2').astype(float)
if not rows or not events or len(x) < 480: sys.exit('no complete microphone recording/click metadata')
amplitude_match=re.search(r'amplitude_peak=(\d+)',text)
amplitude=int(amplitude_match.group(1)) if amplitude_match else 1024
ref=[];r=0x13579bdf
for i in range(480):
 r^=(r<<13)&0xffffffff;r^=r>>17;r^=(r<<5)&0xffffffff;r&=0xffffffff
 ref.append(int((amplitude if r&1 else -amplitude)*np.sin(np.pi*i/479)))
ref=np.array(ref,dtype=float);ref-=ref.mean()
# FFT matched filter: exact known emitted signal against microphone PCM; no listening judgement.
n=1<<(len(x)+len(ref)-1).bit_length()
c=np.fft.irfft(np.fft.rfft(x,n)*np.fft.rfft(ref[::-1],n),n)[len(ref)-1:len(x)]
energy=np.r_[0,np.cumsum(x*x)];energy=energy[len(ref):]-energy[:-len(ref)]
coef=c/np.sqrt(np.maximum(energy,1)*sum(ref*ref))
ends=np.array([r['frame'] for r in rows]);times=np.array([r['read_end_ns'] for r in rows],dtype=np.int64)
def delivery_time(frame):
 i=min(np.searchsorted(ends,frame,side='right'),len(ends)-1)
 return int(times[i])-(int(ends[i])-frame)*1e9/48000
results=[]
for k,block,generated,frame in events:
 a=max(0,int(ends[min(np.searchsorted(times,generated-100_000_000),len(ends)-1)])-192)
 b=min(len(coef),int(ends[min(np.searchsorted(times,generated+900_000_000),len(ends)-1)]))
 if b <= a: sys.exit('recording does not cover every click')
 peak=a+np.argmax(abs(coef[a:b]));at=delivery_time(peak)
 # Null distribution outside the best peak's +/-20 ms within that click's search window.
 null=np.r_[coef[a:max(a,peak-960)],coef[min(b,peak+960):b]]
 result={'click':k,'block':block,'path':'fast' if block%2 else 'raw','generated_ns':generated,'mic_frame':int(peak),'delivery_delay_ms':(at-generated)/1e6,'correlation':float(coef[peak]),'null_rms':float(np.sqrt(np.mean(null*null))),'peak_input_pcm':float(np.max(abs(x[peak:peak+480])))}
 result['peak_to_null_rms']=abs(result['correlation'])/max(result['null_rms'],1e-12)
 result['detected']=bool(result['peak_to_null_rms']>=6 and 0<=result['delivery_delay_ms']<=900)
 results.append(result)
summary={}
for path in ['raw','fast']:
 attempts=[r for r in results if r['path']==path]
 detected=[r for r in attempts if r['detected']]
 vals=[r['delivery_delay_ms'] for r in detected]
 summary[path]={'n':len(vals),'attempted_n':len(attempts)}
 if vals:
  summary[path].update({'median_ms':statistics.median(vals),'mean_ms':statistics.mean(vals),'std_ms':statistics.pstdev(vals),'min_ms':min(vals),'max_ms':max(vals),'correlation_abs_min':min(abs(r['correlation']) for r in detected)})
summary['status']='valid' if min(summary['raw']['n'],summary['fast']['n'])>=10 else 'insufficient-signal'
summary['raw_minus_fast_median_ms']=(summary['raw']['median_ms']-summary['fast']['median_ms']) if summary['status']=='valid' else None
summary['scope']='callback click generation to microphone read delivery; includes input latency; relative A/B output comparison with same continuous input stream'
summary['matched_filter_peak_to_null_rms_min']=min(abs(r['correlation'])/max(r['null_rms'],1e-12) for r in results)
summary['confidence']='Require at least ten detected clicks per path; each peak must exceed six times null RMS and have a nonnegative delay. Review individual clicks too.'
print(json.dumps({'summary':summary,'clicks':results},indent=2))
