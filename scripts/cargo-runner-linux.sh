#!/bin/sh
# cargo's runner on Linux (.cargo/config.toml): runs a test, bench or `cargo
# run` binary with FFmpeg's shared libraries on the loader's path, from the
# dist FFMPEG_DIR names, the one the binary linked against. A value for
# LD_LIBRARY_PATH in [env] never reaches these binaries, because cargo sets
# LD_LIBRARY_PATH itself for every binary it runs on Linux.
: "${FFMPEG_DIR:?cargo sets FFMPEG_DIR from .cargo/config.toml}"
LD_LIBRARY_PATH="$FFMPEG_DIR/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export LD_LIBRARY_PATH
exec "$@"
