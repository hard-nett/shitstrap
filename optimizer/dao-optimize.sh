#!/bin/ash
# shellcheck shell=dash
# Custom CosmWasm optimizer entrypoint for workspaces with local sibling dependencies.
set -o errexit -o nounset -o pipefail

export PATH="$PATH:/root/.cargo/bin"

# Resolve the workspace subdirectory
SUBDIR="${DAO_WORKSPACE_SUBDIR:-shitstrap}"
WORKSPACE_ROOT="/workspace"
PROJECT_DIR="$WORKSPACE_ROOT/$SUBDIR"

if [ ! -d "$PROJECT_DIR" ]; then
  echo "ERROR: Project directory $PROJECT_DIR does not exist." >&2
  echo "Make sure to mount the parent directory tree at /workspace." >&2
  exit 1
fi

if [ ! -f "$PROJECT_DIR/Cargo.toml" ]; then
  echo "ERROR: No Cargo.toml found at $PROJECT_DIR" >&2
  exit 1
fi

echo "=== DAO Custom Optimizer ==="
echo "Workspace root:  $WORKSPACE_ROOT"
echo "Project dir:     $PROJECT_DIR"
rustup toolchain list
cargo --version

mkdir -p "$PROJECT_DIR/artifacts"
rm -f /target/wasm32-unknown-unknown/release/*.wasm

# Hide excluded crates from bob's filesystem scanner
EXCLUDED_CRATES="dao-migrator"
STASH_DIR="/tmp/_dao_optimizer_stash"
RESTORE=0

for crate in $EXCLUDED_CRATES; do
  CRATE_PATH="$PROJECT_DIR/contracts/external/$crate"
  if [ -d "$CRATE_PATH" ]; then
    echo "Stashing excluded crate: $crate"
    mkdir -p "$STASH_DIR"
    mv "$CRATE_PATH" "$STASH_DIR/"
    RESTORE=1
  fi
done

echo "Building project $PROJECT_DIR ..."
(
  cd "$PROJECT_DIR"
  /usr/local/bin/bob .
)
BUILD_EXIT=$?

if [ "$RESTORE" -eq 1 ]; then
  echo "Restoring stashed crates ..."
  for crate in "$STASH_DIR"/*; do
    [ -d "$crate" ] && mv "$crate" "$PROJECT_DIR/contracts/external/"
  done
  rmdir "$STASH_DIR" 2>/dev/null || true
fi

if [ "$BUILD_EXIT" -ne 0 ]; then
  exit $BUILD_EXIT
fi

# Optimize: run wasm-opt on each built .wasm
# Rust 1.78+ produces Wasm with bulk memory operations (memory.copy, memory.fill).
# These require CosmWasm 3.0+ on chain. We enable bulk memory in wasm-opt so it
# accepts the Wasm — there is no way to strip these from the std library without
# -Z build-std.
echo "Optimizing artifacts ..."
for WASM in /target/wasm32-unknown-unknown/release/*.wasm; do
  [ -e "$WASM" ] || continue

  OUT_FILENAME=$(basename "$WASM")
  echo "Optimizing $OUT_FILENAME ..."
  wasm-opt -Os --enable-bulk-memory "$WASM" -o "$PROJECT_DIR/artifacts/$OUT_FILENAME"
done

echo "Post-processing artifacts..."
(
  cd "$PROJECT_DIR/artifacts"
  if test -n "$(find . -maxdepth 1 -name '*.wasm' -print -quit)"; then
    sha256sum -- *.wasm | tee checksums.txt
  else
    echo "Warn: No .wasm file built." >&2
  fi
)

echo "Done."