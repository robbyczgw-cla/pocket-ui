package xyz.pocketui.app;

import java.io.*;
import java.net.*;

/**
 * RetroAchievements' HTTPS through Android's certificate store (the Rust side, `achievements::http`, asks for a request
 * and gets one answer). Worker threads only. HTTPS only, also across redirects; short timeouts so a missing network
 * never hangs a game; the POST body (a password at sign-in, else the sign-in key) is never logged or written to a file.
 */
final class RaHttp {
    private static final int CONNECT_MS = 8000, READ_MS = 25000, REQUEST_LIMIT = 2 * 1024 * 1024, IMAGE_LIMIT = 256 * 1024;

    private RaHttp() {}

    /** "OK\n<status>\n<body>" when the server answered (any status), else "ERR\n<why>". */
    static String request(String address, String post, String contentType, String userAgent) {
        try {
            URL url = new URL(address);
            for (int hop = 0; hop < 4; hop++) {
                if (!"https".equals(url.getProtocol())) return "ERR\nHTTPS required";
                HttpURLConnection c = (HttpURLConnection) url.openConnection();
                c.setConnectTimeout(CONNECT_MS);
                c.setReadTimeout(READ_MS);
                c.setInstanceFollowRedirects(false);
                c.setRequestProperty("User-Agent", userAgent);
                try {
                    // A redirect after a POST is followed as a GET: the body (a password) only ever goes to the first host.
                    if (hop == 0 && !post.isEmpty()) {
                        c.setDoOutput(true);
                        c.setRequestMethod("POST");
                        c.setRequestProperty("Content-Type", contentType.isEmpty() ? "application/x-www-form-urlencoded" : contentType);
                        byte[] body = post.getBytes("UTF-8");
                        c.setFixedLengthStreamingMode(body.length);
                        try (OutputStream out = c.getOutputStream()) { out.write(body); }
                    }
                    int status = c.getResponseCode();
                    if (status >= 300 && status < 400) {
                        String location = c.getHeaderField("Location");
                        if (location == null) return "ERR\nredirect without a destination";
                        url = new URL(url, location);
                        continue;
                    }
                    InputStream in = status >= 400 ? c.getErrorStream() : c.getInputStream();
                    ByteArrayOutputStream text = new ByteArrayOutputStream();
                    if (in != null) {
                        try (InputStream stream = in) {
                            byte[] buffer = new byte[8192];
                            int n;
                            while ((n = stream.read(buffer)) != -1) {
                                if (text.size() + n > REQUEST_LIMIT) return "ERR\nanswer too large";
                                text.write(buffer, 0, n);
                            }
                        }
                    }
                    return "OK\n" + status + "\n" + text.toString("UTF-8");
                } finally { c.disconnect(); }
            }
            return "ERR\ntoo many redirects";
        } catch (IOException | RuntimeException e) {
            return "ERR\n" + e.getClass().getSimpleName();
        }
    }

    /** An achievement's badge into `destination` (a file of the caller's own): "OK\n" or "ERR\n<why>". */
    static String download(String address, String destination, String userAgent) {
        try {
            URL url = new URL(address);
            for (int hop = 0; hop < 4; hop++) {
                if (!"https".equals(url.getProtocol())) return "ERR\nHTTPS required";
                HttpURLConnection c = (HttpURLConnection) url.openConnection();
                c.setConnectTimeout(CONNECT_MS);
                c.setReadTimeout(READ_MS);
                c.setInstanceFollowRedirects(false);
                c.setRequestProperty("User-Agent", userAgent);
                try {
                    int status = c.getResponseCode();
                    if (status >= 300 && status < 400) {
                        String location = c.getHeaderField("Location");
                        if (location == null) return "ERR\nredirect without a destination";
                        url = new URL(url, location);
                        continue;
                    }
                    if (status != 200) return "ERR\nHTTP " + status;
                    long total = 0;
                    try (InputStream in = c.getInputStream(); OutputStream out = new FileOutputStream(destination)) {
                        byte[] buffer = new byte[8192];
                        int n;
                        while ((n = in.read(buffer)) != -1) {
                            if ((total += n) > IMAGE_LIMIT) return "ERR\nimage too large";
                            out.write(buffer, 0, n);
                        }
                    }
                    return "OK\n";
                } finally { c.disconnect(); }
            }
            return "ERR\ntoo many redirects";
        } catch (IOException | RuntimeException e) {
            return "ERR\n" + e.getClass().getSimpleName();
        }
    }
}
