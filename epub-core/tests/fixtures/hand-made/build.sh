#!/usr/bin/env sh
# Rebuild the hand-made fixture EPUBs from the folders next to this script.
# Each folder <name>/ becomes ../<name>.epub. The `mimetype` entry goes first
# and uncompressed (-0), as the EPUB spec requires.
set -eu
cd "$(dirname "$0")"
for dir in */; do
    name="${dir%/}"
    out="../$name.epub"
    rm -f "$out"
    (
        cd "$name"
        zip -X0q "../$out" mimetype
        # Exit code 12 means "nothing to add" (a folder holding only mimetype).
        zip -X9rqD "../$out" . -x mimetype >/dev/null 2>&1 || [ $? -eq 12 ]
    )
done
printf 'This is a text file, not a zip archive.\n' > ../not-a-zip.epub
