# The vendor's binary in libexec/, and bin/agy, which keeps it from
# updating itself: the store holds the version, and a newer one is a new
# pin in pkgs/antigravity-cli. Its updater's only switch is
# AGY_CLI_DISABLE_AUTO_UPDATE, read as the exact string "true". It is a
# static Go program, so its TLS roots are pkg:certs' unless SSL_CERT_FILE
# names others: there is no /etc/ssl.
export HOME=/tmp/home
mkdir -p "$HOME" /out/bin /out/libexec/antigravity-cli
tar -xzf "$SRC/agy_cli_linux_x64_musl.tar.gz" --no-same-owner -C /out/libexec/antigravity-cli antigravity
b=/out/libexec/antigravity-cli/antigravity
chmod 755 "$b"
cat >/out/bin/agy <<EOT
#!/bin/sh
# Antigravity CLI $VERSION, built here from its recipe (R31). Updates are a
# new pin in Oath's pkgs/antigravity-cli, so its own updater is off.
: "\${SSL_CERT_FILE:=/oath/store/pkg/certs/live/ssl/cert.pem}"
export SSL_CERT_FILE
AGY_CLI_DISABLE_AUTO_UPDATE=true exec $LIVE/libexec/antigravity-cli/antigravity "\$@"
EOT
chmod 755 /out/bin/agy
AGY_CLI_DISABLE_AUTO_UPDATE=true "$b" --version 2>&1 | grep -qF "$VERSION"
