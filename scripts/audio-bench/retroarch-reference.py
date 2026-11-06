#!/usr/bin/env python3
"""Measure ROCKNIX's pinned RetroArch sinc in the same signals as resampler_bench.

Usage: retroarch-reference.py SIGNAL_DIR [--out-rate 48000]
Downloads the upstream MIT-licensed implementation plus its headers to SIGNAL_DIR/retroarch-reference;
keeps upstream copyright notices. Compiles a host-only shared library with cc. No code is vendored into
pocket-ui. SIMD mask 0 matches ROCKNIX's aarch64 --disable-neon build. Quality 2 is LOWER, 3 is NORMAL.
For 44100 use a separate signal directory; analyze with resampler.py --out-rate 44100.
"""
import argparse
import ctypes as C
import math
from pathlib import Path
import subprocess
import time
import urllib.request
import numpy as np

PIN = 'bdba046fa6766380bc2457532f38e589df769aaf'
FILES = ['audio/resampler/drivers/sinc_resampler.c', 'include/audio/audio_resampler.h']
FILES += ['include/' + name + '.h' for name in
          ['retro_environment', 'retro_inline', 'filters', 'retro_math', 'memalign', 'boolean', 'retro_common_api']]
WRAPPER = r'''
#include <stdlib.h>
#include "libretro-common/audio/resampler/drivers/sinc_resampler.c"
void *memalign_alloc(size_t align, size_t size) { void *p=0; if(posix_memalign(&p,align,size)) return 0; return p; }
void memalign_free(void *p) { free(p); }
void *ra_new(double rate, double output_rate, int quality) { return sinc_resampler.init(0, fmin(1.0, output_rate/rate), quality, 0); }
size_t ra_process(void *p, const float *input, size_t frames, float *output, double ratio) {
 struct resampler_data d={input,output,frames,0,ratio}; sinc_resampler.process(p,&d); return d.output_frames;
}
void ra_free(void *p) { sinc_resampler.free(p); }
'''
COST = r'''
#include <stdio.h>
#include <time.h>
#include "wrapper.c"
static double now(void) { struct timespec t; clock_gettime(CLOCK_MONOTONIC,&t); return t.tv_sec+t.tv_nsec/1e9; }
int main(int argc,char **argv) {
 double rates[]={22050,32040,32768,44100,48000,65536}; double out_rate=argc>1?atof(argv[1]):48000;
 for(int q=2;q<=3;q++) for(int k=0;k<6;k++) {
  double rate=rates[k]; size_t n=lround(rate/59.7275); float in[7000],out[12000];
  for(size_t i=0;i<n;i++){in[2*i]=16000*sin(i*2*M_PI*1000/rate);in[2*i+1]=-in[2*i];}
  double t=now();void *p=ra_new(rate,out_rate,q);double prepare=(now()-t)*1000;
  if(!p) return 1;
  for(int i=0;i<10;i++) ra_process(p,in,n,out,out_rate/rate);
  t=now();size_t count=0;
  for(int i=0;i<1200;i++) count+=ra_process(p,in,n,out,out_rate/rate/(1+0.005*sin(i*0.03)));
  printf("quality=%d rate=%.0f out_rate=%.0f frames=1200 mean_ms=%.6f prepare_ms=%.6f output=%zu\n",q,rate,out_rate,(now()-t)*1000/1200,prepare,count);
  ra_free(p);
 }
 return 0;
}
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('signals', type=Path)
    parser.add_argument('--out-rate', type=int, default=48000)
    args = parser.parse_args()
    root = args.signals / 'retroarch-reference'
    root.mkdir(exist_ok=True)
    for filename in FILES:
        path = root / 'libretro-common' / filename
        path.parent.mkdir(parents=True, exist_ok=True)
        if not path.exists():
            path.write_bytes(urllib.request.urlopen(
                f'https://raw.githubusercontent.com/libretro/RetroArch/{PIN}/libretro-common/{filename}', timeout=30).read())
    (root / 'wrapper.c').write_text(WRAPPER)
    (root / 'cost.c').write_text(COST)
    libpath = (root / 'reference.so').resolve()
    subprocess.run(['cc', '-O3', '-fPIC', '-shared', '-I', str(root / 'libretro-common/include'),
                    str(root / 'wrapper.c'), '-lm', '-o', str(libpath)], check=True)
    lib = C.CDLL(str(libpath))
    ptr = C.POINTER(C.c_float)
    lib.ra_new.argtypes, lib.ra_new.restype = [C.c_double, C.c_double, C.c_int], C.c_void_p
    lib.ra_process.argtypes = [C.c_void_p, ptr, C.c_size_t, ptr, C.c_double]
    lib.ra_process.restype = C.c_size_t
    lib.ra_free.argtypes = [C.c_void_p]
    metadata = []
    for file in sorted(args.signals.glob('in_*.s16')):
        _, rate, case = file.stem.split('_', 2)
        rate = int(rate)
        input_mono = np.fromfile(file, dtype='<i2').astype(np.float32)
        stereo = np.repeat(input_mono, 2)
        scratch = np.empty(20000, dtype=np.float32)
        for quality, mode in [(2, 'ra-lower'), (3, 'ra-normal')]:
            for suffix, skew in [('', 0), ('_s', 0.004)]:
                resampler = lib.ra_new(rate, args.out_rate, quality)
                if not resampler:
                    raise RuntimeError('RetroArch init failed')
                parts, lag, observations, delivered, emitted = [], 0.0, 0, 0, 0
                for offset in range(0, len(stereo), 1092):
                    chunk = stereo[offset:offset + 1092]
                    n = lib.ra_process(resampler, chunk.ctypes.data_as(ptr), len(chunk) // 2,
                                       scratch.ctypes.data_as(ptr), args.out_rate / rate / (1 + skew))
                    parts.append(np.rint(scratch[:n * 2:2]).clip(-32768, 32767).astype('<i2'))
                    delivered += len(chunk) // 2
                    emitted += n
                    if delivered > len(stereo) // 4:
                        lag += delivered / rate - emitted / args.out_rate * (1 + skew)
                        observations += 1
                lib.ra_free(resampler)
                np.concatenate(parts).tofile(args.signals / f'{mode}_{rate}_{case}{suffix}.s16')
                metadata.append(f'{mode} {rate} {case}{suffix} lag_ms {lag / max(observations, 1) * 1000:.4f}\n')
    (args.signals / 'retroarch-meta.txt').write_text(''.join(metadata))
    print(f'RetroArch {PIN}; output {args.out_rate} Hz; LOWER=2, NORMAL=3; SIMD mask 0')


if __name__ == '__main__':
    main()
