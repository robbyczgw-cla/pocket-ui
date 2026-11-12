package xyz.pocketui.app;

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
 * Fallback private files: `content/&lt;key&gt;/&lt;name&gt;`. GameDirect first tries a seekable descriptor path;
 * disc sets, unchecked cores and providers that deny reopening or return streams still need these files.
 * A small game is copied at every start. A big one stays, with `.stage` beside it saying what it was copied from, until
 * the document changes or it is one of the oldest.
 */
final class GameCopy {
    /** Up to here a copy is made at every start, and `frozen::WHOLE_UP_TO` reads it whole. Change them together. */
    private static final long COPIED_EVERY_START=64L<<20;
    /** Size cap for the other systems: `document_check::MAX_IN_MEMORY`. Some of their loaders also open paths. */
    private static final long LARGEST_IN_MEMORY=512L<<20;
    /** The cores that open the game's file themselves: `document_library::plays_from_its_file` (PSP, PlayStation). */
    private static final Set<String> PLAYED_FROM_FILE=new HashSet<>(Arrays.asList("ppsspp","pcsx_rearmed","flycast","yabasanshiro"));
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
    interface Words { String tooBig(String size,String most); String noRoom(String size,String free); String full(); String outside(); }
    /** One file of a game of several: its document and its place beside the sheet's copy (`document_library::Part`). */
    static final class Part { final Uri document; final String name; Part(Uri document,String name){this.document=document;this.name=name;} }

    private final Context context;
    private final File content;
    GameCopy(Context context){this.context=context;this.content=new File(context.getFilesDir(),"content");}

