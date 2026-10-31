# The vendor's static gh in libexec/, and bin/gh, which turns gh's update
# notice off (the store holds the version, and a newer one is a new pin
# in pkgs/gh), gives it curl's CA bundle when the machine names none, and
# names itself by its slot path (GH_PATH), so the git credential helper
# `gh auth setup-git` writes outlives the next update.
export HOME=/tmp/home
mkdir -p "$HOME" /out/bin /out/libexec/gh /out/share/doc/gh
tar -xzf "$SRC/gh_${VERSION}_linux_amd64.tar.gz" --no-same-owner
d=gh_${VERSION}_linux_amd64
cp "$d/bin/gh" /out/libexec/gh/gh
cp "$d/LICENSE" /out/share/doc/gh/
cat >/out/bin/gh <<EOT
#!/bin/sh
# GitHub CLI $VERSION, the vendor's build (R7). Updates are a new pin in
# Oath's pkgs/gh, so gh's update notice is off.
export GH_NO_UPDATE_NOTIFIER=1 GH_PATH=$LIVE/bin/gh
export SSL_CERT_FILE="\${SSL_CERT_FILE:-/oath/store/pkg/curl/live/ssl/cert.pem}"
exec $LIVE/libexec/gh/gh "\$@"
EOT
chmod 755 /out/bin/gh
/out/libexec/gh/gh --version | grep -q "^gh version $VERSION "
