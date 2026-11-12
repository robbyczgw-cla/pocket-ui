package xyz.pocketui.app;

import java.io.*;
import java.util.Locale;

/** BIOS copies stay private. A short, unreadable or oversized replacement never damages a kept BIOS. */
final class BiosFiles {
    static int size(String name) {
        switch(name) {
            case "dc/dc_boot.bin": return 2<<20;
            case "dc/dc_flash.bin": return 128<<10;
            case "saturn_bios.bin": case "sega_101.bin": case "mpr-17933.bin": return 512<<10;
            default:
                if(name.matches("scph[0-9]{3,5}[a-z]?\\.bin")||name.equals("psxonpsp660.bin"))return 512<<10;
                return 0;
        }
    }
    static void install(File folder,String name,InputStream in) throws IOException {
        int expected=size(name);
        if(expected==0||!name.equals(name.toLowerCase(Locale.ROOT)))throw new IOException("Unknown BIOS name");
        File done=new File(folder,name), parent=done.getParentFile(), part=new File(parent,done.getName()+".part");
        if(!parent.mkdirs()&&!parent.isDirectory())throw new IOException("Cannot prepare BIOS folder");
        try(FileOutputStream out=new FileOutputStream(part)) {
            if(in==null)throw new FileNotFoundException("No BIOS stream");
            byte[] bytes=new byte[65536];int n,total=0;
            while((n=in.read(bytes))!=-1){total+=n;if(total>expected)throw new IOException("BIOS is too large");out.write(bytes,0,n);}
            if(total!=expected)throw new IOException("Wrong BIOS size: "+total);
            out.getFD().sync();
        }catch(IOException|RuntimeException e){part.delete();throw e;}
        if(!part.renameTo(done)){part.delete();throw new IOException("Cannot keep BIOS copy");}
    }
    static boolean hasDreamcast(File folder) {
        File p=new File(folder,"dc/dc_boot.bin");return p.isFile()&&p.length()==size("dc/dc_boot.bin");
    }
    static boolean hasSaturn(File folder) {
        for(String name:new String[]{"saturn_bios.bin","sega_101.bin","mpr-17933.bin"}) {
            File p=new File(folder,name);if(p.isFile()&&p.length()==size(name))return true;
        }
        return false;
    }
}
