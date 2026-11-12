package xyz.pocketui.app;

/** Routing regression: a readable single-file game must get a direct attempt, regardless of size. */
public final class GameDirectTest {
    static void require(boolean value,String message){if(!value)throw new AssertionError(message);}
    static byte[] header(int version){
        int length=version==3?120:version==4?108:124;
        byte[] bytes=new byte[length];System.arraycopy(new byte[]{'M','C','o','m','p','r','H','D'},0,bytes,0,8);
        java.nio.ByteBuffer.wrap(bytes).putInt(8,length).putInt(12,version);return bytes;
    }
    public static void main(String[] args){
        String[][] cartridges={{"mgba","Game.gba"},{"gambatte","Game.gbc"},{"nestopia","Game.nes"},{"bsnes","Game.sfc"},{"gearsystem","Game.sms"},{"clownmdemu","Game.bin"},{"mednafen_wswan","Game.wsc"},{"stella","Game.a26"},{"mednafen_pce_fast","Game.pce"},{"mednafen_ngp","Game.ngp"},{"handy","Game.lnx"},{"melondsds","Game.nds"},{"mupen64plus_next","Game.z64"},{"ppsspp","Game.ISO"},{"pcsx_rearmed","Game.pbp"}};
        for(String[] game:cartridges)require(GameDirect.policy(game[0],game[1],false)==null,"unnecessary copy: "+game[0]+" "+game[1]);
        for(String suffix:new String[]{"cue","M3U","ccd","mds","gdi"})require(GameDirect.policy("pcsx_rearmed","Game."+suffix,false)!=null,"sibling-dependent format has no safe path plan: "+suffix);
        require(GameDirect.policy("ppsspp","Standalone.chd",false)==null,"standalone CHD must get a descriptor attempt");
        require(GameDirect.policy("ppsspp","Game.iso",true)!=null,"a known dependency must not disappear");
        require(GameDirect.policy("unchecked_core","Game.bin",false)!=null,"an unchecked loader needs the safe fallback");
        for(int version:new int[]{3,4,5}){
            byte[] bytes=header(version);require(GameDirect.chdReason(bytes)==null,"standalone CHD v"+version+" was copied");
            bytes[version==3?100:version==4?68:104]=1;
            require(GameDirect.chdReason(bytes)!=null,"CHD parent was ignored for v"+version);
        }
        byte[] v3=header(3);v3[60]=1;require(GameDirect.chdReason(v3)!=null,"v3 parent MD5 was ignored");
        byte[] v4=header(4);v4[19]=1;require(GameDirect.chdReason(v4)!=null,"parent flag was ignored");
        require(GameDirect.chdReason(new byte[4])!=null,"truncated header was trusted");
        byte[] future=header(6);require(GameDirect.chdReason(future)!=null,"unknown CHD version was trusted");
        System.out.println("PASS: 15 pinned loaders attempt direct single-file access, including standalone CHD; dependencies retain a reasoned fallback");
    }
}
