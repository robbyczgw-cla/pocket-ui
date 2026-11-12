package xyz.pocketui.app;

import java.io.*;
import java.net.*;

/** Transport only. The caller verifies the signed catalog and SHA-256 before installing. */
final class ResumableDownload {
    interface Open { HttpURLConnection open(URL url) throws IOException; }
    interface Hooks { void check() throws IOException; void progress(long bytes); }
    static void fetch(String address, File file, long size, Open open, Hooks hooks) throws IOException {
        if(size<=0||size>128L*1024*1024)throw new IOException("Invalid download size");
        if(file.length()>size&&!file.delete())throw new IOException("Partial file unavailable");
        hooks.check();hooks.progress(file.length());
        if(file.isFile()&&file.length()==size)return;
        URL url=new URL(address);
        long deadline=System.nanoTime()+30L*60*1000000000;
        for(int hop=0;hop<6;hop++){
            if(!"https".equals(url.getProtocol()))throw new IOException("HTTPS required");
            hooks.check();long offset=file.length();
            HttpURLConnection c=open.open(url);
            c.setConnectTimeout(10000);c.setReadTimeout(10000);c.setInstanceFollowRedirects(false);
            c.setRequestProperty("Accept-Encoding","identity");
            if(offset>0)c.setRequestProperty("Range","bytes="+offset+"-");
            try{
                int code=c.getResponseCode();
                if(code==301||code==302||code==303||code==307||code==308){
                    String next=c.getHeaderField("Location");if(next==null)throw new IOException("Missing redirect");
                    url=new URL(url,next);continue;
                }
                boolean append=code==206&&offset>0;
                if(code!=200&&!append)throw new IOException("HTTP "+code);
                if(append&&!(("bytes "+offset+"-"+(size-1)+"/"+size).equals(c.getHeaderField("Content-Range"))))
                    throw new IOException("Invalid download range");
                if(!append)offset=0; // Server ignored Range: replace rather than append a second archive.
                long length=c.getContentLengthLong();
                if(length>=0&&length!=size-offset)throw new IOException("Download size changed");
                try(InputStream in=c.getInputStream();FileOutputStream out=new FileOutputStream(file,append)){
                    byte[] buffer=new byte[32768];long total=offset;int n;
                    long speedAt=System.nanoTime(),speedBytes=offset;
                    while(true){
                        hooks.check();if(System.nanoTime()>deadline)throw new IOException("Download took too long");
                        n=in.read(buffer);if(n<0)break;
                        if(total+n>size)throw new IOException("Download too large");
                        out.write(buffer,0,n);total+=n;hooks.progress(total);
                        if(System.nanoTime()-speedAt>=30L*1000000000){
                            if(total-speedBytes<307200)throw new IOException("The download took too long. Check Wi-Fi and try again.");
                            speedAt=System.nanoTime();speedBytes=total;
                        }
                    }
                    out.getFD().sync();
                    if(total!=size)throw new IOException("Download incomplete");
                }
                return;
            }finally{c.disconnect();}
        }
        throw new IOException("Too many redirects");
    }
}
