# git and its helpers (libexec/git-core), linked against libcurl for
# https. NO_PERL drops send-email, svn and the cvs commands; NO_PYTHON
# git-p4; NO_TCLTK gitk and git-gui. CFLAGS has no -g, so no debug info
# names the compiler's store path.
tar -xf "$SRC/$SOURCES"
cd "git-$VERSION"
opts="prefix=$LIVE gitexecdir=$LIVE/libexec/git-core NO_GETTEXT=1 NO_TCLTK=1 NO_PYTHON=1 NO_PERL=1"
opts="$opts NO_RUST=1 NO_OPENSSL=1 USE_LIBPCRE2=1 INSTALL_SYMLINKS=1"
opts="$opts CURL_CONFIG=/oath/store/pkg/curl/live/bin/curl-config"
make -j"$JOBS" $opts CFLAGS="$CFLAGS" all
make $opts CFLAGS="$CFLAGS" install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/bin" "/tmp/dest$LIVE/libexec" "/tmp/dest$LIVE/share" /out/
rm -rf /out/share/perl5 /out/share/gitweb
