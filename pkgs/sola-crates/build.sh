# vendor/ and vendor.toml, the source replacement that points cargo at it.
mkdir -p /out
cp -a "$SRC/vendor" /out/vendor
sed "s|@VENDOR@|$LIVE/vendor|" "$SRC/vendor.toml" >/out/vendor.toml
