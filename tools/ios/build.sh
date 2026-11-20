#!/bin/bash
# [design] Local iPad build only. No upload, provisioning changes, or game data bundled.
set -euo pipefail
ROOT="$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
TARGET="${1:-aarch64-apple-ios}"
case "$TARGET" in
    aarch64-apple-ios|aarch64-apple-ios-sim) ;;
    *) echo "Use aarch64-apple-ios or aarch64-apple-ios-sim" >&2; exit 1 ;;
esac
export IPHONEOS_DEPLOYMENT_TARGET=18.0
pushd "$ROOT/engine" >/dev/null
cargo build --release --locked --target "$TARGET" -p nvforever
popd >/dev/null
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/engine/target}"
[[ "$TARGET_DIR" == /* ]] || TARGET_DIR="$ROOT/engine/$TARGET_DIR"
APP="$TARGET_DIR/$TARGET/NVForever.app"
mkdir -p "$APP"
cp "$TARGET_DIR/$TARGET/release/nvforever" "$APP/nvforever"
cp "$ROOT/tools/ios/Info.plist" "$APP/Info.plist"
cp -R "$ROOT/schemas" "$APP/"
cp -R "$ROOT/lang" "$APP/"
ICONS="$(mktemp -d "${TMPDIR:-/tmp/}nvforever-icons.XXXXXX")"
trap 'rm -rf "$ICONS"' EXIT
cp -R "$ROOT/tools/ios/Assets.xcassets" "$ICONS/Assets.xcassets"
xcrun --sdk macosx swift "$ROOT/tools/ios/icons.swift" \
    "$ROOT/engine/nvforever/assets/icon.png" "$ICONS/Assets.xcassets/AppIcon.appiconset"
PLATFORM=iphoneos
[[ "$TARGET" != aarch64-apple-ios-sim ]] || PLATFORM=iphonesimulator
xcrun actool "$ICONS/Assets.xcassets" --compile "$APP" --platform "$PLATFORM" \
    --minimum-deployment-target "$IPHONEOS_DEPLOYMENT_TARGET" --target-device ipad \
    --app-icon AppIcon --output-partial-info-plist "$ICONS/icons.plist"
/usr/libexec/PlistBuddy -c "Merge $ICONS/icons.plist" "$APP/Info.plist"
if [[ "$TARGET" == aarch64-apple-ios-sim ]]; then
    /usr/libexec/PlistBuddy -c "Set :CFBundleSupportedPlatforms:0 iPhoneSimulator" "$APP/Info.plist"
    codesign --force --sign - "$APP"
else
    echo "Unsigned device app: sign with your own development profile before installing."
fi
echo "$APP"
