# GNU patch. 2.8.1's gnulib probes the builder's kernel for fstatat (fd,
# NULL, AT_EMPTY_PATH) as coreutils' does; "no" makes the same patch on
# every builder.
tar -xf "$SRC/$SOURCES"
cd "patch-$VERSION"
gl_cv_func_fstatat_null_file=no ./configure --prefix="$LIVE" --disable-nls
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/bin" /out/
