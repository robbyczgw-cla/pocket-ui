/* rcheevos' rc_export.h for the TrimUI Brick's pocket-core (build.rs, forced in with -include; the same include guard, so the vendor
   header is skipped). The Brick's pocket-core exports its symbols (--export-dynamic, for its eglGetProcAddress), and the vendor header
   gives every public rcheevos function default visibility: PPSSPP exports its own rc_* and would bind to ours. Here RC_EXPORT is empty,
   and with -fvisibility=hidden all of rcheevos stays inside pocket-core. Only for aarch64 Linux C (no Windows, no i386, no C++). */
#ifndef RC_EXPORT_H
#define RC_EXPORT_H
#define RC_BEGIN_C_DECLS
#define RC_END_C_DECLS
#define RC_CCONV
#define RC_EXPORT
#endif
