package xyz.pocketui.app;

import android.content.Context;
import android.net.Uri;
import android.os.ParcelFileDescriptor;
import java.io.FileInputStream;
import java.io.IOException;
import java.util.Arrays;
import java.util.HashSet;
import java.util.Locale;
import java.util.Set;

/** Capability-checked SAF access. No ROM bytes are written by this path. */
final class GameDirect {
    // These pinned loaders (bundled or optional) accept a path with its original suffix. See ANDROID-MVP.md, "Direct documents".
    private static final Set<String> CORES=new HashSet<>(Arrays.asList("mgba","gambatte","nestopia","bsnes","gearsystem","clownmdemu","mednafen_wswan","stella","mednafen_pce_fast","mednafen_ngp","handy","ppsspp","melondsds","mupen64plus_next","pcsx_rearmed"));
    static String policy(String core,String name,boolean parts){
        if(!CORES.contains(core))return "core has not been checked for descriptor paths";
        String suffix=name.substring(name.lastIndexOf('.')+1).toLowerCase(Locale.ROOT);
        if(parts||Arrays.asList("cue","m3u","gdi","ccd","mds","mdf","toc","m3u8").contains(suffix))return "disc sets need sibling paths";
        return null;
    }
    private static int be32(byte[] bytes,int at){return java.nio.ByteBuffer.wrap(bytes,at,4).getInt();}
    private static boolean nonzero(byte[] bytes,int from,int to){for(int i=from;i<to;i++)if(bytes[i]!=0)return true;return false;}
    /** Pinned libchdr/chd.h layouts: a standalone CHD is one seekable file; a parent needs sibling lookup. */
    static String chdReason(byte[] header){
        if(header.length<16||!Arrays.equals(Arrays.copyOf(header,8),new byte[]{'M','C','o','m','p','r','H','D'}))return "CHD header cannot be checked";
        int version=be32(header,12),length=be32(header,8);
        int expected=version==3?120:version==4?108:version==5?124:0;
        if(expected==0||length!=expected||header.length<expected)return "CHD header version has not been checked";
        boolean parent=version==5?nonzero(header,104,124):(be32(header,16)&1)!=0;
        if(version==3)parent|=nonzero(header,60,76)||nonzero(header,100,120);
        if(version==4)parent|=nonzero(header,68,88);
        return parent?"CHD parent needs sibling paths":null;
    }
    /** null means direct; otherwise a concrete copy reason. A pipe or FUSE reopen denial needs the existing stream copy. */
    static String probe(Context context,Uri document,String core,String name,boolean parts){
        String reason=policy(core,name,parts);if(reason!=null)return reason;
        try(ParcelFileDescriptor fd=context.getContentResolver().openFileDescriptor(document,"r")){
            if(fd==null)return "provider returned no descriptor";
            try(FileInputStream in=new FileInputStream("/proc/self/fd/"+fd.getFd())){
                long size=in.getChannel().size();
                if(size<=0)return "provider has no regular seekable file";
                in.getChannel().position(size-1);
                if(in.read()<0)return "provider cannot seek to the last byte";
                in.getChannel().position(0);
                if(in.read()<0)return "provider cannot rewind";
                if(name.toLowerCase(Locale.ROOT).endsWith(".chd")){
                    in.getChannel().position(0);
                    byte[] header=new byte[124];int length=0,n;
                    while(length<header.length&&(n=in.read(header,length,header.length-length))>0)length+=n;
                    reason=chdReason(Arrays.copyOf(header,length));if(reason!=null)return reason;
                }
            }
            return null;
        }catch(IOException|RuntimeException e){return "provider cannot reopen and seek a descriptor: "+e.getClass().getSimpleName();}
    }
}
