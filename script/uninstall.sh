#!/usr/bin/env sh
set -eu

# Uninstalls Bspterm that was installed using the install.sh script

check_remaining_installations() {
    platform="$(uname -s)"
    if [ "$platform" = "Darwin" ]; then
        # Check for any Bspterm variants in /Applications
        remaining=$(ls -d /Applications/Bspterm*.app 2>/dev/null | wc -l)
        [ "$remaining" -eq 0 ]
    else
        # Check for any Bspterm variants in ~/.local
        remaining=$(ls -d "$HOME/.local/bspterm"*.app 2>/dev/null | wc -l)
        [ "$remaining" -eq 0 ]
    fi
}

prompt_remove_preferences() {
    printf "Do you want to keep your Bspterm preferences? [Y/n] "
    read -r response
    case "$response" in
        [nN]|[nN][oO])
            rm -rf "$HOME/.config/bspterm"
            echo "Preferences removed."
            ;;
        *)
            echo "Preferences kept."
            ;;
    esac
}

main() {
    platform="$(uname -s)"
    channel="${BSPTERM_CHANNEL:-stable}"

    if [ "$platform" = "Darwin" ]; then
        platform="macos"
    elif [ "$platform" = "Linux" ]; then
        platform="linux"
    else
        echo "Unsupported platform $platform"
        exit 1
    fi

    "$platform"

    echo "Bspterm has been uninstalled"
}

linux() {
    suffix=""
    if [ "$channel" != "stable" ]; then
        suffix="-$channel"
    fi

    appid=""
    db_suffix="stable"
    case "$channel" in
      stable)
        appid="dev.bspterm.Bspterm"
        db_suffix="stable"
        ;;
      nightly)
        appid="dev.bspterm.Bspterm-Nightly"
        db_suffix="nightly"
        ;;
      preview)
        appid="dev.bspterm.Bspterm-Preview"
        db_suffix="preview"
        ;;
      dev)
        appid="dev.bspterm.Bspterm-Dev"
        db_suffix="dev"
        ;;
      *)
        echo "Unknown release channel: ${channel}. Using stable app ID."
        appid="dev.bspterm.Bspterm"
        db_suffix="stable"
        ;;
    esac

    # Remove the app directory
    rm -rf "$HOME/.local/bspterm$suffix.app"

    # Remove the binary symlink
    rm -f "$HOME/.local/bin/bspterm"

    # Remove the .desktop file
    rm -f "$HOME/.local/share/applications/${appid}.desktop"

    # Remove the database directory for this channel
    rm -rf "$HOME/.local/share/bspterm/db/0-$db_suffix"

    # Remove socket file
    rm -f "$HOME/.local/share/bspterm/bspterm-$db_suffix.sock"

    # Remove the entire Bspterm directory if no installations remain
    if check_remaining_installations; then
        rm -rf "$HOME/.local/share/bspterm"
        prompt_remove_preferences
    fi

    rm -rf $HOME/.bspterm_server
}

macos() {
    app="Bspterm.app"
    db_suffix="stable"
    app_id="dev.bspterm.Bspterm"
    case "$channel" in
      nightly)
        app="Bspterm Nightly.app"
        db_suffix="nightly"
        app_id="dev.bspterm.Bspterm-Nightly"
        ;;
      preview)
        app="Bspterm Preview.app"
        db_suffix="preview"
        app_id="dev.bspterm.Bspterm-Preview"
        ;;
      dev)
        app="Bspterm Dev.app"
        db_suffix="dev"
        app_id="dev.bspterm.Bspterm-Dev"
        ;;
    esac

    # Remove the app bundle
    if [ -d "/Applications/$app" ]; then
        rm -rf "/Applications/$app"
    fi

    # Remove the binary symlink
    rm -f "$HOME/.local/bin/bspterm"

    # Remove the database directory for this channel
    rm -rf "$HOME/Library/Application Support/Bspterm/db/0-$db_suffix"

    # Remove app-specific files and directories
    rm -rf "$HOME/Library/Application Support/com.apple.sharedfilelist/com.apple.LSSharedFileList.ApplicationRecentDocuments/$app_id.sfl"*
    rm -rf "$HOME/Library/Caches/$app_id"
    rm -rf "$HOME/Library/HTTPStorages/$app_id"
    rm -rf "$HOME/Library/Preferences/$app_id.plist"
    rm -rf "$HOME/Library/Saved Application State/$app_id.savedState"

    # Remove the entire Bspterm directory if no installations remain
    if check_remaining_installations; then
        rm -rf "$HOME/Library/Application Support/Bspterm"
        rm -rf "$HOME/Library/Logs/Bspterm"

        prompt_remove_preferences
    fi

    rm -rf $HOME/.bspterm_server
}

main "$@"
