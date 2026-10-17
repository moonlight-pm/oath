# One perl binary with every core XS module linked in (no dynamic
# loading). It finds its library under the slot path. Configure records
# who, where and when it ran; those are fixed so the same plan gives the
# same product.
tar -xf "$SRC/$SOURCES"
cd perl-$VERSION
sh Configure -des -Dprefix="$LIVE" -Dcc=cc -Dld=cc -Dar=ar -Dnm=nm \
	-Doptimize=-O2 -Uusedl -Dusenm=false \
	-Dman1dir=none -Dman3dir=none \
	-Dcf_by=oath -Dcf_email=oath@oath -Dperladmin=oath@oath \
	-Dmyhostname=oath -Dmydomain=.oath -Dmyuname=oath -Dosvers=oath
# Configure always records the time it ran; set it in config.sh and
# regenerate what reads it (config.h, Config, perlbug).
t='Thu Jan  1 00:00:00 UTC 1970'
sed -i -e "s/^cf_time=.*/cf_time='$t'/" \
	-e "s/^# Configuration time: .*/# Configuration time: $t/" config.sh
sh Configure -S
make -j"$JOBS"
make install DESTDIR=/tmp/dest
# Config records the compiler's search paths (for building XS modules):
# name them by the compiler's slot, not its hash, so this product does
# not change with every pkg:gcc or pkg:glibc.
for f in "/tmp/dest$LIVE"/lib/*/*/Config.pm "/tmp/dest$LIVE"/lib/*/*/Config_heavy.pl; do
	sed -i -e 's|/oath/store/pkg/gcc/sha256-[0-9a-f]*/|/oath/store/pkg/gcc/live/|g' \
		-e 's|/oath/store/pkg/glibc/sha256-[0-9a-f]*/|/oath/store/pkg/glibc/live/|g' "$f"
done
cp -a "/tmp/dest$LIVE/bin" "/tmp/dest$LIVE/lib" /out/
