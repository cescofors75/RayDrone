#!/usr/bin/env bash
# SessionStart hook: make the VST crate buildable in a fresh web session.
#
# The VST GUI (nih_plug_egui) and the standalone binary link system libraries
# that aren't in the base image: OpenGL/X11/xcb (egui's windowing) and
# ALSA/JACK (standalone audio). The WASM engine and the `core` crate need none
# of this — only the VST does. We install the -dev packages so `cargo build`
# inside `vst/` finds them via pkg-config (its default search path covers
# /usr/lib/*/pkgconfig once the .pc files exist, so no PKG_CONFIG_PATH needed).
#
# Best-effort and idempotent: if the libs are already present we exit fast, and
# we never fail the session if apt is unavailable.
set -u

# Already satisfied? (covers a warm container or a re-run) → nothing to do.
if pkg-config --exists gl x11-xcb alsa jack 2>/dev/null; then
    exit 0
fi

# Need root + apt to install. If neither is available, just note it and move on.
if ! command -v apt-get >/dev/null 2>&1; then
    echo "[setup-build-deps] apt-get not found; skipping VST system-lib install." >&2
    exit 0
fi
SUDO=""
if [ "$(id -u)" -ne 0 ]; then
    if sudo -n true 2>/dev/null; then SUDO="sudo"; else
        echo "[setup-build-deps] no root; skipping VST system-lib install." >&2
        exit 0
    fi
fi

echo "[setup-build-deps] installing VST system libraries (OpenGL/X11/xcb, ALSA, JACK)..." >&2
export DEBIAN_FRONTEND=noninteractive
$SUDO apt-get update -qq 2>/dev/null || true
$SUDO apt-get install -y -q \
    pkg-config \
    libgl-dev libx11-dev libx11-xcb-dev \
    libxcb1-dev libxcb-icccm4-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxcb-dri2-0-dev \
    libxcursor-dev libxrandr-dev libxi-dev \
    libasound2-dev libjack-jackd2-dev \
    >/dev/null 2>&1 || echo "[setup-build-deps] install hit an error; VST build may need libs installed manually." >&2

# Don't block the session regardless of how apt went.
exit 0
