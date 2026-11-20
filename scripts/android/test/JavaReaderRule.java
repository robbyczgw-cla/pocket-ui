package xyz.pocketui.app;

import android.app.*;
import android.content.*;
import android.content.pm.*;
import android.database.Cursor;
import android.net.*;
import android.os.*;
import android.provider.DocumentsContract;
import java.io.*;
import java.lang.reflect.*;
import java.net.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.security.MessageDigest;
import java.util.*;
import java.util.zip.*;
import org.json.*;

/** Real Java production readers, real temporary files, the shared Rust fault corpus. */
public final class JavaReaderRule {
    static final String[] SOURCES={"views-handheld","views-mobile","game-stage","game-set-stage","game-copy-stream","game-copy-set-stream","direct-chd3","direct-chd4","direct-chd5","backup-zip","backup-export-source","save-copy-manifest","save-copy-file","save-copy-read-error","save-copy-put","update-notice","update-download","update-read","update-apk-stream"};
    static final byte[] DATA="Reader rule saved bytes".getBytes(StandardCharsets.UTF_8);
    static final byte[] OLD="Kept older data".getBytes(StandardCharsets.UTF_8);
    static final long LARGE=(64L<<20)+1,TIME=1700000000000L;
    static final String GAME="0123456789abcdef",ENTRY="states/"+GAME+"/Auto.pstate";
    static void require(boolean ok,String why){if(!ok)throw new AssertionError(why);}
    static void put(File f,byte[] b)throws IOException{File p=f.getParentFile();if(p!=null)p.mkdirs();Files.write(f.toPath(),b);}
    static byte[] read(File f)throws IOException{return Files.readAllBytes(f.toPath());}
    static byte[] utf8(String s){return s.getBytes(StandardCharsets.UTF_8);}
    static Uri uri(File f){return Uri.parse("file:"+f.getAbsolutePath());}
    static File file(Uri u){return new File(DocumentsContract.path(u));}
    static Object invoke(Object o,String name,Class<?>[] types,Object...args)throws Exception{
        Method m=o.getClass().getDeclaredMethod(name,types);m.setAccessible(true);
        try{return m.invoke(o,args);}catch(InvocationTargetException e){Throwable t=e.getCause();if(t instanceof Exception)throw(Exception)t;throw(Error)t;}
    }
    static void field(Object o,String name,Object value)throws Exception{Field f=o.getClass().getDeclaredField(name);f.setAccessible(true);f.set(o,value);}
    static byte[] zip()throws Exception{
        ByteArrayOutputStream bytes=new ByteArrayOutputStream();
        try(ZipOutputStream out=new ZipOutputStream(bytes)){
            for(String name:new String[]{SaveBackup.MANIFEST,ENTRY}){
                ZipEntry e=new ZipEntry(name);e.setTime(0);out.putNextEntry(e);
                out.write(name.equals(ENTRY)?DATA:utf8("{\"format\":1,\"app\":\"pocket-ui\",\"games\":[],\"files\":{}}"));out.closeEntry();
            }
        }return bytes.toByteArray();
    }
    static byte[] seed(String source)throws Exception{
        if(source.equals("views-handheld"))return utf8("handheld");
        if(source.equals("views-mobile"))return utf8("mobile");
        if(source.equals("game-stage"))return utf8(LARGE+" "+TIME+" 1234567\n");
        if(source.equals("game-set-stage"))return utf8("12000 "+TIME+" 1234567\n5000 "+TIME+" Game.cue\n7000 "+TIME+" track.bin\n");
        if(source.startsWith("direct-chd"))return GameDirectTest.header(Integer.parseInt(source.substring(source.length()-1)));
        if(source.equals("backup-zip"))return zip();
        if(source.equals("save-copy-manifest"))return utf8("{\"format\":1,\"game\":\"Reader\",\"files\":{\"save.srm\":{\"size\":23}}}");
        if(source.equals("update-notice"))return utf8("9.9.9");
        if(source.equals("update-read"))return utf8("{\"format\":1,\"app\":null,\"os\":null}");
        return DATA;
    }
    public static void main(String[] args)throws Exception{
        if(args[0].equals("--sources")){for(String s:SOURCES)System.out.println(s);return;}
        if(args[0].equals("--seeds")){File d=new File(args[1]);d.mkdirs();for(String s:SOURCES)put(new File(d,s),seed(s));return;}
        System.out.println("READER_READY");System.out.flush();
        String source=args[1],label=args[3];File input=new File("input");
        byte[] bytes=label.equals("missing")?null:Files.readAllBytes(Paths.get(args[2]));if(bytes!=null)put(input,bytes);
        boolean valid=label.equals("valid");ContextFixture c=new ContextFixture(new File("fixture"));
        if(source.startsWith("views-")){
            android.view.InputDevice.fixtureGamepad=source.equals("views-mobile");
            if(bytes!=null)put(new File(c.getFilesDir(),"view"),bytes);
            String got=Views.chosen(c);
            require(got.equals("handheld")||got.equals("mobile"),"view has no named fallback");
            if(valid)require(got.equals(source.substring(6)),"valid view was ignored");
        }else if(source.equals("game-stage")||source.equals("game-set-stage")){
            stage(c,input,source,valid,label);
        }else if(source.startsWith("game-copy-")){
            prepare(c,input,source,bytes,valid);
        }else if(source.startsWith("direct-chd")){
            String got=GameDirect.probe(c,uri(input),"ppsspp","Game.chd",false);
            if(valid)require(got==null,"valid standalone CHD was refused: "+got);
            if(got!=null)require(!got.trim().isEmpty(),"descriptor refusal is blank");
        }else if(source.equals("backup-zip")){
            backup(c,input,valid);
        }else if(source.equals("backup-export-source")){
            File output=new File("export.zip");String manifest="{\"format\":1,\"files\":{\""+ENTRY+"\":23}}";
            String plan=new JSONObject().put("manifest",manifest).put("files",new JSONArray().put(new JSONArray().put(ENTRY).put(input.getPath()))).toString();
            String got=SaveBackup.write(c.resolver,uri(output),plan,()->false);
            require(got.equals("exported")||got.equals("export-failed"),"export has no outcome");
            if(bytes==null){require(got.equals("export-failed"),"missing planned save was silently reported exported");require(!output.exists(),"partial backup remained");}
            else{require(got.equals("exported"),"readable bytes not exported");require(Arrays.equals(entry(output,ENTRY),bytes),"export source bytes changed");}
        }else if(source.startsWith("save-copy-")){
            copy(c,input,source,bytes,valid);
        }else if(source.equals("update-notice")){
            UpdateInstaller u=UpdateInstaller.get(c);if(bytes!=null)put(new File(c.getCacheDir(),"pocket-update-version"),bytes);
            String got=u.takeInstalledVersion();if(valid)require(got.equals("9.9.9"),"valid notice was not consumed");
            require(got.isEmpty()||got.equals("9.9.9"),"uninstalled version was announced");
            if(!got.isEmpty())require(!new File(c.getCacheDir(),"pocket-update-version").exists(),"notice not consumed");
        }else if(source.equals("update-read")){
            c.network.input=input;String got=UpdateInstaller.get(c).read("https://fixture.test/update","1048576");
            require(got.startsWith("OK\n")||got.startsWith("ERR\n"),"manifest read has no outcome");
            if(valid)require(got.equals("OK\n"+new String(bytes,StandardCharsets.UTF_8)),"valid manifest bytes changed");
            if(got.startsWith("ERR\n"))require(got.substring(4).trim().length()>10,"manifest failure has no readable reason");
        }else if(source.equals("update-download")){
            UpdateInstaller u=UpdateInstaller.get(c);File part=new File(c.getCacheDir(),"download.part");c.network.input=input;field(u,"ready",part);
            try{invoke(u,"download",new Class<?>[]{Network.class,String.class,File.class,long.class,String.class},c.network,"https://fixture.test/apk",part,(long)DATA.length,UpdateInstaller.hex(MessageDigest.getInstance("SHA-256").digest(DATA)));
                require(Arrays.equals(read(part),DATA),"unchecked damaged download kept");if(valid)require(part.exists(),"valid download not written");
            }catch(IOException e){require(!valid,"valid payload rejected: "+e);String reason=(String)invoke(u,"reason",new Class<?>[]{Exception.class},e);u.fail(reason);require(reason.length()>10,"download failure blank");require(!part.exists(),"failed download remained ready");}
        }else if(source.equals("update-apk-stream")){
            UpdateInstaller u=UpdateInstaller.get(c);field(u,"ready",input);field(u,"readyVersion","9.9.9");field(u,"total",(long)DATA.length);field(u,"phase","permission");u.permissionResult(c);
            // The external probe watchdog also bounds this worker wait under load.
            while(!c.installer.committed&&u.busy())Thread.sleep(2);
            if(bytes==null){require(!c.installer.committed,"missing payload committed");require(new JSONObject(u.status()).getString("message").length()>10,"installer failure blank");}
            else{require(c.installer.committed,"readable payload not streamed");require(Arrays.equals(read(c.installer.out),bytes),"installer stream differs");}
        }else throw new AssertionError("unknown reader "+source);
        System.out.println("PASS "+source+"/"+label);
    }
    static void sparse(File file,long size)throws IOException{File p=file.getParentFile();if(p!=null)p.mkdirs();try(RandomAccessFile f=new RandomAccessFile(file,"rw")){f.setLength(size);}}
    static void stage(ContextFixture c,File input,String source,boolean valid,String label)throws Exception{
        GameCopy copy=new GameCopy(c);boolean got;
        if(source.equals("game-stage")){
            File original=new File("source.iso"),cached=new File("cached.iso");sparse(original,LARGE);sparse(cached,LARGE);
            got=(Boolean)invoke(copy,"sameAsCopy",new Class<?>[]{Uri.class,File.class,File.class,long[].class},uri(original),cached,input,new long[]{LARGE,TIME});
        }else{
            File a=new File("Game.cue"),b=new File("track.bin"),aa=new File("cached/Game.cue"),bb=new File("cached/track.bin");sparse(a,5000);sparse(b,7000);sparse(aa,5000);sparse(bb,7000);
            List<GameCopy.Part> all=Arrays.asList(new GameCopy.Part(uri(a),"Game.cue"),new GameCopy.Part(uri(b),"track.bin"));
            got=(Boolean)invoke(copy,"sameSetAsCopy",new Class<?>[]{List.class,File[].class,File.class,long[][].class,long.class},all,new File[]{aa,bb},input,new long[][]{{5000,TIME},{7000,TIME}},12000L);
        }
        if(valid)require(got,"valid copy identity never reached sample reader");
        if(label.equals("missing")||label.equals("empty")||label.equals("encoding"))require(!got,"unreadable copy identity was trusted");
    }
    static void prepare(ContextFixture c,File input,String source,byte[] bytes,boolean valid)throws Exception{
        GameCopy copy=new GameCopy(c);File cached=new File(c.getFilesDir(),"content/rule/Game.gb");put(cached,OLD);
        GameCopy.Words words=new GameCopy.Words(){public String tooBig(String a,String b){return "Game is too large";}public String noRoom(String a,String b){return "Not enough room";}public String full(){return "Storage is full";}public String outside(){return "Game part is outside its folder";}};
        GameCopy.Progress progress=new GameCopy.Progress(){final Object lock=new Object();public boolean at(long a,long b,boolean big){return true;}public boolean current(){return true;}public Object lock(){return lock;}};
        try{
            File got;
            if(source.equals("game-copy-set-stream")){File sheet=new File("sheet.cue");put(sheet,utf8("FILE track.bin BINARY\n"));got=copy.prepareSet(uri(sheet),"Game.gb",Arrays.asList(new GameCopy.Part(uri(input),"track.bin")),"rule",1,"pcsx_rearmed",words,progress);if(bytes!=null)require(Arrays.equals(read(new File(got.getParentFile(),"track.bin")),bytes),"disc part bytes changed");}
            else{got=copy.prepare(uri(input),"rule","Game.gb","Game.gb.1.part","gambatte",words,progress);if(bytes!=null)require(Arrays.equals(read(got),bytes),"copied ROM bytes changed");}
            require(bytes!=null&&bytes.length>0,"unreadable ROM silently copied");if(valid)require(got.isFile(),"valid copy not kept");
        }catch(IOException e){require(!valid,"valid game copy failed: "+e);require(e.getMessage()!=null&&!e.getMessage().trim().isEmpty(),"copy refusal has no reason");require(Arrays.equals(read(cached),OLD),"failed copy damaged prior game");}
    }
    static byte[] entry(File archive,String name)throws Exception{try(ZipInputStream in=new ZipInputStream(new FileInputStream(archive))){for(ZipEntry e;(e=in.getNextEntry())!=null;)if(e.getName().equals(name)){ByteArrayOutputStream b=new ByteArrayOutputStream();byte[] block=new byte[1024];for(int n;(n=in.read(block))>0;)b.write(block,0,n);return b.toByteArray();}}return null;}
    static void backup(ContextFixture c,File input,boolean valid)throws Exception{
        String listed=SaveBackup.list(c.resolver,uri(input));require(listed.startsWith("listed\n")||listed.equals("import-failed"),"backup listing has no outcome");
        if(valid){JSONObject got=new JSONObject(listed.substring(7));require(got.getJSONArray("names").getString(0).equals(ENTRY),"valid ZIP was not read");require(got.getString("manifest").contains("pocket-ui"),"manifest bytes absent");}
        File saves=new File("restore");File kept=new File(saves,ENTRY);put(kept,OLD);
        JSONObject item=new JSONObject().put("entry",ENTRY).put("to",ENTRY).put("written",1700000000000L);
        String plan=new JSONObject().put("take",new JSONArray().put(item)).put("remove",new JSONArray()).put("place",new JSONArray().put("states/"+GAME)).toString();
        String got=SaveBackup.apply(c.resolver,uri(input),saves,plan,()->false);
        require(Arrays.asList("imported","import-failed","no-room").contains(got),"backup import has no outcome");
        if(valid)require(got.equals("imported"),"valid backup was not restored");
        require(Arrays.equals(read(kept),got.equals("imported")?DATA:OLD),"failed backup import damaged old save");
        require(!new File(saves,"import.part").exists(),"staging directory leaked");
    }
    static void copy(ContextFixture c,File input,String source,byte[] bytes,boolean valid)throws Exception{
        File tree=new File("tree"),dir=new File(tree,SaveCopy.FOLDER+"/Reader");dir.mkdirs();
        c.resolver.tree=tree;SaveCopy copy=new SaveCopy(c.resolver,"tree:"+tree.getAbsolutePath());
        if(source.equals("save-copy-manifest")){
            if(bytes!=null)put(new File(dir,"copy.json"),bytes);
            String got=copy.manifest("Reader");require(got.equals("ok")||got.startsWith("ok\n")||got.startsWith("error\n")||got.equals("gone")||got.equals("no-access"),"manifest has no outcome");
            if(valid){require(got.equals("ok\n"+new String(bytes,StandardCharsets.UTF_8)),"valid copy manifest was ignored");require(copy.games().contains("Reader"),"copy directory not enumerated");}
        }else if(source.equals("save-copy-put")){
            File remote=new File(dir,"saves/save.srm");put(remote,OLD);
            String got=copy.put("Reader","saves/save.srm",input.getPath());
            if(bytes==null){require(!got.equals("ok"),"missing local save reported copied");require(Arrays.equals(read(remote),OLD),"missing source destroyed remote copy");}
            else{require(got.equals("ok"),"readable local save refused: "+got);require(Arrays.equals(read(remote),bytes),"remote copy differs from local bytes");}
            require(SaveCopy.state(c.resolver,"tree:"+tree.getAbsolutePath()).equals("ok"),"writable fixture tree has no named state");
        }else{
            File remote=new File(dir,"saves/save.srm"),local=new File("local.srm");if(bytes!=null)put(remote,bytes);put(local,OLD);
            String got=copy.get("Reader","saves/save.srm",local.getPath());
            if(bytes==null){require(!got.equals("ok"),"missing save reported restored");require(Arrays.equals(read(local),OLD),"missing save damaged local file");}
            else{require(got.equals("ok"),"readable save refused: "+got);require(Arrays.equals(read(local),bytes),"copied save bytes changed");}
            if(valid){put(local,OLD);if(source.equals("save-copy-read-error"))c.resolver.failingStream=remote.getAbsoluteFile();else c.resolver.nullStream=remote.getAbsoluteFile();got=copy.get("Reader","saves/save.srm",local.getPath());require(!got.equals("ok"),"refused provider stream reported success");require(Arrays.equals(read(local),OLD),source.equals("save-copy-read-error")?"partial provider read replaced the existing local save":"provider refusal truncated the existing local save");File[] leaked=local.getAbsoluteFile().getParentFile().listFiles((d,n)->n.startsWith(".pocket-ui-save-")&&n.endsWith(".part"));require(leaked!=null&&leaked.length==0,"partial local replacement leaked");}
        }
    }
    static final class Rows implements Cursor{
        final List<Object[]> rows;int index=-1;Rows(List<Object[]> r){rows=r;}public boolean moveToFirst(){index=0;return !rows.isEmpty();}public boolean moveToNext(){return ++index<rows.size();}public boolean isNull(int i){return rows.get(index)[i]==null;}public long getLong(int i){return ((Number)rows.get(index)[i]).longValue();}public String getString(int i){Object o=rows.get(index)[i];return o==null?null:o.toString();}public void close(){}
    }
    static final class Resolver extends ContentResolver{
        File tree,nullStream,failingStream;
        public InputStream openInputStream(Uri u)throws IOException{
            File f=file(u);if(f.equals(nullStream))return null;
            InputStream in=new FileInputStream(f);if(!f.equals(failingStream))return in;
            return new FilterInputStream(in){int received;public int read(byte[] b,int off,int len)throws IOException{if(received>=5)throw new IOException("Provider stopped after a prefix");int n=super.read(b,off,Math.min(len,5-received));if(n>0)received+=n;return n;}};
        }
        public OutputStream openOutputStream(Uri u,String mode)throws IOException{File f=file(u);if(f.getParentFile()!=null)f.getParentFile().mkdirs();return new FileOutputStream(f);}
        public ParcelFileDescriptor openFileDescriptor(Uri u,String mode)throws IOException{return new ParcelFileDescriptor(file(u));}
        public Cursor query(Uri u,String[] columns,String a,String[] b,String c){
            File f=file(u);List<Object[]> rows=new ArrayList<>();File[] files=u.toString().startsWith("children:")?f.listFiles():new File[]{f};
            if(files!=null)for(File one:files)if(one.exists()){
                Object[] row=new Object[columns.length];for(int i=0;i<columns.length;i++)switch(columns[i]){
                    case "size":row[i]=one.length();break;case "time":row[i]=one.lastModified();break;case "id":row[i]=one.getAbsolutePath();break;case "name":row[i]=one.getName();break;case "mime":row[i]=one.isDirectory()?DocumentsContract.Document.MIME_TYPE_DIR:"binary";break;default:throw new AssertionError(columns[i]);}
                rows.add(row);
            }return new Rows(rows);
        }
        public List<UriPermission> getPersistedUriPermissions(){return tree==null?Collections.emptyList():Arrays.asList(new UriPermission(Uri.parse("tree:"+tree.getAbsolutePath())));}
        public Uri createDocument(Uri parent,String mime,String name)throws IOException{File f=new File(file(parent),name);if(mime.equals(DocumentsContract.Document.MIME_TYPE_DIR))f.mkdirs();else{if(f.getParentFile()!=null)f.getParentFile().mkdirs();f.createNewFile();}return uri(f);}
        public boolean deleteDocument(Uri u)throws IOException{return Files.deleteIfExists(file(u).toPath());}
    }
    static final class FixtureNetwork extends Network{
        File input;
        public URLConnection openConnection(URL u){return new HttpURLConnection(u){public void connect(){}public void disconnect(){}public boolean usingProxy(){return false;}public int getResponseCode(){return input.isFile()?200:404;}public long getContentLengthLong(){return input.length();}public InputStream getInputStream()throws IOException{return new FileInputStream(input);}};}
    }
    static final class Installer extends PackageInstaller{
        final File out;volatile boolean committed;Installer(File root){out=new File(root,"session.apk");}public int createSession(SessionParams p){return 1;}public void abandonSession(int id){}public Session openSession(int id){return new Session(){public OutputStream openWrite(String n,long o,long z)throws IOException{return new FileOutputStream(out);}public void commit(IntentSender sender){committed=true;}};}
    }
    static final class ContextFixture extends Activity{
        final File root;final Resolver resolver=new Resolver();final FixtureNetwork network=new FixtureNetwork();final Installer installer;final PackageManager manager;
        ContextFixture(File r){root=r;root.mkdirs();installer=new Installer(r);manager=new PackageManager(){public PackageInfo getPackageInfo(String n,int f){PackageInfo i=new PackageInfo();i.packageName=getPackageName();i.versionName="9.9.9";return i;}public PackageInstaller getPackageInstaller(){return installer;}};}
        public File getFilesDir(){return root;}public ContentResolver getContentResolver(){return resolver;}public PackageManager getPackageManager(){return manager;}
        public Object getSystemService(String name){if(name.equals(Context.CONNECTIVITY_SERVICE)){android.net.ConnectivityManager m=new android.net.ConnectivityManager();m.fixture=network;return m;}if(name.equals(android.os.storage.StorageManager.class.getName()))return new android.os.storage.StorageManager();throw new UnsupportedOperationException(name);}
    }
}
