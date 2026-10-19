#!/bin/sh
# Install yp, the yourprompt command line, from a GitHub release.
#
#   curl -fsSL https://raw.githubusercontent.com/LiU-SKY/yourprompt/main/install.sh | sh
#
# Settings come from the environment, and go to sh rather than to curl:
#
#   curl -fsSL .../install.sh | YP_VERSION=v0.1.1 sh
#
#   YP_VERSION        the release to install (default: the latest)
#   YP_INSTALL_DIR    where to put yp (default: ~/.local/bin)
#   YP_DOWNLOAD_BASE  where the releases are (default: this repository's on
#                     GitHub). For a mirror, and for testing this script.
#
# Linux and macOS. On Windows, use install.ps1.
#
# Nothing runs until the last line calls main, so a download cut off halfway
# through this file runs nothing at all.

set -eu

repo=LiU-SKY/yourprompt
tmp=
staged=

say() {
    printf '%s\n' "$*"
}

die() {
    printf 'yp installer: %s\n' "$*" >&2
    exit 1
}

cleanup() {
    if [ -n "$staged" ]; then rm -f "$staged"; fi
    if [ -n "$tmp" ]; then rm -rf "$tmp"; fi
}

# Sets target to the release build for this machine.
detect_target() {
    os=$(uname -s)
    arch=$(uname -m)
    case $os in
        Linux) ;;
        Darwin)
            # A shell running under Rosetta reports x86_64 on Apple silicon,
            # where the native build is the one to have.
            if [ "$arch" = x86_64 ] && [ "$(sysctl -n sysctl.proc_translated 2>/dev/null)" = 1 ]; then
                arch=arm64
            fi
            ;;
        MINGW* | MSYS* | CYGWIN* | Windows_NT)
            die "this script is for Linux and macOS. On Windows, run this in PowerShell:
    irm https://raw.githubusercontent.com/$repo/main/install.ps1 | iex"
            ;;
        *)
            die "there is no prebuilt yp for $os. Build it from source instead:
    cargo install --git https://github.com/$repo yp-cli"
            ;;
    esac
    case $arch in
        x86_64 | amd64) arch=x86_64 ;;
        aarch64 | arm64) arch=aarch64 ;;
        *)
            die "there is no prebuilt yp for $arch $os. Build it from source instead:
    cargo install --git https://github.com/$repo yp-cli"
            ;;
    esac
    case $os in
        # The Linux builds are static, so they run whatever the distribution
        # and however old its C library.
        Linux) target=$arch-unknown-linux-musl ;;
        Darwin) target=$arch-apple-darwin ;;
    esac
}

download() {
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL --retry 3 -o "$2" "$1"
    else
        wget -q -O "$2" "$1"
    fi
}

sha256() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1"
    else
        shasum -a 256 "$1"
    fi
}

# The directory a file is in, spelled the way pwd spells it.
dir_of() {
    (CDPATH='' cd "$(dirname "$1")" && pwd)
}

main() {
    command -v curl >/dev/null 2>&1 || command -v wget >/dev/null 2>&1 ||
        die "need curl or wget to download yp"
    command -v sha256sum >/dev/null 2>&1 || command -v shasum >/dev/null 2>&1 ||
        die "need sha256sum or shasum to check the download"

    detect_target

    if [ -n "${YP_INSTALL_DIR:-}" ]; then
        dir=$YP_INSTALL_DIR
    elif [ -n "${HOME:-}" ]; then
        dir=$HOME/.local/bin
    else
        die "HOME is not set, so there is no default place to install to. Set YP_INSTALL_DIR."
    fi

    mkdir -p "$dir" || die "could not create $dir. Set YP_INSTALL_DIR to a directory you own."
    [ -w "$dir" ] || die "cannot write to $dir. Set YP_INSTALL_DIR to a directory you own."
    dir=$(CDPATH='' cd "$dir" && pwd)

    base=${YP_DOWNLOAD_BASE:-https://github.com/$repo/releases}
    base=${base%/}
    case ${YP_VERSION:-latest} in
        latest) url=$base/latest/download release="the latest release" ;;
        v*) url=$base/download/$YP_VERSION release=$YP_VERSION ;;
        *) url=$base/download/v$YP_VERSION release=v$YP_VERSION ;;
    esac
    asset=yp-$target.tar.gz

    trap cleanup EXIT
    trap 'exit 1' HUP INT TERM
    tmp=$(mktemp -d 2>/dev/null || mktemp -d -t yp-install)

    say "downloading $asset from $release"
    download "$url/$asset" "$tmp/$asset" ||
        die "could not download $url/$asset"
    download "$url/$asset.sha256" "$tmp/$asset.sha256" ||
        die "could not download $url/$asset.sha256"

    expected=$(awk '{ print tolower($1); exit }' "$tmp/$asset.sha256")
    case $expected in
        "" | *[!0-9a-f]*) die "$asset.sha256 does not hold a SHA-256 checksum. Nothing was installed." ;;
    esac
    actual=$(sha256 "$tmp/$asset" | awk '{ print tolower($1) }')
    if [ "$actual" != "$expected" ]; then
        die "checksum mismatch for $asset: the release says $expected, the download is $actual. Nothing was installed."
    fi
    say "checksum verified"

    tar -xzf "$tmp/$asset" -C "$tmp"
    [ -f "$tmp/yp" ] || die "$asset does not contain yp. Nothing was installed."

    # Copied in beside the old binary, then renamed over it, because a yp may
    # be running right now (the status line runs it often). Linux refuses to
    # write over a running program, and on macOS writing over a signed binary
    # in place can get its next launch killed. A rename avoids both, and no
    # one ever runs a half-written file.
    staged=$dir/.yp-install.$$
    cp "$tmp/yp" "$staged"
    chmod 755 "$staged"
    mv -f "$staged" "$dir/yp"
    staged=

    version=$("$dir/yp" --version) || die "installed $dir/yp, but it does not run on this machine"
    say "installed $version to $dir/yp"
    say ""

    case ":${PATH:-}:" in
        *":$dir:"* | *":$dir/:"*) on_path=true ;;
        *) on_path=false ;;
    esac
    found=$(command -v yp 2>/dev/null || true)
    case $dir in
        *" "*) run="\"$dir/yp\"" ;;
        *) run=$dir/yp ;;
    esac
    if ! $on_path; then
        say "$dir is not on your PATH. To run yp by name, add this line to your"
        say "shell's startup file (~/.zshrc, ~/.bashrc, ...) and open a new terminal:"
        say ""
        say "    export PATH=\"$dir:\$PATH\""
    elif [ "$(dir_of "$found")" != "$dir" ]; then
        say "$dir is on your PATH, but $found comes before it, so \`yp\` runs that one."
        say "Remove it, or move $dir ahead of it."
    else
        say "$dir is on your PATH."
        run=yp
    fi

    say ""
    say "Next, register the hook and the status line with Claude Code:"
    say ""
    say "    $run install"
}

main "$@"
