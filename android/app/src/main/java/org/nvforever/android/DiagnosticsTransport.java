package org.nvforever.android;

import java.io.*;
import java.net.*;
import java.nio.ByteBuffer;
import java.nio.charset.*;
import java.security.*;
import java.util.*;
import java.util.concurrent.*;
import java.util.concurrent.atomic.*;

/** Bounded, opt-in LAN transport. The caller owns queue files and pairing settings.
 * Only a literal private IPv4 HTTP endpoint is admitted. Direct sockets avoid global
 * URL proxy, cookie and authenticator state. No endpoint, token or payload is logged.
 * A successful result acknowledges the exact bytes sent, not merely HTTP success.
 */
public final class DiagnosticsTransport {
    public static final long MAX_ZIP_BYTES = 16L * 1024 * 1024;
    public static final int MAX_ACK_BYTES = 4096;
    private static final String PATH = "/api/v1/log-bundles";
    private static final int CONNECT_MS = 3000, READ_MS = 5000, TOTAL_MS = 30000;
    private static final ScheduledExecutorService DEADLINES = Executors.newSingleThreadScheduledExecutor(r -> {
        Thread t = new Thread(r, "NVForever-log-upload-deadline"); t.setDaemon(true); return t;
    });
    private DiagnosticsTransport() {}

    public static final class Ack {
        public final String id, sha256;
        private Ack(String id, String sha256) { this.id = id; this.sha256 = sha256; }
    }
    public static final class UploadException extends IOException {
        public final boolean retryable, cancelled;
        public final int statusCode;
        private UploadException(String message, boolean retryable, boolean cancelled, int statusCode) {
            super(message); this.retryable = retryable; this.cancelled = cancelled; this.statusCode = statusCode;
        }
    }
    /** One in-flight operation per instance. A cancelled instance stays cancelled. */
    public static final class Cancellation {
        private final AtomicBoolean cancelled = new AtomicBoolean();
        private final AtomicReference<Socket> active = new AtomicReference<>();
        public boolean isCancelled() { return cancelled.get(); }
        public void cancel() { cancelled.set(true); close(active.get()); }
        private void attach(Socket socket) throws UploadException {
            if (!active.compareAndSet(null, socket)) throw failure("Cancellation is already in use", false, 0);
            if (isCancelled()) { active.compareAndSet(socket, null); close(socket); throw cancelled(); }
        }
        private void detach(Socket socket) { active.compareAndSet(socket, null); }
    }
    private static UploadException failure(String text, boolean retryable, int status) {
        return new UploadException(text, retryable, false, status);
    }
    private static UploadException cancelled() { return new UploadException("Upload cancelled", false, true, 0); }
    private static void check(Cancellation c) throws UploadException { if (c.isCancelled()) throw cancelled(); }
    private static void close(Socket s) { if (s != null) try { s.close(); } catch (IOException ignored) {} }

