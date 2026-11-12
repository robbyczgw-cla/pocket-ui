package xyz.pocketui.app;
import java.io.*;
import java.net.*;
import java.nio.file.*;
import java.util.*;

public final class ResumableDownloadTest {
    static final byte[] DATA=new byte[200000];
    static class Response extends HttpURLConnection {
        final boolean ranges; int offset; boolean connected;
        Response(URL u,boolean ranges){super(u);this.ranges=ranges;}
        public int getResponseCode(){String range=getRequestProperty("Range");offset=ranges&&range!=null?Integer.parseInt(range.substring(6,range.length()-1)):0;connected=true;return offset>0?206:200;}
        public long getContentLengthLong(){return DATA.length-offset;}
        public String getHeaderField(String key){return key.equals("Content-Range")?"bytes "+offset+"-"+(DATA.length-1)+"/"+DATA.length:null;}
        public InputStream getInputStream(){return new ByteArrayInputStream(DATA,offset,DATA.length-offset);}
        public void connect(){} public void disconnect(){} public boolean usingProxy(){return false;}
    }
    static ResumableDownload.Hooks hooks(long cancelAfter){return new ResumableDownload.Hooks(){long bytes;public void check()throws IOException{if(bytes>=cancelAfter)throw new IOException("cancel");}public void progress(long n){bytes=n;}};}
    public static void main(String[] args)throws Exception{
        new Random(42).nextBytes(DATA);Path root=Files.createTempDirectory("core-download-test");
        try{
            File file=root.resolve("partial").toFile();
            try{ResumableDownload.fetch("https://example.test/core",file,DATA.length,u->new Response(u,true),hooks(60000));throw new AssertionError("cancel ignored");}
            catch(IOException e){if(!e.getMessage().equals("cancel"))throw e;}
            long kept=file.length();if(kept<60000||kept>=DATA.length)throw new AssertionError("partial lost");
            final String[] range={null};
            ResumableDownload.fetch("https://example.test/core",file,DATA.length,u->new Response(u,true){public int getResponseCode(){range[0]=getRequestProperty("Range");return super.getResponseCode();}},hooks(Long.MAX_VALUE));
            if(!("bytes="+kept+"-").equals(range[0])||!Arrays.equals(DATA,Files.readAllBytes(file.toPath())))throw new AssertionError("resume corrupt");
            Files.write(file.toPath(),Arrays.copyOf(DATA,12345));
            ResumableDownload.fetch("https://example.test/core",file,DATA.length,u->new Response(u,false),hooks(Long.MAX_VALUE));
            if(!Arrays.equals(DATA,Files.readAllBytes(file.toPath())))throw new AssertionError("ignored range appended");
            file.delete();
            try{ResumableDownload.fetch("http://example.test/core",file,DATA.length,u->new Response(u,true),hooks(Long.MAX_VALUE));throw new AssertionError("HTTP accepted");}catch(IOException expected){}
            if(file.exists())throw new AssertionError("insecure response written");
            System.out.println("PASS cancel keeps bytes; Range resumes; ignored Range replaces; HTTP refused");
        }finally{Files.deleteIfExists(root.resolve("partial"));Files.delete(root);}
    }
}
