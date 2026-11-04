#!/usr/bin/env bash
# Build a release tarball for a GitHub release.
#
#   packaging/make-release.sh            # builds in a Debian bookworm container
#   NO_DOCKER=1 packaging/make-release.sh   # builds with the local toolchain
#
# The container build matters: a binary built on a rolling distribution needs a
# very new glibc and will not start on an LTS release. bookworm gives glibc 2.36.
set -euo pipefail

here="$(dirname "$(readlink -f "$0")")"
root="$(dirname "$here")"
version="$(grep -m1 '^version' "$root/Cargo.toml" | cut -d'"' -f2)"
name="tmx-t150-panel-$version-linux-x86_64"
stage="$root/target/package"
out="$stage/$name"

rm -rf "$out" "$stage/$name.tar.gz"
mkdir -p "$out"

echo "== build =="
if [ -z "${NO_DOCKER:-}" ] && command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
    mkdir -p /tmp/tmx-cargo-home /tmp/tmx-build-bookworm
    docker run --rm --user "$(id -u):$(id -g)" \
        -e HOME=/tmp/cargo-home -e CARGO_HOME=/tmp/cargo-home \
        -v "$root":/src -v /tmp/tmx-cargo-home:/tmp/cargo-home \
        -v /tmp/tmx-build-bookworm:/build -w /src \
        rust:1-bookworm cargo build --release --target-dir /build
    install -m 755 /tmp/tmx-build-bookworm/release/tmx-t150-panel "$out/tmx-t150-panel"
else
    (cd "$root" && cargo build --release)
    install -m 755 "$root/target/release/tmx-t150-panel" "$out/tmx-t150-panel"
fi

echo "== assemble =="
install -m 644 "$root/desktop/tmx-t150-panel.svg" "$out/tmx-t150-panel.svg"
install -m 755 "$here/install.sh" "$out/install.sh"
install -m 755 "$here/uninstall.sh" "$out/uninstall.sh"
install -m 644 "$root/README.md" "$out/README.md"
install -m 644 "$root/LICENSE" "$out/LICENSE"
install -m 644 "$root/NOTICE" "$out/NOTICE"

# Kernel module sources, straight from git so build artefacts cannot slip in.
if git -C "$root" rev-parse --git-dir >/dev/null 2>&1; then
    git -C "$root" archive HEAD driver/t150 | tar -x -C "$out"
else
    mkdir -p "$out/driver"
    cp -r "$root/driver/t150" "$out/driver/"
    rm -rf "$out/driver/t150/build"
    find "$out/driver/t150" -name '*.ko' -o -name '*.o' -o -name '*.mod*' -o -name '.*.cmd' | xargs -r rm -f
fi

tar -czf "$stage/$name.tar.gz" -C "$stage" "$name"
(cd "$stage" && sha256sum "$name.tar.gz" > "$name.tar.gz.sha256")

echo
echo "$stage/$name.tar.gz"
cat "$stage/$name.tar.gz.sha256"
