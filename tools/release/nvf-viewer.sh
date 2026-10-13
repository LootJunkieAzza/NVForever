#!/bin/sh
# Runs the asset viewer with the NIF schema shipped beside it.
here=$(cd "$(dirname "$0")" && pwd)
exec "$here/nvf-viewer" --schema "$here/schemas/nif" "$@"
