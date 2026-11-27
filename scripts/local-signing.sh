#!/bin/sh
# Sténo's local signing identity (self-signed certificate in the login keychain).
#
# Without an Apple developer account, the app used to be signed "ad hoc": macOS then identifies
# the app by the exact hash of the binary, which changes with every build, and asks again for
# microphone, system audio, accessibility and calendar access after each update. Signed with a
# stable certificate, the app keeps the same identity across versions and permissions are kept.
#
# The certificate is created once, on this Mac, and is only used to sign apps built here
# (it never leaves the keychain). To remove it: Keychain Access →
# "Steno Local Signing" → Delete.
#
# Prints the identity name to use with `codesign -s`, or fails (ad hoc signing then takes
# over).
set -eu
NAME="Steno Local Signing"
KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"

if security find-certificate -c "$NAME" "$KEYCHAIN" >/dev/null 2>&1; then
  echo "$NAME"
  exit 0
fi

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
cat > "$TMP/cert.cnf" <<CNF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $NAME
[ext]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
CNF
/usr/bin/openssl req -x509 -newkey rsa:2048 -nodes -days 3650 \
  -config "$TMP/cert.cnf" -keyout "$TMP/key.pem" -out "$TMP/cert.pem" >/dev/null 2>&1
PASS=$(/usr/bin/openssl rand -hex 16)
/usr/bin/openssl pkcs12 -export -inkey "$TMP/key.pem" -in "$TMP/cert.pem" \
  -name "$NAME" -out "$TMP/identity.p12" -passout "pass:$PASS" >/dev/null 2>&1
# -T: codesign can use the key without asking for access on every build.
security import "$TMP/identity.p12" -k "$KEYCHAIN" -P "$PASS" -T /usr/bin/codesign >/dev/null
echo "Local signing identity created: $NAME (login keychain)." >&2
echo "$NAME"
