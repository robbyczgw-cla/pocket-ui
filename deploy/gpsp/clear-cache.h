/* Instruction-cache sync for gpSP/PicoDrive dynarecs on ARM64 (scripts/build-gpsp.sh passes it with -include).
 *
 * Why: zig 0.17's __clear_cache does not sync the cache for aarch64-linux-gnu in a shared object: a small JIT test (write two
 * instructions, __clear_cache, call) died with SIGILL on the first call, and gpSP's generated code did too. This is the usual
 * sequence from the ARM Architecture Reference Manual: data cache clean only when CTR_EL0.IDC is clear, instruction cache
 * invalidate only when CTR_EL0.DIC is clear, with barriers. The same test with this function ran 20000 times without a wrong result. */
#ifndef BRICK_CLEAR_CACHE_H
#define BRICK_CLEAR_CACHE_H
#ifndef __ASSEMBLER__   /* -include reaches the .S files too */
#include <stdint.h>
static inline void brick_clear_cache(void *start, void *end)
{
   uint64_t ctr, a, line;
   __asm__ volatile("mrs %0, ctr_el0" : "=r"(ctr));
   if (!((ctr >> 28) & 1)) {
      line = (uint64_t)4 << ((ctr >> 16) & 15);
      for (a = (uint64_t)start & ~(line - 1); a < (uint64_t)end; a += line)
         __asm__ volatile("dc cvau, %0" : : "r"(a) : "memory");
   }
   __asm__ volatile("dsb ish" : : : "memory");
   if (!((ctr >> 29) & 1)) {
      line = (uint64_t)4 << (ctr & 15);
      for (a = (uint64_t)start & ~(line - 1); a < (uint64_t)end; a += line)
         __asm__ volatile("ic ivau, %0" : : "r"(a) : "memory");
   }
   __asm__ volatile("dsb ish\n\tisb" : : : "memory");
}
#endif /* __ASSEMBLER__ */
#endif
