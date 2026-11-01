# Output: ssl/cert.pem, Mozilla's list as curl.se publishes it, without the
# roots distrust.txt names. The fingerprint is taken from each certificate,
# not from the comment beside it.
mkdir -p /out/ssl /tmp/c
awk -v RS= '{ print > sprintf("/tmp/c/%04d", NR) }' "$SRC/cacert-$VERSION.pem"
grep -v '^#' "$FILES/distrust.txt" | cut -d' ' -f1 >/tmp/distrust
if grep -qvE '^[0-9a-f]{64}$' /tmp/distrust; then
	echo "distrust.txt: not a fingerprint" >&2
	exit 1
fi
: >/tmp/seen
kept=0
for f in /tmp/c/*; do
	if grep -q -e '-----BEGIN CERTIFICATE-----' "$f"; then
		fp=$(sed -e '1,/-----BEGIN/d' -e '/-----END/,$d' "$f" | base64 -d | sha256sum | cut -d' ' -f1)
		[ ${#fp} = 64 ]
		if grep -qx "$fp" /tmp/distrust; then
			echo "$fp" >>/tmp/seen
			echo "== distrusted: $(head -n 1 "$f")" >&2
			continue
		fi
		kept=$((kept + 1))
	fi
	cat "$f"
	echo
done >/tmp/cert.pem
{
	echo "## Oath's CA roots (pkg:certs $VERSION): $kept of Mozilla's, less pkgs/certs/distrust.txt."
	echo
	cat /tmp/cert.pem
} >/out/ssl/cert.pem
while read -r fp; do
	grep -qx "$fp" /tmp/seen || echo "== already gone from upstream: $fp"
done </tmp/distrust

# A list this short is a broken parse.
[ "$kept" -gt 90 ]
[ "$(grep -c -e '-----BEGIN CERTIFICATE-----' /out/ssl/cert.pem)" = "$kept" ]
echo "== $kept roots"
