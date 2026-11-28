/*
 * From RikkaApps/Shizuku-API (provider/src/main/java/moe/shizuku/api/BinderContainer.java, commit 5ab1d6f8), MIT licence,
 * Copyright (c) 2021 RikkaW (android/licences/SHIZUKU-API.txt). Shizuku sends its binder in this parcel; the class name is
 * part of the protocol, so it stays as it is. The androidx annotation is left out (no androidx in this build).
 */
package moe.shizuku.api;

import android.os.IBinder;
import android.os.Parcel;
import android.os.Parcelable;

public class BinderContainer implements Parcelable {

    public IBinder binder;

    public BinderContainer(IBinder binder) {
        this.binder = binder;
    }

    @Override
    public int describeContents() {
        return 0;
    }

    @Override
    public void writeToParcel(Parcel dest, int flags) {
        dest.writeStrongBinder(this.binder);
    }

    protected BinderContainer(Parcel in) {
        this.binder = in.readStrongBinder();
    }

    public static final Creator<BinderContainer> CREATOR = new Creator<BinderContainer>() {
        @Override
        public BinderContainer createFromParcel(Parcel source) {
            return new BinderContainer(source);
        }

        @Override
        public BinderContainer[] newArray(int size) {
            return new BinderContainer[size];
        }
    };
}
