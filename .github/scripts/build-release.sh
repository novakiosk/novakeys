#!/usr/bin/env bash
set -euo pipefail

dnf install -y gcc pkgconf-pkg-config gtk4-devel gtk4-layer-shell-devel \
  libxkbcommon-devel wayland-devel anthy-unicode-devel libhangul-devel \
  librime-devel brise google-noto-sans-cjk-fonts python3 git tar gzip curl ca-certificates \
  xorg-x11-server-Xvfb xorg-x11-xauth
curl --proto '=https' --tlsv1.2 -fsS https://sh.rustup.rs -o /tmp/rustup-init.sh
sh /tmp/rustup-init.sh -y --profile minimal --default-toolchain none
export PATH="$HOME/.cargo/bin:$PATH"
# rustup reads the repository's toolchain and component selection.
rustup show active-toolchain
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo test --locked ime::tests::real_engine_conversion_and_privacy -- --ignored
xvfb-run -a -s "-screen 0 1600x1200x24" \
  env GDK_BACKEND=x11 GSK_RENDERER=cairo GTK_A11Y=none timeout 120s \
  cargo test --locked ui::gtk_lifecycle::active_tree_releases_widgets_and_cancels_interactions \
  -- --exact --ignored --test-threads=1
cargo build --release --locked

[[ $(uname -m) == x86_64 ]]
package="novakeys-${VERSION}-x86_64"
staging=$(mktemp -d)
mkdir -p "$staging/$package" dist
install -m755 "${CARGO_TARGET_DIR:-target}/release/novakeys" "$staging/$package/novakeys"
install -m644 LICENSE README.md config.example "$staging/$package/"
cargo metadata --locked --format-version 1 --filter-platform x86_64-unknown-linux-gnu > "$staging/metadata.json"
python3 .github/scripts/licenses.py "$staging/metadata.json" "$staging/$package/licenses"
{
  printf 'NOVA Keys %s\nSource commit: %s\nFedora image: %s\n' "$VERSION" "$GITHUB_SHA" "$FEDORA_IMAGE"
  cat /etc/fedora-release
  rustc -Vv
  cargo -V
  rpm -q gtk4 gtk4-layer-shell anthy-unicode libhangul librime brise
  sha256sum Cargo.lock
} > "$staging/$package/BUILD-INFO.txt"
epoch=$(git -c safe.directory=/source show -s --format=%ct "$GITHUB_SHA")
tar --sort=name --mtime="@$epoch" --owner=0 --group=0 --numeric-owner \
  -C "$staging" -cf - "$package" | gzip -n > "dist/$package.tar.gz"
(cd dist && sha256sum "$package.tar.gz" > SHA256SUMS)
