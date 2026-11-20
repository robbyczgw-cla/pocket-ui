package android.os.storage;import java.io.*;import java.util.UUID;
public class StorageManager{public long getAllocatableBytes(UUID id)throws IOException{return 1L<<40;}public UUID getUuidForPath(File f)throws IOException{return new UUID(0,0);}public void allocateBytes(FileDescriptor d,long n)throws IOException{throw new UnsupportedOperationException("Android storage allocation");}}
