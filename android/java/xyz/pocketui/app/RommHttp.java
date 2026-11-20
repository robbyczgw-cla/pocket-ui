package xyz.pocketui.app;

import java.io.*;
import java.net.*;

/**
 * RomM over HTTPS through Android's certificate store (the Rust side, `romm_net::AndroidNet`; plain `http://` at home goes
 * through Rust's own client, as the APK allows no clear text here). Worker threads only. HTTPS only, also across
 * redirects, and the token only ever goes to the host it was given for; it is never logged or written anywhere.
 */
final class RommHttp {
    private static final int CONNECT_MS = 8000, READ_MS = 45000;
    private static final long ANSWER_LIMIT = 96L * 1024 * 1024;

    private RommHttp() {}

    private static HttpURLConnection open(URL url, String method, String token, String userAgent, URL first) throws IOException {
        if (!"https".equals(url.getProtocol())) throw new IOException("HTTPS required");
        HttpURLConnection c = (HttpURLConnection) url.openConnection();
        c.setConnectTimeout(CONNECT_MS);
        c.setReadTimeout(READ_MS);
        c.setInstanceFollowRedirects(false);
        c.setRequestMethod(method);
        c.setRequestProperty("User-Agent", userAgent);
        c.setRequestProperty("Accept", "application/json");
        c.setRequestProperty("Accept-Encoding", "identity");
        boolean sameHost = url.getHost().equalsIgnoreCase(first.getHost()) && url.getPort() == first.getPort();
        if (!token.isEmpty() && sameHost) c.setRequestProperty("Authorization", "Bearer " + token);
        return c;
    }

    /** One request: "OK\n<status>\n<body>" when the server answered (any status), else "ERR\n<why>". */
    static String request(String method, String address, String token, String json, String userAgent) {
        try {
            URL first = new URL(address), url = first;
            for (int hop = 0; hop < 4; hop++) {
                HttpURLConnection c = open(url, hop == 0 ? method : "GET", token, userAgent, first);
                try {
                    if (hop == 0 && !json.isEmpty()) {
                        c.setDoOutput(true);
                        c.setRequestProperty("Content-Type", "application/json");
                        byte[] body = json.getBytes("UTF-8");
                        c.setFixedLengthStreamingMode(body.length);
                        try (OutputStream out = c.getOutputStream()) { out.write(body); }
                    }
                    int status = c.getResponseCode();
                    if (status >= 300 && status < 400 && c.getHeaderField("Location") != null) {
                        url = new URL(url, c.getHeaderField("Location"));
                        continue;
                    }
                    InputStream in = status >= 400 ? c.getErrorStream() : c.getInputStream();
                    ByteArrayOutputStream text = new ByteArrayOutputStream();
                    if (in != null) {
                        try (InputStream stream = in) {
                            byte[] buffer = new byte[16384];
                            int n;
                            while ((n = stream.read(buffer)) != -1) {
                                if (text.size() + n > ANSWER_LIMIT) return "ERR\nanswer too large";
                                text.write(buffer, 0, n);
                            }
                        }
                    }
                    return "OK\n" + status + "\n" + text.toString("UTF-8");
                } finally { c.disconnect(); }
            }
            return "ERR\ntoo many redirects";
        } catch (IOException | RuntimeException e) {
            return "ERR\nunreachable";
        }
    }

    /**
     * Appends the file at `address` to `destination` from its current length (a range request). "OK" when it is whole,
     * "HTTP <status>" for an answer that isn't the file, else "ERR\n" and unreachable, stopped, nospace, write or broke.
     * Stops when `stopFile` appears (B on the Pocket); a transfer that moves under 1 KB in 45 s is given up.
     */
    static String fetch(String address, String token, String destination, String stopFile, String size, String userAgent) {
        File file = new File(destination), stop = new File(stopFile);
        long expected;
        try { expected = Long.parseLong(size); } catch (NumberFormatException e) { expected = 0; }
        long offset = file.length();
        boolean anything = false;
        try {
            URL first = new URL(address), url = first;
            for (int hop = 0; hop < 4; hop++) {
                HttpURLConnection c = open(url, "GET", token, userAgent, first);
                if (offset > 0) c.setRequestProperty("Range", "bytes=" + offset + "-");
                try {
                    int status = c.getResponseCode();
                    anything = true;
                    if (status >= 300 && status < 400 && c.getHeaderField("Location") != null) {
                        url = new URL(url, c.getHeaderField("Location"));
                        continue;
                    }
                    boolean append = status == 206 && offset > 0;
                    if (status != 200 && !append) return "HTTP " + status;
                    try (InputStream in = c.getInputStream(); FileOutputStream out = new FileOutputStream(file, append)) {
                        byte[] buffer = new byte[65536];
                        long total = append ? offset : 0, movedAt = System.nanoTime(), movedBytes = total;
                        int n;
                        while ((n = in.read(buffer)) != -1) {
                            if (stop.exists()) return "ERR\nstopped";
                            if (expected > 0 && total + n > expected) return "ERR\nbroke";
                            try { out.write(buffer, 0, n); }
                            catch (IOException e) { return file.getUsableSpace() < 4L * 1024 * 1024 ? "ERR\nnospace" : "ERR\nwrite"; }
                            total += n;
                            if (total - movedBytes >= 1024) { movedAt = System.nanoTime(); movedBytes = total; }
                            else if (System.nanoTime() - movedAt > 45L * 1000000000) return "ERR\nbroke";
                        }
                        out.getFD().sync();
                    }
                    return "OK";
                } finally { c.disconnect(); }
            }
            return "ERR\nbroke";
        } catch (FileNotFoundException e) {
            return "ERR\nwrite";
        } catch (IOException | RuntimeException e) {
            return anything || file.length() > offset ? "ERR\nbroke" : "ERR\nunreachable";
        }
    }
}
