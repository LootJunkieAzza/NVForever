package org.nvforever.android;

import java.io.IOException;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicReference;

/** Pure host concurrency tests. No sockets, Android APIs or production credentials. */
public final class DiagnosticsSessionTest {
    private static int checks;
    private static void check(boolean condition,String message) {
        checks++; if (!condition) throw new AssertionError(message);
    }
    private static final String A="http://192.168.1.2:8765/api/v1/log-bundles";
    private static final String B="http://10.0.0.2:8765/api/v1/log-bundles";
    private static final String TA="test-only-old-pair", TB="test-only-new-pair";
    private static void await(CountDownLatch latch) throws IOException {
        try { if (!latch.await(2,TimeUnit.SECONDS)) throw new IOException("Test barrier timed out"); }
        catch (InterruptedException e) { Thread.currentThread().interrupt(); throw new IOException("Test interrupted"); }
    }
    private static void defaultOffAndSuccessfulChange() throws Exception {
        DiagnosticsSession session=new DiagnosticsSession(false,A,TA);
        DiagnosticsSession.Snapshot initial=session.current();
        check(!initial.enabled && initial.generation==0,"initial default is OFF");
        check(initial.cancellation.isCancelled(),"OFF context cannot upload");
        session.configure(true,B,TB,(enabled,endpoint,token)->{
            DiagnosticsSession.Snapshot pending=session.current();
            check(!pending.enabled&&pending.cancellation.isCancelled(),"OFF is published before persistence");
            check(initial.cancellation.isCancelled(),"previous context cancelled before persistence");
            check(enabled&&endpoint.equals(B)&&token.equals(TB),"callback receives coherent requested pair");
        });
        DiagnosticsSession.Snapshot configured=session.current();
        check(configured.enabled&&configured.endpoint.equals(B)&&configured.token.equals(TB),"new context published after success");
        check(configured.generation==1&&!configured.cancellation.isCancelled(),"new context has a fresh active cancellation");
        check(initial.endpoint.equals(A)&&initial.token.equals(TA)&&!initial.enabled,"captured initial snapshot never mutates");
        session.configure(false,B,TB,(enabled,endpoint,token)->check(!enabled,"disabled request persisted"));
        check(!session.current().enabled&&session.current().generation==2,"disable changes generation and stays OFF");
        check(configured.cancellation.isCancelled(),"disable cancels previous upload context");
    }
    private static void readerDuringPausedPersistence() throws Exception {
        DiagnosticsSession session=new DiagnosticsSession(true,A,TA);
        DiagnosticsSession.Snapshot old=session.current();
        CountDownLatch inside=new CountDownLatch(1),release=new CountDownLatch(1);
        AtomicReference<Throwable> failure=new AtomicReference<>();
        Thread writer=new Thread(()->{
            try { session.configure(true,B,TB,(enabled,endpoint,token)->{inside.countDown();await(release);}); }
            catch(Throwable e){failure.set(e);}
        },"diagnostics-session-test-writer");
        writer.start();
        try {
            await(inside);
            check(old.cancellation.isCancelled(),"active prior context cancelled before blocked storage");
            DiagnosticsSession.Snapshot pending=session.current();
            for(int i=0;i<10000;i++) {
                DiagnosticsSession.Snapshot s=session.current();
                check(s==pending&&!s.enabled&&s.endpoint.equals(B)&&s.token.equals(TB),"concurrent reader sees only complete disabled new pair");
            }
            check(pending.generation==old.generation+1&&pending.cancellation.isCancelled(),"pending context cannot transfer while storage paused");
        } finally {release.countDown();writer.join(2000);}
        check(!writer.isAlive()&&failure.get()==null,"writer completed successfully");
        DiagnosticsSession.Snapshot current=session.current();
        check(current.enabled&&current.endpoint.equals(B)&&current.token.equals(TB),"publish enabled only after persistence released");
        check(current.cancellation!=old.cancellation&&!current.cancellation.isCancelled(),"new active context is independent of cancelled predecessor");
    }
    private static void failedPersistenceStaysOff() throws Exception {
        for(boolean requested:new boolean[]{false,true}) {
            DiagnosticsSession session=new DiagnosticsSession(true,A,TA);
            DiagnosticsSession.Snapshot old=session.current();
            boolean failed=false;
            try {session.configure(requested,B,TB,(enabled,endpoint,token)->{throw new IOException("Injected persistence failure");});}
            catch(IOException expected){failed=true;}
            check(failed,"persistence failure reaches caller");
            DiagnosticsSession.Snapshot s=session.current();
            check(!s.enabled&&s.cancellation.isCancelled(),"failure remains OFF for disable and enable requests");
            check(old.cancellation.isCancelled()&&s.generation==1,"failure never restores the old active context");
            session.configure(true,A,TA,(enabled,endpoint,token)->{});
            check(session.current().enabled&&session.current().generation==2,"explicit successful retry may enable a new context");
        }
        DiagnosticsSession session=new DiagnosticsSession(true,A,TA);
        try {session.configure(true,B,TB,(enabled,endpoint,token)->{throw new IllegalStateException("Injected runtime failure");});}
        catch(IllegalStateException expected){ }
        check(!session.current().enabled&&session.current().cancellation.isCancelled(),"unchecked persistence failure also remains OFF");
    }
    private static void readersNeverMixRepeatedPairChanges() throws Exception {
        DiagnosticsSession session=new DiagnosticsSession(false,A,TA);
        AtomicBoolean done=new AtomicBoolean();AtomicReference<Throwable> failure=new AtomicReference<>();
        CountDownLatch started=new CountDownLatch(1);
        Thread reader=new Thread(()->{
            try {
                started.countDown();
                while(!done.get()) {
                    DiagnosticsSession.Snapshot s=session.current();
                    boolean pair=s.endpoint.equals(A)&&s.token.equals(TA)||s.endpoint.equals(B)&&s.token.equals(TB);
                    if(!pair)throw new AssertionError("Mixed endpoint/token snapshot");
                    if(!s.enabled&&!s.cancellation.isCancelled())throw new AssertionError("Disabled context not cancelled");
                }
            }catch(Throwable e){failure.set(e);}
        },"diagnostics-session-test-reader");
        reader.start();await(started);
        try {for(int i=0;i<1000;i++){final boolean pair=(i&1)==0;session.configure(true,pair?B:A,pair?TB:TA,(enabled,endpoint,token)->Thread.yield());}}
        finally {done.set(true);reader.join(2000);}
        check(!reader.isAlive()&&failure.get()==null,"atomic readers survive repeated concurrent pair changes");
        check(session.current().generation==1000,"one generation per serialized change");
    }
    public static void main(String[] args) throws Exception {
        defaultOffAndSuccessfulChange();readerDuringPausedPersistence();failedPersistenceStaysOff();readersNeverMixRepeatedPairChanges();
        System.out.println("DiagnosticsSessionTest: PASS ("+checks+" checks; no network)");
    }
}
