package xyz.pocketui.app;

import java.io.File;

/**
 * Android extras (docs/ANDROID-EXTRAS.md): the note of the game pocket-ui started in another emulator app (`files/extras/playing`:
 * the package, then a launch id) and the lock that keeps the pause menu's key presses off while a new launch id is not
 * reliably written (Sol's recheck of 38cfee80f: a failed write left the old id valid, so a stale press could still match it).
 * Order of `publish`: the lock first, then the note, then the lock goes; a failure leaves the lock (presses refused until the
 * next start publishes); without a lock the old note goes first; if neither works, the lock goes into a second folder, and
 * if that fails too the game is not started (`Result.FAILED`). No Android types
 * (`scripts/android/test/ExtrasKeysTest.java`).
 */
final class LaunchNote {
    static final String NOTE = "playing", LOCK = "press-locked";

    /** Writes `text` to `f` whole or not at all (`Extras.write`); false when it could not. */
    interface Writer {
        boolean write(File f, String text);
    }

    /** What `publish` achieved. */
    enum Result {
        /** The new id is in force; presses may use it. */
        PUBLISHED,
        /** The new id is not reliably there, but a lock keeps every press off until the next start publishes. */
        LOCKED,
        /** Neither: the old id may still be read. The game must not start (Sol's recheck of 09f9f7f27). */
        FAILED
    }

    /**
     * The new launch `launch` of `pkg` in `dir`. `fallback` is a second, independent folder (our shared press folder) for the
     * lock when `dir` takes neither a lock nor the removal of the old note (a read-only folder).
     */
    static Result publish(File dir, File fallback, String pkg, String launch, Writer w) {
        File note = new File(dir, NOTE), lock = new File(dir, LOCK);
        String text = (pkg == null ? "" : pkg) + "\n" + launch + "\n";
        boolean dirOk = dir.isDirectory() || dir.mkdirs();
        if (dirOk && w.write(lock, launch + "\n")) {
            if (!w.write(note, text)) return Result.LOCKED;
            // Written: presses may go again (also the fallback lock of an earlier failure). A lock that can't go keeps them off.
            File other = fallback == null ? null : new File(fallback, LOCK);
            if (other != null && other.exists() && !other.delete()) return Result.LOCKED;
            return lock.delete() ? Result.PUBLISHED : Result.LOCKED;
        }
        // No lock here: the old note must go first, then the new one (or none: no menu over that game at all).
        if (dirOk && (!note.exists() || note.delete())) {
            return w.write(note, text) ? Result.PUBLISHED : Result.LOCKED;
        }
        // The old id stays readable here: the lock in the other folder, or nothing is safe.
        if (fallback != null && (fallback.isDirectory() || fallback.mkdirs()) && w.write(new File(fallback, LOCK), launch + "\n")) {
            return Result.LOCKED;
        }
        return Result.FAILED;
    }

    /** The launch id in force, "" when there is none. */
    static String launch(File dir) {
        String[] lines = read(new File(dir, NOTE)).trim().split("\n");
        return lines.length >= 2 ? lines[1].trim() : "";
    }

    /** Presses are off: a launch was not reliably published (the lock in `dir` or in `fallback`). */
    static boolean locked(File dir, File fallback) {
        return new File(dir, LOCK).exists() || (fallback != null && new File(fallback, LOCK).exists());
    }

    private static String read(File f) {
        try {
            return new String(java.nio.file.Files.readAllBytes(f.toPath()), java.nio.charset.StandardCharsets.UTF_8);
        } catch (java.io.IOException | RuntimeException e) {
            return "";
        }
    }
}
