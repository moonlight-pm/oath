# GNU coreutils, with hostname; kill and uptime are procps-ng's.
# gnulib probes the builder's kernel for fstatat (fd, NULL, AT_EMPTY_PATH)
# (Linux 6.12: no, 7.x: yes); "no" keeps its replacement, which works on
# both, so every builder makes the same coreutils.
tar -xf "$SRC/$SOURCES"
cd "coreutils-$VERSION"
gl_cv_func_fstatat_null_file=no FORCE_UNSAFE_CONFIGURE=1 ./configure --prefix="$LIVE" --disable-nls \
	--enable-install-program=hostname --enable-no-install-program=kill,uptime
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/bin" "/tmp/dest$LIVE/libexec" /out/
