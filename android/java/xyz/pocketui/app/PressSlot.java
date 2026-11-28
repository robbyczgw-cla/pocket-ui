package xyz.pocketui.app;

import java.io.File;
import java.util.concurrent.atomic.AtomicReference;

/**
 * Android extras (docs/ANDROID-EXTRAS.md): the one pause-menu key press that may run at a time, and its token. A press whose
 * time ran out (a hung Shizuku call) can be taken over; the taker ends exactly that press (its token goes, now or as soon as
 * it makes one), never another's (Sol's recheck of 38cfee80f: a late clean-up could remove a newer press's token). No Android
 * types (`scripts/android/test/ExtrasKeysTest.java`).
 */
final class PressSlot {
    /** One press: until when it holds the slot (uptime ms), and its token once made. */
    static final class Job {
        final long until;
        private File token;
        private boolean dead;

        Job(long until) {
            this.until = until;
        }

        /** Taken over: its token goes, now or when it is made. */
        synchronized void kill() {
            dead = true;
            if (token != null && token.exists() && !token.delete()) token.deleteOnExit();
        }

        /** Its token; false (and the token gone) when it was taken over meanwhile. */
        synchronized boolean attach(File t) {
            token = t;
            if (dead) {
                t.delete();
                return false;
            }
            return true;
        }

        synchronized boolean dead() {
            return dead;
        }
    }

    private final AtomicReference<Job> current = new AtomicReference<>();

    /** The slot for a new press until `until`, or null while a press still holds it. An expired one is ended. */
    Job take(long now, long until) {
        Job prev = current.get();
        if (prev != null && prev.until > now) return null;
        Job mine = new Job(until);
        if (!current.compareAndSet(prev, mine)) return null;
        if (prev != null) prev.kill();
        return mine;
    }

    /** `job` is done: the slot is free again, unless another press took it over already. */
    void release(Job job) {
        current.compareAndSet(job, null);
    }

    boolean busy() {
        return current.get() != null;
    }
}
