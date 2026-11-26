# Shared, with headers and pkg-config; no samples, tests or tools' man pages.
# patches/: Oath's backport for CVE-2026-137326, a clone of the x11-compound-text
# converter sharing its original's private data (the patch's header).
tar -xf "$SRC/$SOURCES"
cd icu/source
for p in "$FILES"/patches/*.patch; do
	patch -p1 -i "$p"
done
./configure --prefix="$LIVE" --libdir="$LIVE/$LIBDIR" --disable-static --disable-samples \
	--disable-tests --disable-extras --disable-layoutex
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/." /out/
rm -rf /out/share/man "/out/$LIBDIR/icu"
# The backport: a clone outlives its original, through a flush of the converter cache,
# converts as the original did, and closes without a double free.
cat >/tmp/ct.c <<'EOF'
#include <string.h>
#include <unicode/ucnv.h>
int main(void) {
	static const UChar thai[] = { 0x0e01, 0x0e02 };
	char a[16], b[16];
	UErrorCode e = U_ZERO_ERROR;
	UConverter *c = ucnv_open("x11-compound-text", &e);
	UConverter *d = ucnv_clone(c, &e);
	int32_t n = ucnv_fromUChars(c, a, (int32_t)sizeof a, thai, 2, &e);
	ucnv_close(c);
	ucnv_flushCache();
	int32_t m = ucnv_fromUChars(d, b, (int32_t)sizeof b, thai, 2, &e);
	ucnv_close(d);
	return U_FAILURE(e) || n <= 0 || n != m || memcmp(a, b, (size_t)n) != 0;
}
EOF
cc -I/out/include /tmp/ct.c -o /tmp/ct -L"/out/$LIBDIR" -licuuc -licudata
LD_LIBRARY_PATH="/out/$LIBDIR" /tmp/ct
