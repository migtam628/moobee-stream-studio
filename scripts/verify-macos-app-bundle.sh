#!/usr/bin/env bash
# Validate the built macOS app, not merely the Tauri JSON source.
# All failures print the observed metadata so CI errors are actionable.
set -euo pipefail

APP_PATH="${1:-apps/desktop/src-tauri/target/release/bundle/macos/MooBee Stream Studio.app}"
PLIST="$APP_PATH/Contents/Info.plist"

die() {
  printf '::error::macOS bundle verification failed: %s\n' "$*" >&2
  exit 1
}

[[ -d "$APP_PATH" ]] || die "Application bundle not found: $APP_PATH"
[[ -s "$PLIST" ]] || die "Info.plist missing: $PLIST"
plutil -lint "$PLIST" || die "Invalid Info.plist"

PLIST_MIN="$(/usr/libexec/PlistBuddy -c 'Print :LSMinimumSystemVersion' "$PLIST")" ||
  die "LSMinimumSystemVersion is missing"
echo "Info.plist LSMinimumSystemVersion: $PLIST_MIN"
[[ "$PLIST_MIN" == "13.0" ]] ||
  die "Expected macOS 13.0 deployment minimum, got: $PLIST_MIN"

for usage in NSScreenCaptureUsageDescription NSMicrophoneUsageDescription NSCameraUsageDescription; do
  value="$(/usr/libexec/PlistBuddy -c "Print :$usage" "$PLIST")" ||
    die "Missing $usage"
  [[ -n "$value" ]] || die "$usage is empty"
  echo "Info.plist $usage: present"
done

EXECUTABLE_NAME="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$PLIST")" ||
  die "CFBundleExecutable is missing"
[[ -n "$EXECUTABLE_NAME" ]] || die "CFBundleExecutable is empty"
EXECUTABLE="$APP_PATH/Contents/MacOS/$EXECUTABLE_NAME"
[[ -x "$EXECUTABLE" ]] || die "Cannot execute $EXECUTABLE"

COMMANDS="$(mktemp)"
trap 'rm -f "$COMMANDS"' EXIT
otool -l "$EXECUTABLE" > "$COMMANDS" ||
  die "Cannot read Mach-O load commands"

# Read the *actual* executable deployment minimum. No fragile grep -A pipes
# (which can fail with pipefail/SIGPIPE or whitespace-dependent regexes).
MACH_MIN="$(awk '
  $1 == "cmd" && $2 == "LC_BUILD_VERSION" { mode = "build"; next }
  $1 == "cmd" && $2 == "LC_VERSION_MIN_MACOSX" { mode = "legacy"; next }
  mode == "build" && $1 == "minos" { print $2; exit }
  mode == "legacy" && $1 == "version" { print $2; exit }
' "$COMMANDS")"
echo "Mach-O deployment minimum: ${MACH_MIN:-not found}"
[[ -n "$MACH_MIN" ]] ||
  die "Missing LC_BUILD_VERSION/LC_VERSION_MIN_MACOSX minimum"

# The binary can target an earlier system than the bundle declares, never a
# later one. For example 12.0 is fine with Info.plist minimum 13.0.
if ! awk -v value="$MACH_MIN" 'BEGIN {
    count = split(value, parts, ".")
    if (count < 2 || parts[1] !~ /^[0-9]+$/ || parts[2] !~ /^[0-9]+$/) exit 1
    if ((parts[1]+0)*1000+(parts[2]+0) > 13000) exit 1
  }'; then
  die "Mach-O requires macOS $MACH_MIN, newer than the app minimum 13.0"
fi

if ! grep -F 'path /usr/lib/swift' "$COMMANDS" >/dev/null; then
  echo "::warning::System Swift runtime rpath not found. Check linking flags."
  die "Missing /usr/lib/swift runtime search path"
fi
echo "System Swift rpath: present"
echo "macOS bundle compatibility metadata verified (macOS 13+ shell, MP4 requires 15+)."
