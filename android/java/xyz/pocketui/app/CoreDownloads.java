package xyz.pocketui.app;

import android.content.Context;
import android.net.*;
import java.io.*;
import java.net.*;

/** Worker transport for store packages. Rust owns signature, checksum and safe extraction. */
final class CoreDownloads {
    private final Context app;
    private volatile boolean cancelled;
    private volatile long bytes;
    CoreDownloads(Context app){this.app=app;}
    String begin(){cancelled=false;bytes=0;return "";}
    String cancel(){cancelled=true;return "";}
    String progress(){return Long.toString(bytes);}
    private Network wifi() throws IOException {
        ConnectivityManager cm=(ConnectivityManager)app.getSystemService(Context.CONNECTIVITY_SERVICE);
        Network n=cm.getActiveNetwork();NetworkCapabilities c=cm.getNetworkCapabilities(n);
        if(c==null||!c.hasTransport(NetworkCapabilities.TRANSPORT_WIFI)||!c.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED))
            throw new IOException("Downloads wait for unmetered Wi-Fi.");
        return n;
    }
    private File local(String address) throws Exception {
        File f=new File(new URI(address)).getCanonicalFile();
        File external=app.getExternalFilesDir(null);
        if(!f.toPath().startsWith(app.getFilesDir().getCanonicalFile().toPath())
            &&(external==null||!f.toPath().startsWith(external.getCanonicalFile().toPath())))throw new IOException("Invalid local channel");
        return f;
    }
    String read(String address) throws Exception {
        InputStream input;
        HttpURLConnection connection=null;
        if(address.startsWith("file:"))input=new FileInputStream(local(address));
        else{
            Network n=wifi();URL url=new URL(address);
            for(int hop=0;;hop++){
                if(hop>=6||!"https".equals(url.getProtocol()))throw new IOException("HTTPS required");
                connection=(HttpURLConnection)n.openConnection(url);connection.setInstanceFollowRedirects(false);
                connection.setConnectTimeout(10000);connection.setReadTimeout(10000);
                int code=connection.getResponseCode();
                if(code==301||code==302||code==303||code==307||code==308){
                    String next=connection.getHeaderField("Location");connection.disconnect();
                    if(next==null)throw new IOException("Missing redirect");url=new URL(url,next);continue;
                }
                if(code!=200){connection.disconnect();throw new IOException("HTTP "+code);}
                input=connection.getInputStream();break;
            }
        }
        try(InputStream in=input;ByteArrayOutputStream out=new ByteArrayOutputStream()){
            byte[] b=new byte[16384];int count,total=0;long deadline=System.nanoTime()+60L*1000000000;
            while((count=in.read(b))!=-1){if(cancelled)throw new IOException("Download cancelled. Choose it again to continue.");if(System.nanoTime()>deadline)throw new IOException("The download took too long. Check Wi-Fi and try again.");if((total+=count)>1024*1024)throw new IOException("Catalog too large");out.write(b,0,count);}
            return out.toString("UTF-8");
        }finally{if(connection!=null)connection.disconnect();}
    }
    String fetch(String address,String hash,String length) throws Exception {
        if(!hash.matches("[a-f0-9]{64}"))throw new IOException("Invalid package checksum");
        long size=Long.parseLong(length);if(size<=0||size>128L*1024*1024)throw new IOException("Invalid package size");
        File dir=new File(app.getFilesDir(),"store/partials");if(!dir.isDirectory()&&!dir.mkdirs())throw new IOException("Download folder unavailable");
        File dest=new File(dir,hash+".part");
        ResumableDownload.Hooks hooks=new ResumableDownload.Hooks(){
            public void check() throws IOException {if(cancelled)throw new IOException("Download cancelled. Choose it again to continue.");if(!address.startsWith("file:"))wifi();}
            public void progress(long value){bytes=value;}
        };
        if(address.startsWith("file:")){
            File source=local(address);if(source.length()!=size)throw new IOException("Download size changed");
            long offset=dest.length();if(offset>size){if(!dest.delete())throw new IOException("Partial file unavailable");offset=0;}
            try(RandomAccessFile in=new RandomAccessFile(source,"r");FileOutputStream out=new FileOutputStream(dest,true)){
                in.seek(offset);bytes=offset;byte[] b=new byte[32768];int count;
                while((count=in.read(b))!=-1){hooks.check();out.write(b,0,count);bytes+=count;}out.getFD().sync();
            }
        }else{
            Network n=wifi();ResumableDownload.fetch(address,dest,size,url->(HttpURLConnection)n.openConnection(url),hooks);
        }
        hooks.check();return dest.getAbsolutePath();
    }
}
