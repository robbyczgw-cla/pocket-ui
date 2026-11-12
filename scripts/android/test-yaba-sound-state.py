#!/usr/bin/env python3
"""Round-trip the pinned core's actual Musashi state methods across a fresh host context.

Usage: python3 scripts/android/test-yaba-sound-state.py <patched core source>
The context type is read from Musashi itself. Host callback/table pointers must stay
those of the receiving process; only sound-CPU values may cross a saved state.
"""
import pathlib,re,subprocess,sys,tempfile
root=pathlib.Path(sys.argv[1])/'yabause/src'
source=(root/'m68kmusashi.c').read_text();header=(root/'musashi/m68kcpu.h').read_text()
def function(name):
    start=source.rfind('\n',0,source.index(name+'('))+1
    brace=source.index('{',start);end=brace+1;depth=1
    while depth:
        depth+=(source[end]=='{')-(source[end]=='}');end+=1
    return source[start:end]
context=re.search(r'typedef struct\s*\{(?:(?!typedef struct).)*?\}\s*m68ki_cpu_core;',header,re.S).group()
code=r'''
#include <stdint.h>
#define MUSASHI_STATE_BYTES 240
#include <stddef.h>
#include <stdio.h>
#include <string.h>
#include <assert.h>
typedef unsigned int uint; typedef uint8_t uint8;
typedef struct {int done,size;} IOCheck_struct;
#define ywrite(c,p,n,k,f) assert(fwrite(p,n,k,f)==(k))
#define yread(c,p,n,k,f) assert(fread(p,n,k,f)==(k))
'''+context+r'''
static m68ki_cpu_core cpu;
static unsigned int m68k_get_context(void *p) {memcpy(p,&cpu,sizeof cpu);return sizeof cpu;}
static void m68k_set_context(void *p) {memcpy(&cpu,p,sizeof cpu);}
static int old_callback(int n) {return n+1;}
static int new_callback(int n) {return n+2;}
'''+function('M68KMusashiSaveState')+'\n'+function('M68KMusashiLoadState')+r'''
int main(void) {
    FILE *fp=tmpfile();assert(fp);
    cpu.pc=0x123456;cpu.dar[0]=0x2001;cpu.dar[15]=0x7ff00;
    cpu.sp[0]=0x10000;cpu.sp[4]=0x7ff00;cpu.int_level=0x500;
    cpu.stopped=1;cpu.pref_addr=0x123456;cpu.pref_data=0x4e72;
    cpu.int_ack_callback=old_callback;cpu.cyc_instruction=(void*)0x1234;
    M68KMusashiSaveState(fp);
    assert(ftell(fp)>64 && "the new process needs actual sound CPU state, not an empty file");
    memset(&cpu,0,sizeof cpu);cpu.int_ack_callback=new_callback;cpu.cyc_instruction=(void*)0x5678;
    rewind(fp);M68KMusashiLoadState(fp);
    assert(cpu.pc==0x123456 && cpu.dar[0]==0x2001 && cpu.dar[15]==0x7ff00);
    assert(cpu.sp[0]==0x10000 && cpu.sp[4]==0x7ff00);
    assert(cpu.int_level==0x500 && cpu.stopped==1);
    assert(cpu.pref_addr==0x123456 && cpu.pref_data==0x4e72);
    assert(cpu.int_ack_callback==new_callback && cpu.cyc_instruction==(void*)0x5678);
    fclose(fp);puts("Yaba: sound CPU registers/IRQ/STOP/prefetch survive; receiving host callbacks stay intact");
}
'''
with tempfile.TemporaryDirectory(prefix='yaba-sound-test-') as tmp:
    p=pathlib.Path(tmp);(p/'test.c').write_text(code)
    subprocess.run(['cc','-g','-fsanitize=address,undefined','-o',str(p/'test'),str(p/'test.c')],check=True)
    subprocess.run([str(p/'test')],check=True,timeout=10)
