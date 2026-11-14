package org.nvforever.android;

import java.io.IOException;

/** Atomic pairing context shared by the UI and its diagnostics worker.
 * A configuration change cancels the old upload and publishes OFF before persistence.
 * Only a successful persistence callback may publish the new enabled context. Readers
 * capture one immutable Snapshot, rather than mixing independently read credentials.
 * Neither snapshots nor errors render credentials into diagnostic text.
 */
public final class DiagnosticsSession {
    @FunctionalInterface
    public interface Persistence {
        void save(boolean enabled, String endpoint, String token) throws IOException;
    }
    public static final class Snapshot {
        public final boolean enabled;
        public final String endpoint, token;
        public final long generation;
        public final DiagnosticsTransport.Cancellation cancellation;
        private Snapshot(boolean enabled,String endpoint,String token,long generation) {
            this.enabled=enabled; this.endpoint=endpoint; this.token=token; this.generation=generation;
            cancellation=new DiagnosticsTransport.Cancellation();
            // An OFF snapshot cannot accidentally be used for a transport operation.
            if (!enabled) cancellation.cancel();
        }
    }
    private volatile Snapshot snapshot;

    public DiagnosticsSession(boolean enabled,String endpoint,String token) {
        if (endpoint==null || token==null) throw new IllegalArgumentException("Session values are required");
        snapshot=new Snapshot(enabled,endpoint,token,0);
    }
    /** Capture exactly once per worker operation. Final fields are safely published. */
    public Snapshot current() { return snapshot; }

    public synchronized void configure(boolean enabled,String endpoint,String token,
                                       Persistence persistence) throws IOException {
        Snapshot previous=snapshot;
        previous.cancellation.cancel();
        long generation=previous.generation+1;
        // Publish cancellation/OFF even when a persistence implementation fails.
        snapshot=new Snapshot(false,endpoint==null ? "" : endpoint,token==null ? "" : token,generation);
        if (endpoint==null || token==null || persistence==null)
            throw new IOException("Invalid session configuration");
        persistence.save(enabled,endpoint,token);
        if (enabled) snapshot=new Snapshot(true,endpoint,token,generation);
    }
}
