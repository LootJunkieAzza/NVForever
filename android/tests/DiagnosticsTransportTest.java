package org.nvforever.android;

import com.sun.net.httpserver.*;
import java.io.*;
import java.net.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.security.MessageDigest;
import java.util.*;
import java.util.concurrent.*;
import java.util.concurrent.atomic.*;
import java.util.zip.*;

/** Pure host checks. All network listeners/clients stay on injected 127.0.0.1.
 * This test-only token is never a production pairing secret.
 */
public final class DiagnosticsTransportTest {
    private static final String TOKEN="unit-test-only-session-token";
    private static final AtomicInteger checks=new AtomicInteger();
    private static void check(boolean ok,String message) { checks.incrementAndGet(); if (!ok) throw new AssertionError(message); }
    private static String sha(File f) throws Exception {
        MessageDigest h=MessageDigest.getInstance("SHA-256"); byte[] buffer=new byte[32768];
        try (InputStream in=new FileInputStream(f)) { for (int n;(n=in.read(buffer))!=-1;) h.update(buffer,0,n); }
        StringBuilder text=new StringBuilder(); for (byte b:h.digest()) text.append(String.format(Locale.ROOT,"%02x",b&255)); return text.toString();
    }
    private static String ack(String sha) { return "{\"ok\":true,\"id\":\"bundle_test-1\",\"sha256\":\""+sha+"\"}"; }
    @FunctionalInterface private interface Operation { void run() throws Exception; }
    private static DiagnosticsTransport.UploadException failed(Operation op) throws Exception {
        try { op.run(); throw new AssertionError("operation unexpectedly succeeded"); }
        catch (DiagnosticsTransport.UploadException e) {
            check(!e.getMessage().contains(TOKEN),"failure must not expose session token"); return e;
        }
    }
    private static final class Server implements AutoCloseable {
        final HttpServer http; final ExecutorService workers;
        Server(HttpHandler handler) throws IOException {
            http=HttpServer.create(new InetSocketAddress(InetAddress.getByAddress(new byte[]{127,0,0,1}),0),0);
            workers=Executors.newCachedThreadPool(r->{Thread t=new Thread(r,"diagnostics-host-test");t.setDaemon(true);return t;});
            http.setExecutor(workers); http.createContext("/api/v1/log-bundles",handler); http.start();
        }
        URL url() throws MalformedURLException { return new URL("http://127.0.0.1:"+http.getAddress().getPort()+"/api/v1/log-bundles"); }
        public void close() { http.stop(0);workers.shutdownNow(); }
    }
    private static byte[] body(HttpExchange x) throws IOException {
        try (InputStream in=x.getRequestBody()) { return in.readAllBytes(); }
    }
    private static void reply(HttpExchange x,int status,byte[] bytes,boolean chunked) throws IOException {
        x.getResponseHeaders().set("Content-Type","application/json");
        try { x.sendResponseHeaders(status,chunked?0:bytes.length); x.getResponseBody().write(bytes); }
        finally { x.close(); }
    }
    private static DiagnosticsTransport.Ack upload(URL url,File f,String sha,DiagnosticsTransport.Cancellation c) throws Exception {
        return DiagnosticsTransport.uploadForLoopbackTest(url,TOKEN,f,sha,c,500,500,2500);
    }
    private static void endpoints(File zip,String digest) throws Exception {
        for (String host:new String[]{"10.0.0.1","172.16.0.1","172.31.255.254","192.168.1.1"}) {
            check(DiagnosticsTransport.validateEndpoint("http://"+host+":8787/api/v1/log-bundles").getHost().equals(host),"private literal allowed");
        }
        for (String text:new String[]{
                "https://192.168.1.1/api/v1/log-bundles","ftp://192.168.1.1/api/v1/log-bundles",
                "http://localhost/api/v1/log-bundles","http://receiver.invalid/api/v1/log-bundles",
                "http://127.0.0.1/api/v1/log-bundles","http://8.8.8.8/api/v1/log-bundles",
                "http://172.15.1.1/api/v1/log-bundles","http://172.32.1.1/api/v1/log-bundles",
                "http://192.169.1.1/api/v1/log-bundles","http://169.254.1.1/api/v1/log-bundles",
                "http://100.64.1.1/api/v1/log-bundles","http://[::1]/api/v1/log-bundles",
                "http://user@192.168.1.1/api/v1/log-bundles","http://192.168.1.1/api/v1/log-bundles?x=1",
                "http://192.168.1.1/api/v1/log-bundles#x","http://192.168.1.1/api/v1/log-bundles/",
                "http://192.168.1.1/other","http://192.168.001.1/api/v1/log-bundles",
                "http://3232235777/api/v1/log-bundles","http://0xc0a80101/api/v1/log-bundles",
                "http://192.168.1.1:0/api/v1/log-bundles","http://192.168.1.1:65536/api/v1/log-bundles",
                "http://192.168.1.1/api/v1/%6cog-bundles","http://192.168.1.1//api/v1/log-bundles"}) {
            check(!failed(()->DiagnosticsTransport.validateEndpoint(text)).retryable,"invalid endpoint is permanent");
        }
        AtomicInteger requests=new AtomicInteger();
        try (Server server=new Server(x->{requests.incrementAndGet();body(x);reply(x,200,ack(digest).getBytes(StandardCharsets.UTF_8),false);})) {
            check(!failed(()->DiagnosticsTransport.upload(server.url(),TOKEN,zip,digest,new DiagnosticsTransport.Cancellation())).retryable,"production rejects loopback");
            for (String bad:new String[]{"", "a\r\nInjected: yes", "a b", "a"+(char)0+"b", "a\u00e9b"}) {
                check(!failed(()->DiagnosticsTransport.uploadForLoopbackTest(server.url(),bad,zip,digest,new DiagnosticsTransport.Cancellation(),500,500,2500)).retryable,"invalid header is permanent");
            }
            check(!failed(()->upload(server.url(),zip,"0".repeat(64),new DiagnosticsTransport.Cancellation())).retryable,"wrong queued digest rejected");
            DiagnosticsTransport.Cancellation cancel=new DiagnosticsTransport.Cancellation();cancel.cancel();
            check(failed(()->upload(server.url(),zip,digest,cancel)).cancelled,"cancel before network");
            check(requests.get()==0,"invalid requests never reach HTTP server");
        }
    }
    private static void successAndGlobals(File zip,String digest) throws Exception {
        CookieHandler oldCookies=CookieHandler.getDefault(); ProxySelector oldProxy=ProxySelector.getDefault(); java.net.Authenticator oldAuth=java.net.Authenticator.getDefault();
        AtomicInteger consulted=new AtomicInteger(); AtomicInteger requests=new AtomicInteger();
        try {
            CookieHandler.setDefault(new CookieHandler() {
                public Map<String,List<String>> get(URI uri,Map<String,List<String>> headers) { consulted.incrementAndGet();return Map.of("Cookie",List.of("must-not-send")); }
                public void put(URI uri,Map<String,List<String>> headers) { consulted.incrementAndGet(); }
            });
            ProxySelector.setDefault(new ProxySelector() {
                public List<Proxy> select(URI uri) {consulted.incrementAndGet();throw new AssertionError("system proxy consulted");}
                public void connectFailed(URI uri,SocketAddress address,IOException e) {consulted.incrementAndGet();}
            });
            java.net.Authenticator.setDefault(new java.net.Authenticator() {protected PasswordAuthentication getPasswordAuthentication(){consulted.incrementAndGet();return new PasswordAuthentication("must-not-send",new char[]{'x'});}});
            for (boolean chunked:new boolean[]{false,true}) try (Server server=new Server(x->{
                requests.incrementAndGet();check(x.getRequestMethod().equals("POST"),"POST method");
                check(x.getRequestURI().toString().equals("/api/v1/log-bundles"),"exact request target");
                check(TOKEN.equals(x.getRequestHeaders().getFirst("X-NVForever-Token")),"explicit session header");
                check("application/zip".equals(x.getRequestHeaders().getFirst("Content-Type")),"ZIP content type");
                check(x.getRequestHeaders().getFirst("Cookie")==null&&x.getRequestHeaders().getFirst("Authorization")==null,"no ambient credentials");
                check(Arrays.equals(body(x),Files.readAllBytes(zip.toPath())),"exact payload bytes");
                x.getResponseHeaders().add("Set-Cookie","ignored=value");
                reply(x,200,ack(digest).getBytes(StandardCharsets.UTF_8),chunked);
            })) {
                DiagnosticsTransport.Ack result=upload(server.url(),zip,digest,new DiagnosticsTransport.Cancellation());
                check(result.id.equals("bundle_test-1")&&result.sha256.equals(digest),"validated receipt");
            }
            try (Server server=new Server(x->{body(x);x.getResponseHeaders().set("WWW-Authenticate","Basic realm=ignored");reply(x,401,"no".getBytes(StandardCharsets.UTF_8),false);})) {
                DiagnosticsTransport.UploadException e=failed(()->upload(server.url(),zip,digest,new DiagnosticsTransport.Cancellation()));
                check(e.statusCode==401&&!e.retryable,"authentication rejection is permanent");
            }
            check(requests.get()==2&&consulted.get()==0,"no ambient proxy/cookies/authenticator used");
        } finally {CookieHandler.setDefault(oldCookies);ProxySelector.setDefault(oldProxy);java.net.Authenticator.setDefault(oldAuth);}
    }
    private static void statusAndAckFailures(File zip,String digest) throws Exception {
        for (int status:new int[]{302,307,400,408,429,500,503,507}) try (Server server=new Server(x->{
            body(x);x.getResponseHeaders().set("Location","http://127.0.0.1:1/not-followed");reply(x,status,"no".getBytes(StandardCharsets.UTF_8),false);
        })) {
            DiagnosticsTransport.UploadException e=failed(()->upload(server.url(),zip,digest,new DiagnosticsTransport.Cancellation()));
            check(e.statusCode==status,"HTTP failure retains status");
            check(e.retryable==(status==408||status==429||status>=500),"bounded retry classification");
        }
        for (String text:new String[]{
                "not JSON","{}","{\"ok\":false,\"id\":\"x\",\"sha256\":\""+digest+"\"}",
                ack("0".repeat(64)),ack(digest)+"tail",ack(digest).replace("bundle_test-1","../unsafe"),
                ack(digest).replace("bundle_test-1","bad id"),ack(digest).replace("true","1"),
                ack(digest).replace("\"id\":","\"extra\":0,\"id\":"),
                ack(digest).replace("\"id\":","\"id\":\"duplicate\",\"id\":"),
                ack(digest).replace("\"sha256\":\""+digest+"\"","\"sha256\":7")}) {
            try (Server server=new Server(x->{body(x);reply(x,200,text.getBytes(StandardCharsets.UTF_8),false);})) {
                check(!failed(()->upload(server.url(),zip,digest,new DiagnosticsTransport.Cancellation())).retryable,"invalid acknowledgement is permanent");
            }
        }
        for (boolean chunked:new boolean[]{false,true}) try (Server server=new Server(x->{body(x);reply(x,200,new byte[DiagnosticsTransport.MAX_ACK_BYTES+1],chunked);})) {
            check(!failed(()->upload(server.url(),zip,digest,new DiagnosticsTransport.Cancellation())).retryable,"oversized acknowledgement rejected");
        }
        try (Server server=new Server(x->{body(x);reply(x,200,new byte[]{(byte)255},false);})) {
            check(!failed(()->upload(server.url(),zip,digest,new DiagnosticsTransport.Cancellation())).retryable,"invalid UTF-8 rejected");
        }
        try (Server server=new Server(x->{
            body(x);byte[] bytes=ack(digest).getBytes(StandardCharsets.UTF_8);
            try {x.sendResponseHeaders(200,bytes.length+10);x.getResponseBody().write(bytes);}
            finally {x.close();}
        })) {
            check(failed(()->upload(server.url(),zip,digest,new DiagnosticsTransport.Cancellation())).retryable,"truncated acknowledgement is retryable network failure");
        }
    }
    private static void offlineCancelTimeoutAndBounds(File zip,String digest,Path temp) throws Exception {
        int port;try (ServerSocket socket=new ServerSocket(0,1,InetAddress.getByAddress(new byte[]{127,0,0,1}))) {port=socket.getLocalPort();}
        URL offline=new URL("http://127.0.0.1:"+port+"/api/v1/log-bundles");
        check(failed(()->upload(offline,zip,digest,new DiagnosticsTransport.Cancellation())).retryable,"offline retryable");
        Path large=temp.resolve("oversized.zip");try (RandomAccessFile f=new RandomAccessFile(large.toFile(),"rw")){f.setLength(DiagnosticsTransport.MAX_ZIP_BYTES+1);}
        check(!failed(()->upload(offline,large.toFile(),digest,new DiagnosticsTransport.Cancellation())).retryable,"oversized ZIP rejected before connection");
        CountDownLatch arrived=new CountDownLatch(1),release=new CountDownLatch(1);
        try (Server server=new Server(x->{body(x);arrived.countDown();try{release.await(3,TimeUnit.SECONDS);}catch(InterruptedException e){Thread.currentThread().interrupt();}finally{x.close();}})) {
            DiagnosticsTransport.Cancellation cancel=new DiagnosticsTransport.Cancellation();
            ExecutorService executor=Executors.newSingleThreadExecutor();
            try {
                Future<DiagnosticsTransport.UploadException> result=executor.submit(()->failed(()->upload(server.url(),zip,digest,cancel)));
                check(arrived.await(2,TimeUnit.SECONDS),"request reached local test server");cancel.cancel();
                check(result.get(2,TimeUnit.SECONDS).cancelled,"cancellation closes active socket promptly");
            } finally {release.countDown();executor.shutdownNow();}
        }
        try (Server server=new Server(x->{body(x);try{Thread.sleep(1000);}catch(InterruptedException e){Thread.currentThread().interrupt();}finally{x.close();}})) {
            check(failed(()->DiagnosticsTransport.uploadForLoopbackTest(server.url(),TOKEN,zip,digest,new DiagnosticsTransport.Cancellation(),200,100,1500)).retryable,"read timeout bounded");
        }
        Path stalled=temp.resolve("stalled.zip");try(RandomAccessFile f=new RandomAccessFile(stalled.toFile(),"rw")){f.setLength(DiagnosticsTransport.MAX_ZIP_BYTES);}
        String largeDigest=sha(stalled.toFile());
        try (Server server=new Server(x->{try{Thread.sleep(1500);}catch(InterruptedException e){Thread.currentThread().interrupt();}finally{x.close();}})) {
            check(failed(()->DiagnosticsTransport.uploadForLoopbackTest(server.url(),TOKEN,stalled.toFile(),largeDigest,new DiagnosticsTransport.Cancellation(),200,500,100)).retryable,"overall deadline also bounds a stalled request write");
        }
    }
    public static void main(String[] args) throws Exception {
        Path temp=Files.createTempDirectory("nvf-diagnostics-transport-test-");
        try {
            File zip=temp.resolve("synthetic.zip").toFile();
            try(ZipOutputStream out=new ZipOutputStream(new FileOutputStream(zip))){out.putNextEntry(new ZipEntry("log.txt"));out.write("synthetic diagnostic fixture\n".getBytes(StandardCharsets.UTF_8));out.closeEntry();}
            String digest=sha(zip);endpoints(zip,digest);successAndGlobals(zip,digest);statusAndAckFailures(zip,digest);offlineCancelTimeoutAndBounds(zip,digest,temp);
            System.out.println("DiagnosticsTransportTest: PASS ("+checks.get()+" checks; loopback-only network)");
        } finally {
            try(java.util.stream.Stream<Path> paths=Files.walk(temp)){paths.sorted(Comparator.reverseOrder()).forEach(p->{try{Files.delete(p);}catch(IOException e){throw new UncheckedIOException(e);}});}
        }
    }
}