    /** The file to play. SecurityException, FileNotFoundException, IllegalArgumentException: the document can't be read. */
    File prepare(Uri document,String key,String name,String partName,String core,Words words,Progress progress) throws IOException {
        long[] now=stamp(document);
        if(!PLAYED_FROM_FILE.contains(core)&&now[0]>LARGEST_IN_MEMORY)throw new IOException(words.tooBig(shown(now[0]),shown(LARGEST_IN_MEMORY)));
        File folder=new File(content,key);
        if(!folder.mkdirs()&&!folder.isDirectory())throw new IOException("Cannot prepare content");
        File rom=new File(folder,name), part=new File(folder,partName), stage=new File(folder,STAGE);
        removeParts(part);
        boolean big=now[0]>COPIED_EVERY_START;
        if(big&&sameAsCopy(document,rom,stage,now)){stage.setLastModified(System.currentTimeMillis());return rom;}
        if(big)makeRoom(now[0],key,()->{stage.delete();rom.delete();},part,words);
        CRC32 whole=new CRC32();
        long total=0;
        try(InputStream in=context.getContentResolver().openInputStream(document);FileOutputStream out=new FileOutputStream(part)) {
            if(in==null)throw new FileNotFoundException("no stream");
            byte[] bytes=new byte[big?1<<20:65536];int n;
            while((n=in.read(bytes))!=-1){
                total+=n;
                if(!PLAYED_FROM_FILE.contains(core)&&total>LARGEST_IN_MEMORY)throw new IOException(words.tooBig(shown(total),shown(LARGEST_IN_MEMORY)));
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

    /**
     * A game of several files (a cue with its tracks, a playlist with its discs): the sheet's copy, its files beside it under
     * the names the sheet gives them. As `prepare` for one file, over all of them: one bar, one room for the whole, one
     * `.stage` ("size time crc" of the whole, then "size time name" for each file), and nothing left of an older copy.
     */
    File prepareSet(Uri document,String name,List<Part> parts,String key,int turn,String core,Words words,Progress progress) throws IOException {
        List<Part> all=new ArrayList<>();
        all.add(new Part(document,name));
        Set<String> names=new HashSet<>();
        for(Part p:parts){
            // The copy's folder holds the game and nothing else: a name that leaves it, or names it twice, isn't followed.
            if(!placeable(p.name)||!names.add(p.name.toLowerCase(Locale.ROOT))||p.name.equalsIgnoreCase(name))throw new IOException(words.outside());
            all.add(p);
        }
        long[][] now=new long[all.size()][];
        long whole=0;
        for(int i=0;i<all.size();i++){now[i]=stamp(all.get(i).document);whole=whole<0||now[i][0]<0?-1:whole+now[i][0];}
        if(!PLAYED_FROM_FILE.contains(core)&&whole>LARGEST_IN_MEMORY)throw new IOException(words.tooBig(shown(whole),shown(LARGEST_IN_MEMORY)));
        File folder=new File(content,key);
        if(!folder.mkdirs()&&!folder.isDirectory())throw new IOException("Cannot prepare content");
        File stage=new File(folder,STAGE), rom=new File(folder,name);
        File[] files=new File[all.size()], partFiles=new File[all.size()];
        for(int i=0;i<all.size();i++){
            files[i]=new File(folder,all.get(i).name);
            File parent=files[i].getParentFile();
            if(!parent.mkdirs()&&!parent.isDirectory())throw new IOException("Cannot prepare content");
            partFiles[i]=new File(parent,files[i].getName()+"."+turn+".part");
        }
        removeParts(new HashSet<>(Arrays.asList(partFiles)));
        boolean big=whole>COPIED_EVERY_START;
        if(big&&sameSetAsCopy(all,files,stage,now,whole)){stage.setLastModified(System.currentTimeMillis());return rom;}
        if(big)makeRoom(whole,key,()->{stage.delete();clear(folder,Collections.emptySet());},partFiles[0],words);
        CRC32 crc=new CRC32();
        long total=0;
        try{
            byte[] bytes=new byte[big?1<<20:65536];
            for(int i=0;i<all.size();i++){
                long own=0;
                try(InputStream in=context.getContentResolver().openInputStream(all.get(i).document);FileOutputStream out=new FileOutputStream(partFiles[i])) {
                    if(in==null)throw new FileNotFoundException("no stream");
                    int n;
                    while((n=in.read(bytes))!=-1){
                        total+=n;own+=n;
                        if(!PLAYED_FROM_FILE.contains(core)&&total>LARGEST_IN_MEMORY)throw new IOException(words.tooBig(shown(total),shown(LARGEST_IN_MEMORY)));
                        out.write(bytes,0,n);crc.update(bytes,0,n);
                        if(!progress.at(total,whole,big||total>COPIED_EVERY_START))throw new Cancelled();
                    }
                    if(own==0)throw new FileNotFoundException("empty: "+all.get(i).name);
                    if(total>COPIED_EVERY_START)out.getFD().sync();
                }
            }
        }catch(IOException|RuntimeException e){
            for(File part:partFiles)part.delete();
            String why=e.getMessage();
            if(why!=null&&(why.contains("ENOSPC")||why.contains("No space left")))throw new IOException(words.full(),e);
            throw e;
        }
        synchronized(progress.lock()){
            if(!progress.current()){for(File part:partFiles)part.delete();throw new Cancelled();}
            // `.stage` first: a kill among the renames leaves a set that is copied again, never one taken for current.
            stage.delete();
            for(int i=0;i<all.size();i++)if(!partFiles[i].renameTo(files[i])){for(File part:partFiles)part.delete();throw new IOException("Cannot finish content copy");}
        }
        // An older copy's files under other names (a track renamed, a disc taken out) don't stay beside this one.
        Set<File> keep=new HashSet<>(Arrays.asList(files));
        keep.add(stage);
        clear(folder,keep);
        if(total>COPIED_EVERY_START){
            StringBuilder lines=new StringBuilder();
            boolean known=true;long newest=0;
            for(int i=0;i<all.size();i++){
                long[] after=stamp(all.get(i).document);
                boolean same=after[0]==files[i].length()&&after[1]>0&&after[1]==now[i][1];
                known&=same;newest=Math.max(newest,after[1]);
                lines.append(files[i].length()).append(' ').append(same?after[1]:0).append(' ').append(all.get(i).name).append('\n');
            }
            writeStage(stage,total+" "+(known?newest:0)+" "+crc.getValue()+"\n"+lines);
            trim(key);
        }
        return rom;
    }
    /** A place inside the copy's folder: plain names between slashes, none of them `.` or `..`. */
    private static boolean placeable(String name) {
        if(name.isEmpty()||name.startsWith("/")||name.contains("\\")||name.contains(":"))return false;
        for(String step:name.split("/",-1))if(step.isEmpty()||step.equals(".")||step.equals("..")||step.endsWith(".part"))return false;
        return true;
    }
    /** The set's copy may be played when each document has its size and time from the copy and its bytes at sixteen places. */
    private boolean sameSetAsCopy(List<Part> all,File[] files,File stage,long[][] now,long whole) throws IOException {
        // Every document is opened, as at any start: a lost grant or a missing track is said, not played round.
        List<ParcelFileDescriptor> open=new ArrayList<>();
        try{
            for(Part p:all){ParcelFileDescriptor file=context.getContentResolver().openFileDescriptor(p.document,"r");if(file==null)throw new FileNotFoundException("no descriptor");open.add(file);}
            if(!stage.isFile())return false;
            List<String> lines;
            try(BufferedReader in=new BufferedReader(new FileReader(stage))){lines=new ArrayList<>();String line;while((line=in.readLine())!=null)lines.add(line);}
            if(lines.size()!=all.size()+1)return false;
            String[] head=lines.get(0).trim().split(" ");
            if(head.length<3||!head[0].equals(Long.toString(whole))||head[1].equals("0"))return false;
            for(int i=0;i<all.size();i++){
                String[] was=lines.get(i+1).split(" ",3);
                if(was.length<3||!was[2].equals(all.get(i).name)||!was[0].equals(Long.toString(now[i][0]))||now[i][1]<=0||!was[1].equals(Long.toString(now[i][1])))return false;
                if(!files[i].isFile()||files[i].length()!=now[i][0])return false;
                if(open.get(i).getStatSize()>=0&&!sameSamples(open.get(i),files[i],now[i][0]))return false;
            }
            return true;
        }finally{for(ParcelFileDescriptor file:open)file.close();}
    }
    private static boolean sameSamples(ParcelFileDescriptor file,File rom,long size) throws IOException {
        try(FileInputStream in=new FileInputStream(file.getFileDescriptor());RandomAccessFile copy=new RandomAccessFile(rom,"r")) {
            byte[] a=new byte[SAMPLE], b=new byte[SAMPLE];
            for(int i=0;i<16;i++){
                long at=Math.max(0,(size-SAMPLE)*i/15);
                in.getChannel().position(at);copy.seek(at);
                int n=in.read(a);
                if(n<=0)return size==0;
                copy.readFully(b,0,n);
                if(!Arrays.equals(Arrays.copyOf(a,n),Arrays.copyOf(b,n)))return false;
            }
        }
        return true;
    }
    /** Everything in the folder, its folders too, but `keep` and another start's part. */
    private static void clear(File folder,Set<File> keep) {
        File[] files=folder.listFiles();
        if(files==null)return;
        for(File file:files){
            if(file.isDirectory()){clear(file,keep);if(!keep.contains(file))file.delete();continue;}
            if(!keep.contains(file)&&!file.getName().endsWith(".part"))file.delete();
        }
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
    private static void writeStage(File stage,long size,long time,long crc) throws IOException { writeStage(stage,size+" "+time+" "+crc+"\n"); }
    private static void writeStage(File stage,String text) throws IOException {
        File next=new File(stage.getPath()+".new");
        try(FileOutputStream out=new FileOutputStream(next)){out.write(text.getBytes("UTF-8"));out.getFD().sync();}
        if(!next.renameTo(stage))next.delete();
    }

    /** What a kill or a cancelled start left. Each worker has its own part, and one still in a slow read just writes on to a removed file. */
    private void removeParts(File own) { removeParts(Collections.singleton(own)); }
    private void removeParts(Set<File> own) { removeParts(content,own,0); }
    /** A set's parts lie in its folders too (a playlist's `Disc 1/`). */
    private static void removeParts(File folder,Set<File> own,int depth) {
        File[] files=folder.listFiles();
        if(files==null||depth>4)return;
        for(File file:files){
            if(file.isDirectory())removeParts(file,own,depth+1);
            else if(depth>0&&file.getName().endsWith(".part")&&!own.contains(file))file.delete();
        }
    }

    private long allocatable() {
        try{StorageManager storage=context.getSystemService(StorageManager.class);return storage.getAllocatableBytes(storage.getUuidForPath(context.getFilesDir()));}
        catch(IOException|RuntimeException e){return context.getFilesDir().getUsableSpace();}
    }

    /** Older big copies go when the game needs their room, this game's own last. Android may free other apps' caches for it. */
    private void makeRoom(long size,String key,Runnable dropOwn,File part,Words words) throws IOException {
        long need=size+KEEP_FREE;
        List<File> others=big();
        // Nothing goes when it couldn't be enough anyway: they stay ready to play.
        long could=allocatable();
        for(File folder:others)could+=copySize(folder);
        if(could<need)throw new IOException(words.noRoom(shown(need),shown(Math.max(0,could-KEEP_FREE))));
        for(int i=others.size()-1;i>=0&&allocatable()<need;i--)if(!others.get(i).getName().equals(key))remove(others.get(i));
        if(allocatable()<need)dropOwn.run();
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
        if(files!=null)for(File file:files)size+=file.isDirectory()?copySize(file):file.length();
        return size;
    }
    /** `.stage` first: a kill in between leaves a file that is copied again, never one taken for current. */
    private static void remove(File folder) {
        android.util.Log.i("PocketMvp","copy removed for room: "+folder.getName());
        new File(folder,STAGE).delete();
        clear(folder,Collections.emptySet());
    }
    static String shown(long bytes) {
        return bytes>=1L<<30?String.format(Locale.ROOT,"%.1f GB",bytes/(double)(1L<<30)):(bytes>>20)+" MB";
    }
}
