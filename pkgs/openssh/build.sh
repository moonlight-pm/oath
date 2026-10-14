tar -xf "$SRC/$SOURCES"
cd openssh-$VERSION
./configure --prefix=/ --sysconfdir=/etc/ssh --without-openssl \
	--with-privsep-path=/var/empty --disable-strip --without-pam \
	--without-selinux --without-kerberos5 --without-ldns \
	--without-security-key-builtin --with-default-path=/bin \
	--with-sftp-server=/bin/sftp-server
make -j"$JOBS" ssh scp sftp sftp-server ssh-keygen
mkdir -p /out/bin
cp ssh scp sftp sftp-server ssh-keygen /out/bin/
