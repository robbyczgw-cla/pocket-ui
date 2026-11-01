package dev.pocket.spike;

import android.content.Context;
import android.database.Cursor;
import android.net.Uri;
import android.os.ParcelFileDescriptor;
import android.os.storage.StorageManager;
import android.provider.DocumentsContract;
import java.io.*;
import java.util.*;
import java.util.zip.CRC32;

/**
 * The private file a core plays: `content/&lt;key&gt;/&lt;name&gt;`, copied from the player's document. A core can't read a document:
 * PPSSPP opens a path, and a provider's descriptor can't be opened again by path (docs/ANDROID-MVP.md, "Big games").
 * A small game is copied at every start. A big one stays, with `.stage` beside it saying what it was copied from, until
 * the document changes or it is one of the oldest.
 */
final class GameCopy {
    /** Up to here a copy is made at every start, and `frozen::WHOLE_UP_TO` reads it whole. Change them together. */
    private static final long COPIED_EVERY_START=64L<<20;
    /** Every core but PPSSPP is given the whole game in memory: `document_check::MAX_IN_MEMORY`. Change them together. */
    private static final long LARGEST_IN_MEMORY=512L<<20;
    /** What a big copy leaves free for saves and moments: `document_check::KEEP_FREE`. Change them together. */
    private static final long KEEP_FREE=256L<<20;
    /** The big copies kept: `frozen::MOST`, so that the games standing open start without a wait. */
    private static final int KEPT=5;
    private static final String STAGE=".stage";
    private static final int SAMPLE=4096;

    /** Cancel was pressed: the part is gone and nothing else changed. */
    static final class Cancelled extends IOException {}
    /** Asked at every block, `big` from the size on where a window is due; false stops the copy. The rename to the real name happens holding `lock` while `current`. */
    interface Progress { boolean at(long done,long total,boolean big); boolean current(); Object lock(); }
    /** The sentences for a person, in the player's language. */
    interface Words { String tooBig(String size,String most); String noRoom(String size,String free); String full(); }

    private final Context context;
    private final File content;
    GameCopy(Context context){this.context=context;this.content=new File(context.getFilesDir(),"content");}

    /** The file to play. SecurityException, FileNotFoundException, IllegalArgumentException: the document can't be read. */
    File prepare(Uri document,String key,String name,String partName,String core,Words words,Progress progress) throws IOException {
        long[] now=stamp(document);
        if(!core.equals("ppsspp")&&now[0]>LARGEST_IN_MEMORY)throw new IOException(words.tooBig(shown(now[0]),shown(LARGEST_IN_MEMORY)));
        File folder=new File(content,key);
        if(!folder.mkdirs()&&!folder.isDirectory())throw new IOException("Cannot prepare content");
        File rom=new File(folder,name), part=new File(folder,partName), stage=new File(folder,STAGE);
        removeParts(part);
        boolean big=now[0]>COPIED_EVERY_START;
        if(big&&sameAsCopy(document,rom,stage,now)){stage.setLastModified(System.currentTimeMillis());return rom;}
        if(big)makeRoom(now[0],key,rom,stage,part,words);
        CRC32 whole=new CRC32();
        long total=0;
        try(InputStream in=context.getContentResolver().openInputStream(document);FileOutputStream out=new FileOutputStream(part)) {
            if(in==null)throw new FileNotFoundException("no stream");
            byte[] bytes=new byte[big?1<<20:65536];int n;
            while((n=in.read(bytes))!=-1){
                total+=n;
                if(!core.equals("ppsspp")&&total>LARGEST_IN_MEMORY)throw new IOException(words.tooBig(shown(total),shown(LARGEST_IN_MEMORY)));
                out.write(bytes,0,n);whole.update(bytes,0,n);
                if(!progress.at(total,now[0],big||total>COPIED_EVERY_START))throw new Cancelled();
            }
            if(total==0)throw new FileNotFoundException("empty");
            if(total>COPIED_EVERY_START)out.getFD().sync();
        }catch(IOException|RuntimeException e){
            part.delete();
            String why=e.getMessage();
            if(why!=null&&(why.contains("ENOSPC")||why.contains("No space left")))throw new IOException(words.full(),e);
            throw e;
        }
        synchronized(progress.lock()){
            if(!progress.current()){part.delete();throw new Cancelled();}
            stage.delete();
            if(!part.renameTo(rom)){part.delete();throw new IOException("Cannot finish content copy");}
        }
        if(total>COPIED_EVERY_START){
            long[] after=stamp(document);
            // The crc is what the copy holds, always true. Time 0 (the provider says none, or the file changed under the copy): nothing to know the document by, so the next start copies again.
            boolean known=after[0]==total&&after[1]>0&&after[1]==now[1];
            writeStage(stage,total,known?after[1]:0,whole.getValue());
            trim(key);
        }
        return rom;
    }

    /** Room a first copy of this game would have, in bytes; -1: it has a copy, it needs none. */
    long room(String key) {
        if(new File(new File(content,key),STAGE).isFile())return -1;
        long room=allocatable();
        for(File folder:big())room+=copySize(folder);
        return room;
    }

    /** The provider's size and time; -1 and 0 when it doesn't say. Throws as `prepare` says when the document is out of reach. */
    private long[] stamp(Uri document) {
        long[] found={-1,0};
        try(Cursor c=context.getContentResolver().query(document,new String[]{DocumentsContract.Document.COLUMN_SIZE,DocumentsContract.Document.COLUMN_LAST_MODIFIED},null,null,null)) {
            if(c!=null&&c.moveToFirst()){if(!c.isNull(0))found[0]=c.getLong(0);if(!c.isNull(1))found[1]=c.getLong(1);}
        }
        return found;
    }

