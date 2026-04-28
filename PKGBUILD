# Maintainer: Your Name <youremail@domain.com>
pkgname=wallerd
pkgver=r2.f84f838
pkgrel=1
pkgdesc="A wayland daemon that can display images as a wallpaper, or in a window, with GLSL shader support, and image switch"
arch=('x86_64')
url="https://github.com/Remi-Avec-Un-I/wallerd"
license=('MIT')
depends=('wayland' 'libglvnd')
makedepends=('rust' 'git')
source=("git+$url.git")
sha256sums=('SKIP')


pkgver() {
    cd "$pkgname"
    git describe --long --tags --abbrev=7 2>/dev/null || printf "r%s.%s" "$(git rev-list --count HEAD)" "$(git rev-parse --short HEAD)"
}

prepare() {
    cd "$pkgname"
    export RUSTUP_TOOLCHAIN=stable
    cargo fetch --locked --target "$(rustc -vV | sed -n 's/host: //p')"
}

build() {
    cd "$pkgname"
    export RUSTFLAGS="-C target-cpu=native"
    export RUSTUP_TOOLCHAIN=stable
    export CARGO_TARGET_DIR=target
    cargo build --release --frozen
}

package() {
    cd "$pkgname"

    install -Dm755 target/release/wallerd   "$pkgdir/usr/bin/wallerd"
    install -Dm755 target/release/wallerctl "$pkgdir/usr/bin/wallerctl"

    install -dm755 "$pkgdir/usr/share/wallerd/shaders"
    cp -r shaders/* "$pkgdir/usr/share/wallerd/shaders/"
}
