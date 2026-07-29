#!/usr/bin/env bash
# Fetch, verify and build the pinned llama.cpp for ONE Android ABI.
#
# Deliberately not vendored: the source is fetched at an exact commit and its
# tree hash is verified BEFORE any compilation, so what ships is reproducible
# from this repository plus the lock file — not an opaque blob someone has to
# trust. Nothing built here is committed.
set -euo pipefail
cd "$(dirname "$0")"

LOCK=llama-source.lock.json
read_lock() { python3 -c "import json;print(json.load(open('$LOCK'))$1)"; }

COMMIT=$(read_lock "['commit']")
EXPECT_SHA=$(read_lock "['sourceArchiveSha256']")
ABI=$(read_lock "['androidAbi']")
PLATFORM=$(read_lock "['androidPlatform']")
NDK_VERSION=$(read_lock "['ndkVersion']")

: "${ANDROID_NDK_HOME:=/opt/homebrew/share/android-commandlinetools/ndk/$NDK_VERSION}"
[ -d "$ANDROID_NDK_HOME" ] || { echo "NDK $NDK_VERSION not found at $ANDROID_NDK_HOME" >&2; exit 2; }

WORK=${LLAMA_WORK_DIR:-build/llama-src}
OUT=build/jniLibs/$ABI
mkdir -p "$(dirname "$WORK")" "$OUT"

if [ ! -d "$WORK/.git" ]; then
  echo "fetching llama.cpp @ $COMMIT"
  git init --quiet "$WORK"
  git -C "$WORK" remote add origin "https://github.com/ggml-org/llama.cpp.git" 2>/dev/null || true
  git -C "$WORK" fetch --quiet --depth 1 origin "$COMMIT"
  git -C "$WORK" checkout --quiet FETCH_HEAD
fi

# Verify BEFORE building: a moved tag or a tampered tree must not compile.
ACTUAL_SHA=$(git -C "$WORK" archive --format=tar HEAD | shasum -a 256 | cut -c1-64)
if [ "$ACTUAL_SHA" != "$EXPECT_SHA" ]; then
  echo "source tree hash mismatch" >&2
  echo "  expected $EXPECT_SHA" >&2
  echo "  actual   $ACTUAL_SHA" >&2
  exit 1
fi
echo "source tree verified: $ACTUAL_SHA"

cmake -B "$WORK/build-$ABI" -S "$WORK" -G Ninja \
  -DCMAKE_TOOLCHAIN_FILE="$ANDROID_NDK_HOME/build/cmake/android.toolchain.cmake" \
  -DANDROID_ABI="$ABI" \
  -DANDROID_PLATFORM="$PLATFORM" \
  -DCMAKE_BUILD_TYPE=Release \
  -DBUILD_SHARED_LIBS=OFF \
  -DLLAMA_CURL=OFF \
  -DLLAMA_BUILD_SERVER=OFF \
  -DLLAMA_BUILD_EXAMPLES=OFF \
  -DLLAMA_BUILD_TESTS=OFF \
  -DLLAMA_BUILD_TOOLS=OFF \
  -DGGML_OPENMP=OFF \
  -DGGML_VULKAN=OFF \
  -DGGML_OPENCL=OFF > /dev/null

cmake --build "$WORK/build-$ABI" --target llama llama-common -j "$(sysctl -n hw.ncpu 2>/dev/null || nproc)"

echo "llama.cpp static libs built for $ABI"
echo "  commit: $COMMIT"
echo "  tree:   $ACTUAL_SHA"
