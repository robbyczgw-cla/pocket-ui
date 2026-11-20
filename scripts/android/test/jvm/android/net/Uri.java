package android.net;
public final class Uri { private final String value; private Uri(String s){value=s;} public static Uri parse(String s){return new Uri(s);} public String toString(){return value;} public boolean equals(Object o){return o instanceof Uri&&value.equals(o.toString());} public int hashCode(){return value.hashCode();} }
