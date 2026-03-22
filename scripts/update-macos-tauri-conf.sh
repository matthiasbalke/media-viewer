#!/usr/bin/env bash
# update-macos-tauri-conf.sh
#
# Collects libheif and all its Homebrew transitive dylib dependencies,
# copies them into src-tauri/macOS-frameworks/, fixes their install names
# so the binary embeds @executable_path/../Frameworks/<name> at link time,
# and generates src-tauri/tauri.macos.conf.json with bundle.macOS.frameworks
# pointing to relative paths (e.g. "./macOS-frameworks/libheif.1.dylib").
#
# Also generates macOS-frameworks/libheif.pc so that pkg-config finds the
# modified dylibs (with fixed install names) rather than the Homebrew originals.
# In CI, prepend macOS-frameworks/ to PKG_CONFIG_PATH before building.
#
# Run this script whenever you update libheif (brew upgrade libheif),
# then commit both src-tauri/macOS-frameworks/ and src-tauri/tauri.macos.conf.json.
#
# Usage:
#   ./scripts/update-macos-tauri-conf.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SRC_TAURI="$REPO_ROOT/src-tauri"
FRAMEWORKS_DIR="$SRC_TAURI/macOS-frameworks"
CONF_FILE="$SRC_TAURI/tauri.macos.conf.json"

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

echo "Resolving libheif dylib dependencies ..."

# Track visited libraries to avoid infinite loops (bash 3.2-compatible, no declare -A)
VISITED=""
ABSOLUTE_PATHS=()

# Recursively follow otool -L to collect all Homebrew dylib paths.
collect_deps() {
  local lib="$1"
  local name
  name=$(basename "$lib")
  case ":$VISITED:" in *":$name:"*) return ;; esac
  VISITED="$VISITED:$name:"
  echo "  + $lib"
  ABSOLUTE_PATHS+=("$lib")
  while IFS= read -r dep; do
    collect_deps "$dep"
  done < <(
    otool -L "$lib" 2>/dev/null \
      | awk '/^[[:space:]]+\// && /\/(opt\/homebrew|usr\/local)\// && !/\/usr\/lib/{print $1}'
  )
}

collect_deps "$(brew --prefix libheif)/lib/libheif.dylib"

echo ""
echo "Copying dylibs to $FRAMEWORKS_DIR ..."
rm -rf "$FRAMEWORKS_DIR"
mkdir -p "$FRAMEWORKS_DIR"

RELATIVE_PATHS=()
for abs in "${ABSOLUTE_PATHS[@]}"; do
  name=$(basename "$abs")
  cp "$abs" "$FRAMEWORKS_DIR/$name"
  echo "  copied $name"
  RELATIVE_PATHS+=("./macOS-frameworks/$name")
done

echo ""
echo "Fixing dylib install names for self-contained bundling ..."

# Step A: Change each dylib's own install name to @executable_path/../Frameworks/<name>
for abs in "${ABSOLUTE_PATHS[@]}"; do
  name=$(basename "$abs")
  dest="$FRAMEWORKS_DIR/$name"
  new_id="@executable_path/../Frameworks/$name"
  install_name_tool -id "$new_id" "$dest"
  echo "  id: $name → $new_id"
done

# Step B: Rewrite each dylib's internal references to other bundled dylibs
for abs in "${ABSOLUTE_PATHS[@]}"; do
  name=$(basename "$abs")
  dest="$FRAMEWORKS_DIR/$name"
  for dep_abs in "${ABSOLUTE_PATHS[@]}"; do
    dep_name=$(basename "$dep_abs")
    install_name_tool -change "$dep_abs" \
      "@executable_path/../Frameworks/$dep_name" "$dest" 2>/dev/null || true
  done
done

echo ""
echo "Generating $FRAMEWORKS_DIR/libheif.pc for compile-time linking ..."

HEIF_PREFIX="$(brew --prefix libheif)"
HEIF_VERSION="$(PKG_CONFIG_PATH="$(brew --prefix libheif)/lib/pkgconfig" pkg-config --modversion libheif)"

cat > "$FRAMEWORKS_DIR/libheif.pc" << PCEOF
prefix=$HEIF_PREFIX
exec_prefix=\${prefix}
libdir=$FRAMEWORKS_DIR
includedir=\${prefix}/include

Name: libheif
Description: HEIF and HEIC file format decoder and encoder
Version: $HEIF_VERSION
Cflags: -I\${includedir}
Libs: -L\${libdir} -lheif
PCEOF

echo "  libheif.pc → libdir=$FRAMEWORKS_DIR, includedir=$HEIF_PREFIX/include"

echo ""
echo "Generating $CONF_FILE ..."

# Write relative paths to a temp file (one per line) — avoids bash 4.4-only @Q expansion
_TMP_PATHS=$(mktemp)
printf '%s\n' "${RELATIVE_PATHS[@]}" > "$_TMP_PATHS"

python3 - "$_TMP_PATHS" "$CONF_FILE" <<'EOF'
import json, sys

paths_file, conf_file = sys.argv[1], sys.argv[2]
with open(paths_file) as f:
    framework_list = [l.rstrip("\n") for l in f if l.strip()]

config = {
    "bundle": {
        "macOS": {
            "frameworks": framework_list
        }
    }
}

with open(conf_file, "w") as fp:
    json.dump(config, fp, indent=2)
    fp.write("\n")

print(open(conf_file).read())
EOF

rm -f "$_TMP_PATHS"

echo "Done. Run from CI; macOS-frameworks/ is gitignored and regenerated each build."
