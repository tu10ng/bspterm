#!/usr/bin/env sh
set -eu

# Downloads a tarball and unpacks it into ~/.local/.

main() {
    platform="$(uname -s)"
    arch="$(uname -m)"
    channel="${BSPTERM_CHANNEL:-stable}"
    BSPTERM_VERSION="${BSPTERM_VERSION:-latest}"
    # Use TMPDIR if available (for environments with non-standard temp directories)
    if [ -n "${TMPDIR:-}" ] && [ -d "${TMPDIR}" ]; then
        temp="$(mktemp -d "$TMPDIR/bspterm-XXXXXX")"
    else
        temp="$(mktemp -d "/tmp/bspterm-XXXXXX")"
    fi

    if [ "$platform" = "Darwin" ]; then
        platform="macos"
    elif [ "$platform" = "Linux" ]; then
        platform="linux"
    else
        echo "Unsupported platform $platform"
        exit 1
    fi

    case "$platform-$arch" in
        macos-arm64* | linux-arm64* | linux-armhf | linux-aarch64)
            arch="aarch64"
            ;;
        macos-x86* | linux-x86* | linux-i686*)
            arch="x86_64"
            ;;
        *)
            echo "Unsupported platform or architecture"
            exit 1
            ;;
    esac

    if command -v curl >/dev/null 2>&1; then
        curl () {
            command curl -fL "$@"
        }
    elif command -v wget >/dev/null 2>&1; then
        curl () {
            wget -O- "$@"
        }
    else
        echo "Could not find 'curl' or 'wget' in your path"
        exit 1
    fi

    "$platform" "$@"

    if [ "$(command -v bspterm)" = "$HOME/.local/bin/bspterm" ]; then
        echo "Bspterm has been installed. Run with 'bspterm'"
    else
        echo "To run Bspterm from your terminal, you must add ~/.local/bin to your PATH"
        echo "Run:"

        case "$SHELL" in
            *zsh)
                echo "   echo 'export PATH=\$HOME/.local/bin:\$PATH' >> ~/.zshrc"
                echo "   source ~/.zshrc"
                ;;
            *fish)
                echo "   fish_add_path -U $HOME/.local/bin"
                ;;
            *)
                echo "   echo 'export PATH=\$HOME/.local/bin:\$PATH' >> ~/.bashrc"
                echo "   source ~/.bashrc"
                ;;
        esac

        echo "To run Bspterm now, '~/.local/bin/bspterm'"
    fi
}

linux() {
    if [ -n "${BSPTERM_BUNDLE_PATH:-}" ]; then
        cp "$BSPTERM_BUNDLE_PATH" "$temp/bspterm-linux-$arch.tar.gz"
    else
        echo "Downloading Bspterm version: $BSPTERM_VERSION"
        # Update this URL when hosting is set up
        echo "Error: Download URL not configured"
        exit 1
    fi

    suffix=""
    if [ "$channel" != "stable" ]; then
        suffix="-$channel"
    fi

    appid=""
    case "$channel" in
      stable)
        appid="dev.bspterm.Bspterm"
        ;;
      nightly)
        appid="dev.bspterm.Bspterm-Nightly"
        ;;
      preview)
        appid="dev.bspterm.Bspterm-Preview"
        ;;
      dev)
        appid="dev.bspterm.Bspterm-Dev"
        ;;
      *)
        echo "Unknown release channel: ${channel}. Using stable app ID."
        appid="dev.bspterm.Bspterm"
        ;;
    esac

    # Unpack
    rm -rf "$HOME/.local/bspterm$suffix.app"
    mkdir -p "$HOME/.local/bspterm$suffix.app"
    tar -xzf "$temp/bspterm-linux-$arch.tar.gz" -C "$HOME/.local/"

    # Setup ~/.local directories
    mkdir -p "$HOME/.local/bin" "$HOME/.local/share/applications"

    # Link the binary
    if [ -f "$HOME/.local/bspterm$suffix.app/bin/bspterm" ]; then
        ln -sf "$HOME/.local/bspterm$suffix.app/bin/bspterm" "$HOME/.local/bin/bspterm"
    else
        ln -sf "$HOME/.local/bspterm$suffix.app/bin/cli" "$HOME/.local/bin/bspterm"
    fi

    # Copy .desktop file
    desktop_file_path="$HOME/.local/share/applications/${appid}.desktop"
    cp "$HOME/.local/bspterm$suffix.app/share/applications/bspterm$suffix.desktop" "${desktop_file_path}"
    sed -i "s|Icon=bspterm|Icon=$HOME/.local/bspterm$suffix.app/share/icons/hicolor/512x512/apps/bspterm.png|g" "${desktop_file_path}"
    sed -i "s|Exec=bspterm|Exec=$HOME/.local/bspterm$suffix.app/bin/bspterm|g" "${desktop_file_path}"
}

macos() {
    echo "Downloading Bspterm version: $BSPTERM_VERSION"
    # Update this URL when hosting is set up
    echo "Error: Download URL not configured"
    exit 1
}

main "$@"