    public static URL validateEndpoint(String text) throws UploadException {
        if (text==null || text.length()>256) throw failure("Invalid upload endpoint",false,0);
        try {
            URL url = new URI(text).toURL(); endpoint(url, false); return url;
        } catch (UploadException e) { throw e; }
        catch (Exception e) { throw failure("Invalid upload endpoint", false, 0); }
    }
    private static final class Endpoint {
        final byte[] address; final String host; final int port;
        Endpoint(byte[] address, String host, int port) { this.address=address; this.host=host; this.port=port; }
    }
    private static Endpoint endpoint(URL url, boolean testLoopback) throws UploadException {
        if (url == null || !"http".equals(url.getProtocol()) || url.getUserInfo() != null
                || url.getQuery() != null || url.getRef() != null || !PATH.equals(url.getPath()))
            throw failure("Invalid upload endpoint", false, 0);
        String host = url.getHost();
        if (host == null || host.length()>15 || !host.matches("[0-9]+\\.[0-9]+\\.[0-9]+\\.[0-9]+"))
            throw failure("Upload endpoint must be a literal private IPv4 address", false, 0);
        String[] parts = host.split("\\."); byte[] address = new byte[4]; int[] value = new int[4];
        for (int i=0;i<4;i++) {
            if (parts[i].length()>3 || (parts[i].length()>1 && parts[i].charAt(0)=='0'))
                throw failure("Invalid upload IPv4 address", false, 0);
            value[i]=Integer.parseInt(parts[i]);
            if (value[i]>255) throw failure("Invalid upload IPv4 address", false, 0);
            address[i]=(byte)value[i];
        }
        boolean privateAddress = value[0]==10 || (value[0]==172 && value[1]>=16 && value[1]<=31)
                || (value[0]==192 && value[1]==168);
        boolean loopback = testLoopback && host.equals("127.0.0.1");
        int port=url.getPort()==-1 ? 80 : url.getPort();
        if ((!privateAddress && !loopback) || port<1 || port>65535)
            throw failure("Upload endpoint must be private IPv4 HTTP", false, 0);
        return new Endpoint(address,host,port);
    }
    public static Ack upload(URL endpoint, String token, File zip, String expectedSha256,
                             Cancellation cancellation) throws UploadException {
        return upload(endpoint,token,zip,expectedSha256,cancellation,false,CONNECT_MS,READ_MS,TOTAL_MS);
    }
    // Test seam admits only 127.0.0.1 in addition to production private addresses.
    // It cannot replace the HTTP implementation, disable hashing, or enlarge limits.
    static Ack uploadForLoopbackTest(URL endpoint, String token, File zip, String sha,
                                    Cancellation cancel, int connect, int read, int total) throws UploadException {
        if (connect<1 || connect>CONNECT_MS || read<1 || read>READ_MS || total<1 || total>TOTAL_MS)
            throw failure("Invalid test timeout",false,0);
        return upload(endpoint,token,zip,sha,cancel,true,connect,read,total);
    }
    private static Ack upload(URL url, String token, File zip, String expected,
                              Cancellation cancellation, boolean testLoopback,
                              int connectMs,int readMs,int totalMs) throws UploadException {
        if (cancellation == null) throw failure("Cancellation is required",false,0);
        check(cancellation);
        Endpoint target=endpoint(url,testLoopback);
        if (token==null || token.isEmpty() || token.length()>512) throw failure("Invalid session token",false,0);
        for (int i=0;i<token.length();i++) if (token.charAt(i)<33 || token.charAt(i)>126)
            throw failure("Invalid session token",false,0);
        if (expected==null || !expected.matches("[0-9a-f]{64}")) throw failure("Invalid bundle digest",false,0);
        if (zip==null || !zip.isFile()) throw failure("Bundle file is unavailable",false,0);
        long length=zip.length();
        if (length<1 || length>MAX_ZIP_BYTES) throw failure("Bundle exceeds upload size bounds",false,0);
        if (!digest(zip,cancellation).equals(expected)) throw failure("Bundle digest differs from queued digest",false,0);
        Socket socket=new Socket(Proxy.NO_PROXY);
        AtomicBoolean timedOut=new AtomicBoolean(); ScheduledFuture<?> deadline=null;
        try {
            cancellation.attach(socket);
            deadline=DEADLINES.schedule(() -> { timedOut.set(true); close(socket); },totalMs,TimeUnit.MILLISECONDS);
            socket.connect(new InetSocketAddress(InetAddress.getByAddress(target.address),target.port),connectMs);
            socket.setSoTimeout(readMs); check(cancellation);
            OutputStream output=socket.getOutputStream();
            String headers="POST "+PATH+" HTTP/1.1\r\nHost: "+target.host+":"+target.port
                    +"\r\nX-NVForever-Token: "+token+"\r\nContent-Type: application/zip\r\nAccept: application/json"
                    +"\r\nContent-Length: "+length+"\r\nConnection: close\r\n\r\n";
            output.write(headers.getBytes(StandardCharsets.US_ASCII));
            MessageDigest sent=sha256(); long count=0; byte[] buffer=new byte[32768];
            try (InputStream input=new FileInputStream(zip)) {
                for (int n;(n=input.read(buffer))!=-1;) {
                    check(cancellation); count+=n;
                    if (count>length || count>MAX_ZIP_BYTES) throw failure("Bundle changed while uploading",false,0);
                    output.write(buffer,0,n); sent.update(buffer,0,n);
                }
            }
            output.flush(); check(cancellation);
            if (count!=length || !hex(sent.digest()).equals(expected)) throw failure("Bundle changed while uploading",false,0);
            InputStream input=new BufferedInputStream(socket.getInputStream());
            Response response=readResponse(input,cancellation);
            if (response.status<200 || response.status>=300) {
                boolean retry=response.status==408 || response.status==429 || response.status>=500;
                throw failure(response.status>=300 && response.status<400 ? "Upload redirect refused" : "Upload server refused request",retry,response.status);
            }
            byte[] body=readBody(input,response,cancellation);
            check(cancellation);
            return parseAck(body,expected);
        } catch (UploadException e) {
            if (cancellation.isCancelled()) throw cancelled();
            throw e;
        } catch (IOException e) {
            if (cancellation.isCancelled()) throw cancelled();
            throw failure(timedOut.get() || e instanceof SocketTimeoutException ? "Upload timed out" : "Upload connection failed",true,0);
        } finally {
            if (deadline!=null) deadline.cancel(false);
            cancellation.detach(socket); close(socket);
        }
    }
    private static String digest(File file,Cancellation cancel) throws UploadException {
        MessageDigest hash=sha256(); long count=0; byte[] buffer=new byte[32768];
        try (InputStream input=new FileInputStream(file)) {
            for (int n;(n=input.read(buffer))!=-1;) {
                check(cancel); count+=n;
                if (count>MAX_ZIP_BYTES) throw failure("Bundle exceeds upload size bounds",false,0);
                hash.update(buffer,0,n);
            }
            return hex(hash.digest());
        } catch (UploadException e) { throw e; }
        catch (IOException e) { throw failure("Bundle cannot be read",false,0); }
    }
    private static MessageDigest sha256() {
        try { return MessageDigest.getInstance("SHA-256"); }
        catch (NoSuchAlgorithmException impossible) { throw new IllegalStateException("SHA-256 unavailable"); }
    }
    private static String hex(byte[] bytes) {
        char[] alphabet="0123456789abcdef".toCharArray(),out=new char[bytes.length*2];
        for (int i=0;i<bytes.length;i++) { out[i*2]=alphabet[(bytes[i]&255)>>>4]; out[i*2+1]=alphabet[bytes[i]&15]; }
        return new String(out);
    }
    private static final class Response {
        int status; long length=-1; boolean chunked;
    }
    private static Response readResponse(InputStream input,Cancellation cancel) throws IOException {
        String status=line(input,1024,cancel);
        if (!status.matches("HTTP/1\\.[01] [0-9]{3}( .*)?")) throw failure("Malformed upload response",false,0);
        Response result=new Response(); result.status=Integer.parseInt(status.substring(9,12));
        int total=status.length()+2; boolean lengthSeen=false,transferSeen=false;
        for (int count=0;;count++) {
            String header=line(input,1024,cancel); total+=header.length()+2;
            if (total>8192 || count>64) throw failure("Upload response headers exceed bounds",false,result.status);
            if (header.isEmpty()) break;
            int colon=header.indexOf(':');
            if (colon<=0) throw failure("Malformed upload response header",false,result.status);
            String name=header.substring(0,colon).toLowerCase(Locale.ROOT),value=header.substring(colon+1).trim();
            if (!name.matches("[a-z0-9!#$%&'*+.^_`|~-]+")) throw failure("Malformed upload response header",false,result.status);
            if (name.equals("content-length")) {
                if (lengthSeen || !value.matches("[0-9]{1,10}")) throw failure("Invalid upload response length",false,result.status);
                lengthSeen=true; result.length=Long.parseLong(value);
            } else if (name.equals("transfer-encoding")) {
                if (transferSeen || !value.equalsIgnoreCase("chunked")) throw failure("Unsupported upload response framing",false,result.status);
                transferSeen=true; result.chunked=true;
            } else if (name.equals("content-encoding") && !value.equalsIgnoreCase("identity"))
                throw failure("Compressed upload response refused",false,result.status);
        }
        if ((lengthSeen && transferSeen) || (result.status>=200 && result.status<300 && result.length>MAX_ACK_BYTES))
            throw failure("Upload response body exceeds bounds or has ambiguous framing",false,result.status);
        return result;
    }
    private static String line(InputStream input,int max,Cancellation cancel) throws IOException {
        ByteArrayOutputStream out=new ByteArrayOutputStream();
        for (;;) {
            check(cancel); int b=input.read();
            if (b<0) throw new EOFException();
            if (b=='\r') { if (input.read()!='\n') throw new EOFException(); return out.toString("ISO-8859-1"); }
            if (b=='\n' || b<32 && b!='\t' || b==127 || out.size()>=max)
                throw failure("Malformed or oversized upload response line",false,0);
            out.write(b);
        }
    }
    private static byte[] readBody(InputStream input,Response response,Cancellation cancel) throws IOException {
        ByteArrayOutputStream out=new ByteArrayOutputStream();
        if (response.chunked) {
            int trailers=0;
            for (;;) {
                String size=line(input,32,cancel);
                if (!size.matches("[0-9a-fA-F]{1,8}")) throw failure("Malformed upload response chunk",false,response.status);
                long n=Long.parseLong(size,16);
                if (n==0) {
                    for (;;) { String trailer=line(input,1024,cancel); trailers+=trailer.length()+2; if (trailers>8192) throw failure("Oversized upload response trailers",false,response.status); if (trailer.isEmpty()) return out.toByteArray(); }
                }
                if (n>MAX_ACK_BYTES-out.size()) throw failure("Upload acknowledgement exceeds bounds",false,response.status);
                copyExactly(input,out,(int)n,cancel);
                if (input.read()!='\r' || input.read()!='\n') throw new EOFException();
            }
        }
        if (response.length>=0) { copyExactly(input,out,(int)response.length,cancel); return out.toByteArray(); }
        for (;;) {
            check(cancel); int b=input.read(); if (b<0) return out.toByteArray();
            if (out.size()>=MAX_ACK_BYTES) throw failure("Upload acknowledgement exceeds bounds",false,response.status);
            out.write(b);
        }
    }
    private static void copyExactly(InputStream input,OutputStream output,int bytes,Cancellation cancel) throws IOException {
        byte[] buffer=new byte[1024]; int left=bytes;
        while (left>0) { check(cancel); int n=input.read(buffer,0,Math.min(left,buffer.length)); if (n<0) throw new EOFException(); output.write(buffer,0,n); left-=n; }
    }
    private static Ack parseAck(byte[] body,String expected) throws UploadException {
        final String text;
        try { text=StandardCharsets.UTF_8.newDecoder().onMalformedInput(CodingErrorAction.REPORT).onUnmappableCharacter(CodingErrorAction.REPORT).decode(ByteBuffer.wrap(body)).toString(); }
        catch (CharacterCodingException e) { throw failure("Acknowledgement is not UTF-8 JSON",false,0); }
        Json p=new Json(text); p.space(); p.expect('{'); int fields=0; String id=null,sha=null; boolean ok=false;
        p.space();
        if (!p.peek('}')) for (;;) {
            String key=p.string(); p.space(); p.expect(':'); p.space(); int bit;
            if (key.equals("ok")) { bit=1; p.literal("true"); ok=true; }
            else if (key.equals("id")) { bit=2; id=p.string(); }
            else if (key.equals("sha256")) { bit=4; sha=p.string(); }
            else throw failure("Unknown acknowledgement field",false,0);
            if ((fields&bit)!=0) throw failure("Duplicate acknowledgement field",false,0); fields|=bit;
            p.space(); if (p.peek('}')) break; p.expect(','); p.space();
        }
        p.expect('}'); p.space();
        if (p.at!=text.length() || fields!=7 || !ok || id==null || !id.matches("[A-Za-z0-9][A-Za-z0-9_-]{0,127}") || !expected.equals(sha))
            throw failure("Upload acknowledgement does not match the bundle",false,0);
        return new Ack(id,sha);
    }
    private static final class Json {
        final String text; int at;
        Json(String text) { this.text=text; }
        void space() { while (at<text.length() && " \t\r\n".indexOf(text.charAt(at))>=0) at++; }
        boolean peek(char c) { return at<text.length() && text.charAt(at)==c; }
        void expect(char c) throws UploadException { if (!peek(c)) throw failure("Malformed acknowledgement JSON",false,0); at++; }
        void literal(String value) throws UploadException { if (!text.startsWith(value,at)) throw failure("Malformed acknowledgement JSON",false,0); at+=value.length(); }
        String string() throws UploadException {
            expect('"'); StringBuilder out=new StringBuilder();
            while (at<text.length()) {
                char c=text.charAt(at++); if (c=='"') return out.toString();
                if (c<32) throw failure("Malformed acknowledgement JSON",false,0);
                if (c=='\\') {
                    if (at==text.length()) throw failure("Malformed acknowledgement JSON",false,0);
                    c=text.charAt(at++);
                    switch (c) {
                        case '"': case '\\': case '/': break;
                        case 'b': c='\b'; break; case 'f': c='\f'; break; case 'n': c='\n'; break;
                        case 'r': c='\r'; break; case 't': c='\t'; break;
                        case 'u':
                            if (at+4>text.length()) throw failure("Malformed acknowledgement JSON",false,0);
                            String digits=text.substring(at,at+4); if (!digits.matches("[0-9a-fA-F]{4}")) throw failure("Malformed acknowledgement JSON",false,0);
                            c=(char)Integer.parseInt(digits,16); at+=4; break;
                        default: throw failure("Malformed acknowledgement JSON",false,0);
                    }
                }
                out.append(c);
            }
            throw failure("Malformed acknowledgement JSON",false,0);
        }
    }
}
