# Output: ssl/cert.pem, from certdata.txt in NSS's tree.
mkdir -p /out/ssl
tar -xf "$SRC/$SOURCES" "nss-$VERSION/nss/lib/ckfw/builtins/certdata.txt"
python3 "$FILES/certdata2pem.py" "nss-$VERSION/nss/lib/ckfw/builtins/certdata.txt" \
	"$FILES/distrust.txt" /out/ssl/cert.pem
