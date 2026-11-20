#!/bin/bash
# Capability probe only: never installs a runtime or reaches the network.
reader_jvm_works() {
    command -v javac >/dev/null 2>&1 && command -v java >/dev/null 2>&1 &&
        javac --help 2>&1 | grep -- '--release' >/dev/null && java -version >/dev/null 2>&1
}
reader_find_jvm() {
    if [ -n "${JAVA_READER_JDK:-}" ]; then
        PATH="$JAVA_READER_JDK/bin:$PATH"; export PATH
        [ -x "$JAVA_READER_JDK/bin/javac" ] && [ -x "$JAVA_READER_JDK/bin/java" ] && reader_jvm_works
        return $?
    fi
    if reader_jvm_works; then return 0; fi
    for reader_jdk_bin in "${JAVA_HOME:-/nonexistent}/bin" "$HOME"/opt/jdk-*/bin; do
        if [ -x "$reader_jdk_bin/javac" ] && [ -x "$reader_jdk_bin/java" ]; then
            PATH="$reader_jdk_bin:$PATH"; export PATH
            if reader_jvm_works; then return 0; fi
        fi
    done
    return 1
}
reader_require_jvm() {
    if reader_find_jvm; then return; fi
    echo 'JVM file-reader rules unavailable: this host has no compatible JDK. Android Java readers were NOT executed.' >&2
    if [ "${1:-}" = --if-available ]; then exit 77; fi
    exit 1
}

reader_require_proc() {
    if [ -d /proc/self/fd ]; then return; fi
    echo 'JVM descriptor reader rules unavailable: Linux /proc/self/fd is required. Android descriptor probes were NOT executed.' >&2
    if [ "${1:-}" = --if-available ]; then exit 77; fi
    exit 1
}
