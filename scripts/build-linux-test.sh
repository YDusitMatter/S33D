#!/bin/bash
set -euo pipefail
if [[ $(uname -s) != Linux || $(uname -m) != x86_64 ]]; then
    echo 'Run this script on x86_64 Linux.' >&2
    exit 1
fi
if [[ $# != 3 ]]; then
    echo 'Usage: build-linux-test.sh NEW_RUN_DIRECTORY SOURCE_ZIP YYYY-MM-DD' >&2
    exit 1
fi
repo=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
run=$(realpath -m -- "$1")
source_zip=$(realpath -- "$2")
package_date=$3
if [[ -e $run ]]; then
    echo 'Run directory already exists; preserve previous builds.' >&2
    exit 1
fi
if [[ -n ${RUSTFLAGS:-}${CARGO_ENCODED_RUSTFLAGS:-}${CFLAGS:-}${CXXFLAGS:-} ]]; then
    echo 'Review pre-existing compiler flags before privacy-mapped building.' >&2
    exit 1
fi
mkdir -p -- "$run/private" "$run/build/release"
cp -R -- "$repo/Plug-ins" "$run/build/release/Plug-ins"
export CARGO_TARGET_DIR="$run/build"
export CARGO_PROFILE_RELEASE_DEBUG=0
export CARGO_PROFILE_RELEASE_STRIP=debuginfo
export CMAKE_BUILD_PARALLEL_LEVEL=6
export RUSTFLAGS="--remap-path-prefix=$repo=/src/S33D --remap-path-prefix=$run=/build/S33D --remap-path-prefix=$CARGO_HOME=/cargo-cache --remap-path-prefix=$HOME=/build-user"
export CFLAGS="-ffile-prefix-map=$repo=/src/S33D -ffile-prefix-map=$run=/build/S33D -ffile-prefix-map=$CARGO_HOME=/cargo-cache -ffile-prefix-map=$HOME=/build-user"
export CXXFLAGS="$CFLAGS"
rustc --version > "$run/private/toolchain.txt"
cargo --version >> "$run/private/toolchain.txt"
cmake --version >> "$run/private/toolchain.txt"
cargo fmt --all --manifest-path "$repo/Cargo.toml" -- --check > "$run/private/fmt.log" 2>&1
cargo build --release --locked --offline --manifest-path "$repo/Cargo.toml" --bin s33d-app --bin s33d-agent -j 2 > "$run/private/build.log" 2>&1
printf '{"exit_code":0,"release_profile":true,"target":"x86_64-unknown-linux-gnu"}\n' > "$run/private/build-result.json"
cargo test --workspace --release --locked --offline --manifest-path "$repo/Cargo.toml" -j 2 > "$run/private/tests.log" 2>&1
printf '{"exit_code":0,"workspace":true,"release_profile":true}\n' > "$run/private/test-result.json"
python3 "$repo/scripts/package-linux.py" --release-directory "$run/build/release" --source-archive "$source_zip" --output-directory "$run/package" --date "$package_date" > "$run/private/package-result.json"
cat "$run/private/package-result.json"
