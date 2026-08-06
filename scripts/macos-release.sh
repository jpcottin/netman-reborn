#!/bin/sh
# Construit, signe, notarise et staple le paquet d'installation macOS.
#
# Le paquet pose le binaire dans /usr/local/bin, le frontend dans
# /usr/local/share/netman/static — recompilé pour en faire le défaut de
# --static-dir, comme le port FreeBSD — et la page de manuel netman(1).
#
# Prérequis (une fois) :
#   - certificats « Developer ID Application » et « Developer ID Installer »
#     dans le trousseau (Xcode → Settings → Accounts → Manage Certificates,
#     en tant qu'Account Holder) ;
#   - identifiants de notarisation enregistrés sous le profil $PROFILE :
#     xcrun notarytool store-credentials netman-notary \
#         --apple-id <apple-id> --team-id <team-id> \
#         --password <mot-de-passe-d-application>
#
# Usage : scripts/macos-release.sh [version]
# La version par défaut est lue dans Cargo.toml ; le script s'exécute depuis
# la racine du dépôt, sur l'arbre de travail courant (se placer sur le tag).

set -eu

VERSION="${1:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)}"
ARCH="$(uname -m)"
IDENTIFIER="net.fenyo.netman"
PROFILE="netman-notary"
APP_ID="Developer ID Application"     # complété par le trousseau
INST_ID="Developer ID Installer"
STATIC_DEFAULT="/usr/local/share/netman/static"
PKG="netman-reborn-v${VERSION}-macos-${ARCH}.pkg"
ROOT="$(mktemp -d)"
trap 'rm -rf "$ROOT"' EXIT

# Binaire avec le frontend installé pour défaut, puis arbre restauré.
sed -i '' "s|default_value = \"static\"|default_value = \"$STATIC_DEFAULT\"|" src/main.rs
cargo build --release
git checkout -- src/main.rs

mkdir -p "$ROOT/usr/local/bin" "$ROOT/usr/local/share/netman" \
         "$ROOT/usr/local/share/man/man1"
install -m 755 target/release/netman "$ROOT/usr/local/bin/"
cp -R static "$ROOT/usr/local/share/netman/"
install -m 644 netman.1 "$ROOT/usr/local/share/man/man1/"

# Signature du binaire (runtime durci, exigé par la notarisation), puis du
# paquet, notarisation et agrafage du ticket pour une installation hors ligne.
codesign --force --options runtime --timestamp \
    --sign "$APP_ID" "$ROOT/usr/local/bin/netman"
pkgbuild --root "$ROOT" --install-location / \
    --identifier "$IDENTIFIER" --version "$VERSION" \
    --sign "$INST_ID" "$PKG"
xcrun notarytool submit "$PKG" --keychain-profile "$PROFILE" --wait
xcrun stapler staple "$PKG"

# Vérifications : Gatekeeper accepte le paquet, signature et ticket en place.
spctl -a -vv -t install "$PKG"
pkgutil --check-signature "$PKG"
echo "OK: $PKG"
