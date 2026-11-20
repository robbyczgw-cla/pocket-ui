# Host boundaries for the Java file-reader rule

The `android.*` classes here are narrow, file-backed test doubles. The production
GameCopy, GameDirect, SaveBackup, SaveCopy, UpdateInstaller and Views classes are
compiled unchanged. Their file/stream/ZIP readers run against real local files;
ParcelFileDescriptor owns a real Linux descriptor and GameDirect really reopens
`/proc/self/fd`. Unsupported Activity, task, APK-parser and storage operations fail
explicitly. No Android runtime, Binder, provider permissions, mobile filesystem or
PackageInstaller acceptance is simulated as an established device result.

`org/json` is the unmodified, Apache-2.0-licensed Android JSON implementation from
`com.vaadin.external.google:android-json:0.0.20131108.vaadin1`, sources archive:
https://repo.maven.apache.org/maven2/com/vaadin/external/google/android-json/0.0.20131108.vaadin1/android-json-0.0.20131108.vaadin1-sources.jar
Sources archive SHA-256: `54c781eea645c450cbbc4a5a1b5a474745465452cec1354cb567b781ea6622c3`.
Its copyright/license headers are retained. It is test-only, permits an offline
JDK run, and is not bundled in the APK. Android's installed platform JSON library
remains its own implementation; these probes exercise the app's file boundaries,
not equivalence of every platform JSON extension.
