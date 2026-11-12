package xyz.pocketui.app;

import java.io.*;
import java.nio.file.*;
import java.util.Arrays;

public class BiosFilesTest {
    public static void main(String[] args) throws Exception {
        File root=Files.createTempDirectory("sega-bios-").toFile();
        try {
            for(String name:new String[]{"saturn_bios.bin","sega_101.bin","mpr-17933.bin","dc/dc_boot.bin","dc/dc_flash.bin","scph5501.bin"}) {
                byte[] original=new byte[BiosFiles.size(name)];Arrays.fill(original,(byte)0x5a);
                BiosFiles.install(root,name,new ByteArrayInputStream(original));
                File target=new File(root,name);
                if(!Arrays.equals(original,Files.readAllBytes(target.toPath())))throw new AssertionError(name);
                for(int len:new int[]{original.length-1,original.length+1}) {
                    try {BiosFiles.install(root,name,new ByteArrayInputStream(new byte[len]));throw new AssertionError("Bad size accepted: "+name);}
                    catch(IOException expected){}
                    if(!Arrays.equals(original,Files.readAllBytes(target.toPath())))throw new AssertionError("Kept BIOS damaged: "+name);
                    if(new File(target.getPath()+".part").exists())throw new AssertionError("Partial BIOS kept");
                }
            }
            if(!BiosFiles.hasDreamcast(root))throw new AssertionError("Imported Dreamcast BIOS not usable");
            byte[] boot=Files.readAllBytes(new File(root,"dc/dc_boot.bin").toPath());
            try {
                BiosFiles.install(root,"dc/dc_boot.bin",new InputStream(){public int read()throws IOException{throw new IOException("Provider disappeared");}});
                throw new AssertionError("Lost provider accepted");
            }catch(IOException expected){}
            if(!Arrays.equals(boot,Files.readAllBytes(new File(root,"dc/dc_boot.bin").toPath())))throw new AssertionError("Lost provider damaged BIOS");
            if(!BiosFiles.hasSaturn(root))throw new AssertionError("Imported Saturn BIOS not usable");
            for(String name:new String[]{"../dc_boot.bin","dc/../saturn_bios.bin","Game.bin","dc/dc_flash.bin/extra"}) {
                try {BiosFiles.install(root,name,new ByteArrayInputStream(new byte[0]));throw new AssertionError("Unsafe name accepted");}
                catch(IOException expected){}
            }
            System.out.println("BIOS: Saturn/PS1/Dreamcast sizes, private paths and replacement preservation passed");
        }finally {
            try(java.util.stream.Stream<Path> paths=Files.walk(root.toPath())) {
                paths.sorted(java.util.Comparator.reverseOrder()).forEach(p->{try{Files.delete(p);}catch(IOException e){throw new UncheckedIOException(e);}});
            }
        }
    }
}
