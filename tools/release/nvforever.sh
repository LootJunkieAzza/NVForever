#!/bin/sh
# Runs NVForever with the schemas shipped beside it. Game data: pass --data with your
# Fallout New Vegas folder (read-only) or set Path= under [Game] in the home's NVForever.ini.
# Example: ./nvforever.sh --play --cell Goodsprings
here=$(cd "$(dirname "$0")" && pwd)
: "${NVF_PLAY_SCRIPT_FUNCTIONS:=$here/schemas/script/functions.json}"
export NVF_PLAY_SCRIPT_FUNCTIONS
exec "$here/nvforever" --esm-schema "$here/schemas/fnv" --nif-schema "$here/schemas/nif" "$@"
