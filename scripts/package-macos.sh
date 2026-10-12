#!/bin/sh
# Builds the Mac app: "Light Assist.app", and a zip of it to hand out.
#
#   sh scripts/package-macos.sh                       # signed with this Mac's own Developer ID or Apple
#                                                     # Development identity, if it has one
#   SIGN_IDENTITY="Developer ID Application: …" sh scripts/package-macos.sh
#                                                     # signed for other people's Macs, which then
#                                                     # also needs notarizing by Apple
#   IDENTIFIER=com.example.LightAssist …              # the app's name to macOS, if not the default
#   SKIP_BUILD=1 …                                    # package what target/release already holds
#
# macOS remembers what its user allowed an app (seeing the screen, using the mouse and keyboard) by the
# app's identifier and by who signed it. A build signed for this Mac only has nobody's name on it:
# macOS may not list it under Screen Recording at all, and a rebuild is a different app to it. To try
# the permissions, sign with any identity `security find-identity -v -p codesigning` lists, an
# "Apple Development" one included.
set -eu
cd "$(dirname "$0")/.."

identifier="${IDENTIFIER:-com.light-assist.LightAssist}"
# Signed with the first identity this Mac has for it, unless told which. Without one it is signed for
# this Mac only, which macOS may refuse to list under Screen Recording: say so.
identity="${SIGN_IDENTITY:-$(security find-identity -v -p codesigning 2>/dev/null | sed -n 's/.*"\(Developer ID Application: .*\)"$/\1/p' | head -1)}"
identity="${identity:-$(security find-identity -v -p codesigning 2>/dev/null | sed -n 's/.*"\(Apple Development: .*\)"$/\1/p' | head -1)}"
if [ -z "$identity" ]; then
    identity=-
    echo "No signing identity on this Mac: the app is signed for this Mac only, and macOS may not list it under Screen Recording." >&2
fi

if [ -z "${SKIP_BUILD:-}" ]; then
    pnpm build
    cargo build --release -p light-assist
fi

version=$(sed -n 's/^version = "\(.*\)"/\1/p' apps/agent-rs/Cargo.toml | head -1)
out=target/macos
app="$out/Light Assist.app"
rm -rf "$out"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"

cp target/release/light-assist "$app/Contents/MacOS/light-assist"
sed -e "s/{Identifier}/$identifier/" -e "s/{Version}/$version/g" packaging/macos/Info.plist > "$app/Contents/Info.plist"

# The icon at every size macOS asks for, from the one picture in packaging/macos.
icons="$out/light-assist.iconset"
mkdir -p "$icons"
for size in 16 32 128 256 512; do
    sips -z $size $size packaging/macos/icon.png --out "$icons/icon_${size}x${size}.png" > /dev/null
    sips -z $((size * 2)) $((size * 2)) packaging/macos/icon.png --out "$icons/icon_${size}x${size}@2x.png" > /dev/null
done
iconutil -c icns "$icons" -o "$app/Contents/Resources/light-assist.icns"
rm -rf "$icons"

# The hardened runtime is what notarizing asks for; it means nothing for a build signed for this Mac only.
if [ "$identity" = "-" ]; then
    codesign --force --sign - --identifier "$identifier" "$app"
else
    codesign --force --options runtime --timestamp --sign "$identity" --identifier "$identifier" "$app"
fi
codesign --verify --strict "$app"

zip="$out/LightAssist_${version}_$(uname -m).zip"
ditto -c -k --keepParent "$app" "$zip"
# The same under the name the website's download link asks a release for, whatever the version.
cp "$zip" "$out/LightAssist-macOS.zip"
echo "Signed by: $identity"
echo "Built $app"
echo "      $zip"
