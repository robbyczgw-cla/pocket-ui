# The virtual Mini V2: the ROCKNIX userland (pocket-vdev-base, imported by `scripts/vdev.sh image` from
# the real ROCKNIX image, never shipped) plus Debian's Mesa for llvmpipe and a few boot scripts.

ARG BASE=pocket-vdev-base:latest

# Stage 1: Mesa with llvmpipe. ROCKNIX's own Mesa has no llvmpipe, only softpipe (~2 fps here).
FROM debian:trixie-slim AS mesa
RUN apt-get update \
 && apt-get install -y --no-install-recommends libegl1 libgles2 libegl-mesa0 libgl1-mesa-dri libgbm1 \
 && rm -rf /var/lib/apt/lists/*
COPY mesa-closure.sh /mesa-closure.sh
RUN sh /mesa-closure.sh /out

# Stage 2: the device.
FROM ${BASE}

COPY --from=mesa /out/ /opt/mesa/
# Keep ROCKNIX's own copy of every library it has, except the Mesa family itself that has to be replaced.
RUN cd /opt/mesa/lib && for f in *.so*; do \
      case "$f" in libEGL*|libGLES*|libGLdispatch*|libGLX*|libGL.*|libgallium*|libLLVM*|libglapi*|libgbm*) ;; \
        *) [ -e "/usr/lib/$f" ] && rm -f "$f" ;; esac; \
    done; true

COPY vdev/ /vdev/
RUN chmod +x /vdev/entrypoint.sh /vdev/vdev-pad.py /vdev/roms-link.py /vdev/bin/*

# What the systemd units and run-pocket-ui.sh set on the device, plus software GL through llvmpipe.
ENV PATH=/vdev/bin:/usr/bin:/usr/sbin \
    HOME=/storage \
    XDG_RUNTIME_DIR=/var/run/0-runtime-dir \
    WAYLAND_DISPLAY=wayland-1 \
    SWAYSOCK=/var/run/0-runtime-dir/sway-ipc.0.sock \
    PIPEWIRE_RUNTIME_DIR=/run/pipewire \
    LD_LIBRARY_PATH=/opt/mesa/lib \
    LIBGL_DRIVERS_PATH=/opt/mesa/lib/dri \
    __EGL_VENDOR_LIBRARY_FILENAMES=/opt/mesa/share/glvnd/egl_vendor.d/50_mesa.json \
    LIBGL_ALWAYS_SOFTWARE=1

ENTRYPOINT ["/vdev/entrypoint.sh"]
