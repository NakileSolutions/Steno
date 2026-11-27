#!/bin/sh
# Identité de signature locale de Sténo (certificat auto-signé dans le trousseau de session).
#
# Sans compte développeur Apple, l'app était signée « ad hoc » : macOS reconnaît alors l'app
# à l'empreinte exacte du binaire, qui change à chaque compilation, et redemande micro, audio
# système, accessibilité et calendriers après chaque mise à jour. Signée avec un certificat
# stable, l'app garde la même identité d'une version à l'autre et les autorisations restent.
#
# Le certificat est créé une seule fois, sur ce Mac, et ne sert qu'à signer les apps
# compilées ici (il ne quitte jamais le trousseau). Pour le retirer : Trousseaux d'accès →
# « Steno Local Signing » → Supprimer.
#
# Affiche le nom de l'identité à utiliser avec `codesign -s`, ou échoue (la signature ad hoc
# prend alors le relais).
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
# -T : codesign peut utiliser la clé sans demander l'accès à chaque compilation.
security import "$TMP/identity.p12" -k "$KEYCHAIN" -P "$PASS" -T /usr/bin/codesign >/dev/null
echo "Identité de signature locale créée : $NAME (trousseau de session)." >&2
echo "$NAME"
