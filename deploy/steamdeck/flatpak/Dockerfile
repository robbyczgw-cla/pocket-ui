# Build and test environment for the pocket-ui Flatpak (docs/STEAMDECK.md): flatpak-builder, and Gamescope with Xvfb to run
# the result headless at 1280x800. Run privileged (bubblewrap needs user namespaces). Never built in CI.
FROM ubuntu:26.04
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends flatpak flatpak-builder gamescope xvfb xauth x11-utils xdotool imagemagick \
    python3 ca-certificates curl dbus-x11 mesa-vulkan-drivers libgl1-mesa-dri procps fonts-dejavu-core && rm -rf /var/lib/apt/lists/*
RUN flatpak remote-add --system --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
