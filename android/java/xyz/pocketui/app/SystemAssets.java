package xyz.pocketui.app;

import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;

/** Replaceable emulator files, copied before the core/audio starts. No saves live here. */
final class SystemAssets {
    interface Source {
        String[] list(String path) throws IOException;
        InputStream open(String path) throws IOException;
    }
    static synchronized int install(Source source, String path, File destination) throws IOException {
        String[] children=source.list(path);
        if(children.length>0){
            if(!destination.isDirectory()&&!destination.mkdirs())throw new IOException("System folder unavailable");
            int copied=0;
            for(String child:children)copied+=install(source,path+"/"+child,new File(destination,child));
            return copied;
        }
        // As on Linux, a player's existing font or compatibility file wins. Missing files in a
        // partial installation are filled in; a failed copy never publishes a partial font.
        if(destination.isFile())return 0;
        File part=new File(destination.getParentFile(),destination.getName()+".pocket-part");
        try {
            try(InputStream input=source.open(path); FileOutputStream output=new FileOutputStream(part)){
                byte[] buffer=new byte[16384];int n;
                while((n=input.read(buffer))!=-1)output.write(buffer,0,n);
            }
            if(!part.renameTo(destination))throw new IOException("System file unavailable");
            return 1;
        } finally { if(part.exists())part.delete(); }
    }
}
