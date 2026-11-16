package xyz.pocketui.app;

import android.app.AlertDialog;
import android.content.Intent;
import android.net.Uri;
import android.os.Bundle;

/** The handheld view (android/handheld): the Linux app's pages in libpocket_handheld, over the same Android I/O as the Mobile
 *  view (PocketBase), in a process of its own (the app ends its process when it ends, as on Linux). It reads the games where
 *  they are, so it asks for All files access once, explained, with the Mobile view (the folder picker) as the other way. */
public class HandheldActivity extends PocketBase {
    static { System.loadLibrary("pocket_handheld"); }
    @Override protected boolean ownsBrightness() { return true; }
    @Override protected native boolean nativeBack();
    @Override protected native void nativeConfigurationChanged();
    @Override protected native String nativePrepareDocument(int fd,String key,String name);

    private native void nativeEnding();

    /** The view ends, finished or taken down by Android (a configuration it doesn't follow, memory): what the app keeps is
     *  written, then its process ends, as the app ends on Linux. Its loop does not end with the Activity, and NativeActivity's
     *  onDestroy and any new HandheldActivity in this process would wait for it (an ANR, 12.11.2026; review #2). */
    @Override protected void onDestroy() {
        try { nativeEnding(); } catch (Throwable t) { android.util.Log.w("PocketHandheld", "ending", t); }
        Views.endProcess(this);
        super.onDestroy();
    }

    private AlertDialog filesDialog;
    private static final int READ_GAMES = 51;

    @Override protected void onResume() {
        super.onResume();
        askForFiles();
    }

    private void askForFiles() {
        if ("true".equals(filesAccess()) || (filesDialog != null && filesDialog.isShowing())) return;
        String lang = java.util.Locale.getDefault().getLanguage();
        boolean de = lang.equals("de"), es = lang.equals("es"), pt = lang.equals("pt"), fr = lang.equals("fr"), it = lang.equals("it");
        String title = de ? "Deine Spiele lesen" : es ? "Leer tus juegos" : pt ? "Ler seus jogos" : fr ? "Lire tes jeux" : it ? "Leggere i tuoi giochi" : "Read your games";
        String message = de
            ? "Die Handheld-Oberfläche liest deine Spiele direkt aus dem Ordner ROMs (zum Beispiel ROMs/gba, ROMs/snes). Dafür braucht sie unter Android den Zugriff auf alle Dateien. pocket-ui liest dort nur Spiele und Bilder. Spielstände liegen im App-Ordner; hat die Mobil-Oberfläche einen Spieleordner, kommt nach jedem Spiel eine Kopie in dessen Ordner „pocket-ui saves“.\n\nOhne diesen Zugriff nimm die Mobil-Oberfläche: Dort wählst du einen Ordner aus."
            : es
            ? "La interfaz portátil lee tus juegos directamente de la carpeta ROMs (por ejemplo ROMs/gba, ROMs/snes). Para eso Android le pide acceso a todos los archivos. pocket-ui solo lee juegos e imágenes ahí. Las partidas guardadas están en la carpeta de la app; si la interfaz móvil tiene una carpeta de juegos, al terminar cada juego se copia una a su carpeta «pocket-ui saves».\n\nSin este acceso, usa la interfaz móvil: allí eliges una carpeta."
            : pt
            ? "A interface de handheld lê seus jogos direto da pasta ROMs (por exemplo ROMs/gba, ROMs/snes). Para isso, o Android pede acesso a todos os arquivos. O pocket-ui só lê jogos e imagens ali. Seus saves ficam na pasta do app; se o visual de celular tiver uma pasta de jogos, ao fim de cada jogo uma cópia deles vai para a pasta \"pocket-ui saves\".\n\nSem esse acesso, use o visual de celular: lá você escolhe uma pasta."
            : fr
            ? "L'interface Handheld lit tes jeux directement dans le dossier ROMs (par exemple ROMs/gba, ROMs/snes). Pour cela, Android demande l'accès à tous les fichiers. pocket-ui n'y lit que des jeux et des images. Tes sauvegardes restent dans le dossier de l'appli ; si la vue Mobile a un dossier de jeux, une copie va dans son dossier \"pocket-ui saves\" à la fin d'une partie.\n\nSans cet accès, utilise la vue Mobile : tu y choisis un dossier."
            : it
            ? "La vista portatile legge i tuoi giochi direttamente dalla cartella ROMs (per esempio ROMs/gba, ROMs/snes). Per questo Android chiede l'accesso a tutti i file. pocket-ui legge lì solo giochi e immagini. I save stanno nella cartella dell'app; se la vista mobile ha una cartella dei giochi, a fine partita una loro copia va nella sua cartella \"pocket-ui saves\".\n\nSenza questo accesso usa la vista mobile: lì scegli una cartella."
            : "The Handheld view reads your games straight from the ROMs folder (for example ROMs/gba, ROMs/snes). For that, Android asks for access to all files. pocket-ui only reads games and pictures there. Your saves live in the app's own folder; if the Mobile view has a games folder, a copy of them goes into its folder \"pocket-ui saves\" when a game ends.\n\nWithout this access, use the Mobile view: there you pick a folder.";
        String allow = de ? "Zugriff erlauben" : es ? "Permitir acceso" : pt ? "Permitir acesso" : fr ? "Autoriser l'accès" : it ? "Consenti accesso" : "Allow access";
        String mobile = de ? "Mobil-Oberfläche" : es ? "Interfaz móvil" : pt ? "Visual de celular" : fr ? "Vue Mobile" : it ? "Vista mobile" : "Mobile view";
        filesDialog = new AlertDialog.Builder(this).setTitle(title).setMessage(message).setCancelable(false)
            .setPositiveButton(allow, (d, w) -> {
                if (android.os.Build.VERSION.SDK_INT >= 30) {
                    try { startActivity(new Intent(android.provider.Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION, Uri.parse("package:" + getPackageName()))); }
                    catch (RuntimeException e) { startActivity(new Intent(android.provider.Settings.ACTION_MANAGE_ALL_FILES_ACCESS_PERMISSION)); }
                } else requestPermissions(new String[] { android.Manifest.permission.READ_EXTERNAL_STORAGE }, READ_GAMES);
            })
            .setNegativeButton(mobile, (d, w) -> setView(Views.MOBILE))
            .show();
        // A pad has no touch to focus a button: the main button has the focus from the start.
        android.widget.Button main = filesDialog.getButton(AlertDialog.BUTTON_POSITIVE);
        if (main != null) { main.setFocusableInTouchMode(true); main.requestFocus(); }
    }

    /** Before Android 11: refused (also "don't ask again", which brings no onResume), the explanation comes back with the
     *  Mobile view as the other way. */
    @Override public void onRequestPermissionsResult(int code, String[] permissions, int[] results) {
        super.onRequestPermissionsResult(code, permissions, results);
        if (code == READ_GAMES) askForFiles();
    }
}
