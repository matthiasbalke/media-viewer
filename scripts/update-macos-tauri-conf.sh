#!/usr/bin/env bash
# update-macos-tauri-conf.sh
#
# Collects libheif and all its Homebrew transitive dylib dependencies and
# generates src-tauri/tauri.macos.conf.json with bundle.macOS.frameworks
# pointing to the ORIGINAL Homebrew library paths.
#
# Tauri reads this during macOS builds and for each listed framework:
#   1. Copies the dylib into the .app's Contents/Frameworks/
#   2. Runs install_name_tool to rewrite the binary's LC_LOAD_DYLIB entry
#      from the absolute Homebrew path to @executable_path/../Frameworks/
#
# Run this script whenever you update libheif (brew upgrade libheif),
# then commit the regenerated src-tauri/tauri.macos.conf.json.
#
# Usage:
#   ./scripts/update-macos-tauri-conf.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CONF_FILE="$REPO_ROOT/src-tauri/tauri.macos.conf.json"

if [[ "$(uname)" != "Darwin" ]]; then
  echo "This script is macOS-only." >&2
  exit 1
fi

if ! command -v brew &>/dev/null; then
  echo "Homebrew is required but not found." >&2
  exit 1
fi

if ! brew list libheif &>/dev/null; then
  echo "libheif is not installed. Run: brew install libheif" >&2
  exit 1
fi

echo "Resolving libheif framework paths ..."

# Track visited libraries to avoid infinite loops
declare -A VISITED
FRAMEWORK_PATHS=()

# Recursively follow otool -L to collect original Homebrew dylib paths.
# We intentionally keep the original paths (not copies) so that Tauri can
# match them against the binary's LC_LOAD_DYLIB entries and rewrite them.
collect_deps() {
  local lib="$1"
  local name
  name=$(basename "$lib")
  [[ "${VISITED[$name]+_}" ]] && return
  VISITED["$name"]=1
  echo "  + $lib"
  FRAMEWORK_PATHS+=("$lib")
  while IFS= read -r dep; do
    collect_deps "$dep"
  done < <(
    otool -L "$lib" 2>/dev/null \
      | awk '/^[[:space:]]+\// && /\/(opt\/homebrew|usr\/local)\// && !/\/usr\/lib/{print $1}'
  )
}

collect_deps "$(brew --prefix libheif)/lib/libheif.dylib"

echo ""
echo "Generating $CONF_FILE ..."

python3 - <<EOF
import json

paths = ${FRAMEWORK_PATHS[@]@Q}
# Parse the bash array passed as quoted strings
import shlex
framework_list = shlex.split(paths)

config = {
    "bundle": {
        "macOS": {
            "frameworks": framework_list
        }
    }
}

with open("$CONF_FILE", "w") as fp:
    json.dump(config, fp, indent=2)
    fp.write("\n")

print(open("$CONF_FILE").read())
EOF

echo "Done. Commit src-tauri/tauri.macos.conf.json to version control."
