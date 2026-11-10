# Output: bin/, lib/ (libmysqlclient), its plugins and private libraries in
# lib/mysql, include/ and share/. /lib links lib/ entry by entry, so nothing of
# the server's goes there by its upstream name (lib/plugin). LOAD DATA and INTO
# OUTFILE are off until a server's secure_file_priv names a directory.
tar -xf "$SRC/$SOURCES" --no-same-owner
cd "mysql-$VERSION"
for p in "$FILES"/patches/*.patch; do
	patch -p1 -i "$p"
done
# The sandbox's flags put every pack's headers first, ahead of what MySQL
# bundles. What it takes from Oath is named below; only <term.h> needs a path.
unset CPPFLAGS LDFLAGS
n=/oath/store/pkg/ncurses/live/include
CFLAGS="-O2 -pipe -I$n -I$n/ncursesw" CXXFLAGS="$CFLAGS"
cmake -S . -B /tmp/b -DCMAKE_INSTALL_PREFIX="$LIVE" -DCMAKE_BUILD_TYPE=Release \
	-DCMAKE_PREFIX_PATH="/oath/store/pkg/icu/live;/oath/store/pkg/ncurses/live;/oath/store/pkg/zlib/live;/oath/store/pkg/zstd/live" \
	-DINSTALL_LIBDIR="$LIBDIR" -DINSTALL_PLUGINDIR="$LIBDIR/mysql/plugin" \
	-DINSTALL_PRIV_LIBDIR="$LIBDIR/mysql/private" -DINSTALL_MYSQLTESTDIR= \
	-DINSTALL_MYSQLSHAREDIR=share/mysql -DINSTALL_SUPPORTFILESDIR=share/mysql/support-files \
	-DINSTALL_DOCDIR=share/doc/mysql -DINSTALL_DOCREADMEDIR=share/doc/mysql -DINSTALL_MANDIR=share/man \
	-DINSTALL_SECURE_FILE_PRIVDIR=NULL \
	-DWITH_SSL=system -DOPENSSL_ROOT_DIR=/oath/store/pkg/openssl/live -DWITH_ICU=system -DWITH_ZLIB=system -DWITH_ZSTD=system \
	-DWITH_EDITLINE=bundled -DCURSES_INCLUDE_PATH=/oath/store/pkg/ncurses/live/include/ncursesw \
	-DCURSES_LIBRARY=/oath/store/pkg/ncurses/live/lib/libncursesw.so -DWITH_LZ4=bundled -DWITH_PROTOBUF=bundled -DWITH_TIRPC=bundled \
	-DWITH_FIDO=none -DWITH_CURL=none -DWITH_ROUTER=OFF -DWITH_NDB=OFF -DWITH_NDBCLUSTER_STORAGE_ENGINE=OFF -DWITH_GROUP_REPLICATION=OFF -DWITH_UNIT_TESTS=OFF \
	-DWITH_AUTHENTICATION_LDAP=OFF -DWITH_AUTHENTICATION_KERBEROS=OFF -DWITH_AUTHENTICATION_WEBAUTHN=OFF \
	-DWITH_AUTHENTICATION_CLIENT_PLUGINS=OFF
cmake --build /tmp/b -j"$JOBS"
DESTDIR=/tmp/dest cmake --install /tmp/b
cp -a "/tmp/dest$LIVE/." /out/
rm -f "/out/$LIBDIR"/*.a
# Test programs and plugins, some of them test logins: MySQL's Test component
# names each, and its own packages ship them apart. Two scripts need a Perl at
# /usr/bin, which Oath does not have.
DESTDIR=/tmp/t cmake --install /tmp/b --component Test
(cd "/tmp/t$LIVE" && find . -type f -o -type l) | (cd /out && xargs -r rm -f --)
rm -f /out/bin/mysqld_multi /out/bin/mysqldumpslow

# What it bundles, by the versions in their directories' names.
{
	printf '[\n  {"name": "mysql", "version": "%s", "purl": "pkg:generic/mysql@%s"}' "$VERSION" "$VERSION"
	for x in abseil:abseil-cpp- boost:boost_ libedit:libedit- lz4:lz4- protobuf:protobuf- \
		unordered_dense:unordered_dense- xxhash:xxHash-; do
		n=${x%%:*} p=${x#*:}
		v=$(basename extra/"$n"/"$p"*)
		v=$(echo "${v#"$p"}" | tr _ .)
		printf ',\n  {"name": "%s", "version": "%s", "purl": "pkg:generic/%s@%s"}' "$n" "$v" "$n" "$v"
	done
	printf '\n]\n'
} >/out/components.json

# It runs: a server of its own in /tmp, reached by its own client.
export LD_LIBRARY_PATH="/out/$LIBDIR:/out/$LIBDIR/mysql/private"
/out/bin/mysqld --version | grep -qF " $VERSION "
/out/bin/mysql --version | grep -qF " $VERSION "
/out/bin/mysqld --no-defaults --initialize-insecure --user=root --datadir=/tmp/data \
	--lc-messages-dir=/out/share/mysql
/out/bin/mysqld --no-defaults --user=root --datadir=/tmp/data --socket=/tmp/s --port=0 \
	--skip-networking --mysqlx=OFF --lc-messages-dir=/out/share/mysql \
	--plugin-dir="/out/$LIBDIR/mysql/plugin" >/tmp/server.log 2>&1 &
for _ in $(seq 60); do
	/out/bin/mysqladmin --no-defaults -uroot -S /tmp/s ping >/dev/null 2>&1 && break
	sleep 1
done
/out/bin/mysql --no-defaults -uroot -S /tmp/s -Ne 'SELECT VERSION()' | grep -qx "$VERSION"
/out/bin/mysqladmin --no-defaults -uroot -S /tmp/s shutdown
wait
