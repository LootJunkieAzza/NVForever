#!/bin/sh
# Runs NVForever with the schemas shipped beside it. Game data: set NVFOREVER_DATA to your
# Fallout New Vegas Data folder (read-only), or pass --data .../Data.
# Example: ./nvforever.sh --play --cell Goodsprings
here=$(cd "$(dirname "$0")" && pwd)
: "${NVF_PLAY_SCRIPT_FUNCTIONS:=$here/schemas/script/functions.json}"
export NVF_PLAY_SCRIPT_FUNCTIONS
exec "$here/nvforever" --esm-schema "$here/schemas/fnv" --nif-schema "$here/schemas/nif" "$@"
