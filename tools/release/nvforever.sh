#!/bin/sh
# Runs NVForever with the schemas shipped beside it (nvforever also finds them itself when
# started directly). With no arguments it opens on the main menu. Game data: pass --data
# with your Fallout New Vegas folder (read-only) or set Path= under [Game] in the home's
# NVForever.ini.
# Examples: ./nvforever.sh --data ~/FalloutNV           (the main menu)
#           ./nvforever.sh --play --cell Primm         (playing, straight in Primm)
#           ./nvforever.sh --explore                   (the free-camera explorer)
here=$(cd "$(dirname "$0")" && pwd)
: "${NVF_PLAY_SCRIPT_FUNCTIONS:=$here/schemas/script/functions.json}"
export NVF_PLAY_SCRIPT_FUNCTIONS
exec "$here/nvforever" --esm-schema "$here/schemas/fnv" --nif-schema "$here/schemas/nif" "$@"
