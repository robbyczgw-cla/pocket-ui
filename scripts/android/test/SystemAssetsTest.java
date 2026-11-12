package xyz.pocketui.app;

import java.io.*;
import java.nio.file.*;
import java.util.*;

/** Runs on the host JDK: no Android runtime, emulator or player's files. */
public final class SystemAssetsTest {
    static final class Assets implements SystemAssets.Source {
        final Map<String,byte[]> files=new TreeMap<>();
        boolean fail;
        Assets(){
            for(String name:new String[]{"compat.ini","ppge_atlas.zim","vfpu/table.dat","flash0/font/ltn0.pgf"})
                files.put("system/PPSSPP/"+name,("pinned "+name).getBytes(java.nio.charset.StandardCharsets.UTF_8));
        }
        public String[] list(String path){
            Set<String> children=new TreeSet<>();
            for(String name:files.keySet())if(name.startsWith(path+"/"))children.add(name.substring(path.length()+1).split("/",2)[0]);
            return children.toArray(new String[0]);
        }
        public InputStream open(String path)throws IOException{
            byte[] data=files.get(path);if(data==null)throw new FileNotFoundException(path);
            if(fail)return new InputStream(){int read;public int read()throws IOException{if(read==3)throw new IOException("interrupted");return data[read++];}};
            return new ByteArrayInputStream(data);
        }
    }
    static void require(boolean yes,String why){if(!yes)throw new AssertionError(why);}
    public static void main(String[] args)throws Exception{
        Path root=Files.createTempDirectory("ppsspp-assets-");
        try{
            Assets assets=new Assets();File destination=root.resolve("bios/PPSSPP").toFile();
            assets.fail=true;
            try{SystemAssets.install(assets,"system/PPSSPP",destination);throw new AssertionError("copy should fail");}catch(IOException expected){}
            require(!destination.toPath().resolve("compat.ini").toFile().exists(),"a partial file became visible to the core");
            assets.fail=false;
            require(SystemAssets.install(assets,"system/PPSSPP",destination)==4,"first start did not complete the system folder");
            Path font=destination.toPath().resolve("flash0/font/ltn0.pgf");
            require(Arrays.equals(Files.readAllBytes(font),assets.files.get("system/PPSSPP/flash0/font/ltn0.pgf")),"font not available where PPSSPP reads it");
            Files.write(font,new byte[]{42});
            Files.delete(destination.toPath().resolve("ppge_atlas.zim"));
            require(SystemAssets.install(assets,"system/PPSSPP",destination)==1,"a partial installation was not repaired");
            require(Arrays.equals(Files.readAllBytes(font),new byte[]{42}),"the player's custom font was replaced");
            require(SystemAssets.install(assets,"system/PPSSPP",destination)==0,"second start recopied complete files");
            try(java.util.stream.Stream<Path> paths=Files.walk(root)){
                require(paths.noneMatch(p->p.toString().endsWith(".pocket-part")),"a partial copy was left behind");
            }
            System.out.println("system assets: interrupted copy, retry, fonts, partial-folder repair and existing files ok");
        }finally{
            try(java.util.stream.Stream<Path> paths=Files.walk(root)){
                for(Path p:(Iterable<Path>)paths.sorted(Comparator.reverseOrder())::iterator)Files.delete(p);
            }
        }
    }
}