    /**
     * The copy may be played when the document has its size and time and what it holds at sixteen places. Opening it also
     * asks Android: a lost grant or a missing file is said as at any other start, the copy isn't played behind its back.
     */
    private boolean sameAsCopy(Uri document,File rom,File stage,long[] now) throws IOException {
        try(ParcelFileDescriptor file=context.getContentResolver().openFileDescriptor(document,"r")) {
            if(file==null)throw new FileNotFoundException("no descriptor");
            if(!rom.isFile()||rom.length()!=now[0]||now[1]<=0||!stage.isFile())return false;
            String[] was=readStage(stage);
            if(was.length<3||!was[0].equals(Long.toString(now[0]))||!was[1].equals(Long.toString(now[1])))return false;
            if(file.getStatSize()<0)return true; // A cloud provider's pipe: its size and time are all there is, and a download is worse.
            try(FileInputStream in=new FileInputStream(file.getFileDescriptor());RandomAccessFile copy=new RandomAccessFile(rom,"r")) {
                byte[] a=new byte[SAMPLE], b=new byte[SAMPLE];
                for(int i=0;i<16;i++){
                    long at=(now[0]-SAMPLE)*i/15;
                    in.getChannel().position(at);copy.seek(at);
                    int n=in.read(a);
                    if(n<=0)return false;
                    copy.readFully(b,0,n);
                    if(!Arrays.equals(Arrays.copyOf(a,n),Arrays.copyOf(b,n)))return false;
                }
            }
            return true;
        }
    }

    private static String[] readStage(File stage) throws IOException {
        try(BufferedReader in=new BufferedReader(new FileReader(stage))){String line=in.readLine();return line==null?new String[0]:line.trim().split(" ");}
    }
    /** "size time crc": `frozen::mix_file` hashes the crc, which is the whole file's, so a patch between samples still counts. */
    private static void writeStage(File stage,long size,long time,long crc) throws IOException {
        File next=new File(stage.getPath()+".new");
        try(FileOutputStream out=new FileOutputStream(next)){out.write((size+" "+time+" "+crc+"\n").getBytes("UTF-8"));out.getFD().sync();}
        if(!next.renameTo(stage))next.delete();
    }

    /** What a kill or a cancelled start left. Each worker has its own part, and one still in a slow read just writes on to a removed file. */
    private void removeParts(File own) {
        File[] folders=content.listFiles();
        if(folders==null)return;
        for(File folder:folders){
            File[] files=folder.listFiles((d,n)->n.endsWith(".part")&&!n.equals(own.getName()));
            if(files!=null)for(File part:files)part.delete();
        }
    }

    private long allocatable() {
        try{StorageManager storage=context.getSystemService(StorageManager.class);return storage.getAllocatableBytes(storage.getUuidForPath(context.getFilesDir()));}
        catch(IOException|RuntimeException e){return context.getFilesDir().getUsableSpace();}
    }

    /** Older big copies go when the game needs their room, this game's own last. Android may free other apps' caches for it. */
    private void makeRoom(long size,String key,File rom,File stage,File part,Words words) throws IOException {
        long need=size+KEEP_FREE;
        List<File> others=big();
        // Nothing goes when it couldn't be enough anyway: they stay ready to play.
        long could=allocatable();
        for(File folder:others)could+=copySize(folder);
        if(could<need)throw new IOException(words.noRoom(shown(need),shown(Math.max(0,could-KEEP_FREE))));
        for(int i=others.size()-1;i>=0&&allocatable()<need;i--)if(!others.get(i).getName().equals(key))remove(others.get(i));
        if(allocatable()<need){stage.delete();rom.delete();}
        long free=allocatable();
        if(free<need)throw new IOException(words.noRoom(shown(need),shown(Math.max(0,free-KEEP_FREE))));
        // Android's way of asking for the room now, so that a phone that fills up meanwhile says so here, not at 90 %.
        try(FileOutputStream out=new FileOutputStream(part)){
            StorageManager storage=context.getSystemService(StorageManager.class);
            storage.allocateBytes(out.getFD(),size);
        }catch(IOException e){part.delete();throw new IOException(words.noRoom(shown(need),shown(Math.max(0,free-KEEP_FREE))),e);}
    }

    /** The big copies, the most recently played first. */
    private List<File> big() {
        List<File> found=new ArrayList<>();
        File[] folders=content.listFiles();
        if(folders!=null)for(File folder:folders)if(new File(folder,STAGE).isFile())found.add(folder);
        Map<File,Long> played=new HashMap<>();
        for(File folder:found)played.put(folder,new File(folder,STAGE).lastModified());
        found.sort((a,b)->Long.compare(played.get(b),played.get(a)));
        return found;
    }
    private void trim(String key) {
        List<File> kept=big();
        for(int i=kept.size()-1;i>=KEPT;i--)if(!kept.get(i).getName().equals(key))remove(kept.get(i));
    }
    private static long copySize(File folder) {
        long size=0;
        File[] files=folder.listFiles();
        if(files!=null)for(File file:files)size+=file.length();
        return size;
    }
    /** `.stage` first: a kill in between leaves a file that is copied again, never one taken for current. */
    private static void remove(File folder) {
        android.util.Log.i("PocketMvp","copy removed for room: "+folder.getName());
        new File(folder,STAGE).delete();
        File[] files=folder.listFiles();
        if(files!=null)for(File file:files)file.delete();
    }
    static String shown(long bytes) {
        return bytes>=1L<<30?String.format(Locale.ROOT,"%.1f GB",bytes/(double)(1L<<30)):(bytes>>20)+" MB";
    }
}
