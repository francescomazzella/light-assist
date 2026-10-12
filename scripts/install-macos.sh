#!/bin/sh
# Builds Light Assist from this source, signs it with your own certificate, and installs it.
#
#   sh scripts/install-macos.sh               # build, install in Applications, and open it
#   sh scripts/install-macos.sh --build-only  # build only: target/macos/Light Assist.app
#   LIGHT_ASSIST_LAN_ONLY=1 sh scripts/install-macos.sh
#                                             # an app that never uses the website: the person assisting
#                                             # you then opens the address on your network
#
# Why your own certificate: macOS shows an app the screen, and lets it use the mouse and keyboard, only
# once you have allowed it in System Settings, and it lists there only apps signed by someone. A free
# Apple ID is enough. In Xcode: Settings → Accounts → add your Apple ID → Manage Certificates → + →
# Apple Development. Run this again afterwards.
#
# Needs: the Xcode command line tools, Rust (rustup.rs) and Node.js with pnpm (pnpm.io).
set -eu
cd "$(dirname "$0")/.."

fail() { echo "$*" >&2; exit 1; }
[ "$(uname)" = Darwin ] || fail "This builds the Mac app, on a Mac."
xcode-select -p > /dev/null 2>&1 || fail "The Xcode command line tools are missing: run  xcode-select --install"
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
command -v cargo > /dev/null || fail "Rust is missing: see https://rustup.rs"
command -v node > /dev/null || fail "Node.js is missing: see https://nodejs.org"
command -v pnpm > /dev/null || fail "pnpm is missing: see https://pnpm.io/installation"

# A certificate of yours, as the packaging script will find it.
security find-identity -v -p codesigning 2>/dev/null | grep -q -E '"(Developer ID Application|Apple Development): ' ||
    fail "There is no signing certificate on this Mac. In Xcode: Settings → Accounts → add your Apple ID → Manage Certificates → + → Apple Development. Then run this again."

# The website and relay this app's assistants can use, unless it is to stay on the local network.
if [ -z "${LIGHT_ASSIST_LAN_ONLY:-}" ]; then
    export LIGHT_ASSIST_RELAY_URL="${LIGHT_ASSIST_RELAY_URL:-wss://light-assist-relay.francescomazzella.workers.dev}"
    export LIGHT_ASSIST_PAGE_URL="${LIGHT_ASSIST_PAGE_URL:-https://assist.mazzella.dev}"
fi

pnpm install --frozen-lockfile
sh scripts/package-macos.sh
built="target/macos/Light Assist.app"
[ "${1:-}" = --build-only ] && exit 0

# Into the Applications folder, or your own if that one is not yours to write in.
folder=/Applications
[ -w "$folder" ] || { folder="$HOME/Applications"; mkdir -p "$folder"; }
installed="$folder/Light Assist.app"

# What macOS knows the app by: its identifier and its signer. Allowed permissions carry over to a copy
# signed the same way; one signed otherwise needs them allowed again, and the old entries only get in
# the way of that.
signed_by() { codesign -d -r- "$1" 2>/dev/null | sed -n 's/.*designated => //p'; }
before=""
[ -d "$installed" ] && before=$(signed_by "$installed")

pkill -f "Light Assist.app/Contents/MacOS/light-assist" 2> /dev/null && sleep 1 || true
rm -rf "$installed"
ditto "$built" "$installed"

if [ "$before" != "$(signed_by "$installed")" ]; then
    identifier=$(defaults read "$installed/Contents/Info" CFBundleIdentifier)
    tccutil reset ScreenCapture "$identifier" > /dev/null 2>&1 || true
    tccutil reset Accessibility "$identifier" > /dev/null 2>&1 || true
    echo "Allow it in System Settings → Privacy & Security, under Screen & System Audio Recording (then quit"
    echo "Light Assist and open it again) and, when someone first asks to use your mouse and keyboard, Accessibility."
fi

open "$installed"
echo "Installed $installed"
