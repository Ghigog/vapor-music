#!/usr/bin/env bash
#
# Launch the Godot editor with the Android release-signing credentials in the
# environment, so they never get written into export_presets.cfg (which is
# tracked in git).
#
# Godot reads GODOT_ANDROID_KEYSTORE_RELEASE_{PATH,USER,PASSWORD} and they take
# precedence over the preset fields, which stay empty.
#
# One-time setup — store the keystore password in the macOS Keychain:
#   security add-generic-password -s vapor-music-release-keystore -a "$USER" -w
# (it prompts; the password is never echoed and never touches the filesystem)
#
# Then just run this instead of opening Godot from the Dock:
#   ./tools/godot-export.sh

set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
KEYSTORE="$HOME/Library/Application Support/Godot/keystores/vapor_music_release.keystore"
KEY_ALIAS="vapormusic"
KEYCHAIN_SERVICE="vapor-music-release-keystore"
GODOT_BIN="/Applications/Godot.app/Contents/MacOS/Godot"

if [[ ! -x "$GODOT_BIN" ]]; then
	echo "error: Godot not found at $GODOT_BIN" >&2
	exit 1
fi

if [[ ! -f "$KEYSTORE" ]]; then
	echo "error: release keystore not found at:" >&2
	echo "  $KEYSTORE" >&2
	echo "Recreate it with: keytool -v -genkeypair -keyalg RSA -keysize 4096 \\" >&2
	echo "  -validity 10950 -alias $KEY_ALIAS -keystore \"$KEYSTORE\"" >&2
	exit 1
fi

if ! KEYSTORE_PASS="$(security find-generic-password -s "$KEYCHAIN_SERVICE" -w 2>/dev/null)"; then
	cat >&2 <<-EOF
		error: keystore password not in the Keychain.

		Store it once (prompts, nothing is echoed or saved to disk):
		  security add-generic-password -s $KEYCHAIN_SERVICE -a "\$USER" -w
	EOF
	exit 1
fi

export GODOT_ANDROID_KEYSTORE_RELEASE_PATH="$KEYSTORE"
export GODOT_ANDROID_KEYSTORE_RELEASE_USER="$KEY_ALIAS"
export GODOT_ANDROID_KEYSTORE_RELEASE_PASSWORD="$KEYSTORE_PASS"

echo "Release signing loaded from Keychain (alias: $KEY_ALIAS)."
echo "Leave the preset's Keystore fields EMPTY — these override them."

exec "$GODOT_BIN" --path "$PROJECT_DIR" --editor "$@"
