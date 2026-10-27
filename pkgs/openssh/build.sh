# OpenSSH against pkg:openssl (RSA and ECDSA keys from libcrypto; ed25519,
# chacha20-poly1305 and ML-KEM its own): sshd (svc:sshd, R36), whose
# unauthenticated process runs as void, chrooted in /var/empty, with
# sshd-session and sshd-auth beside it in libexec/; and the client tools,
# ssh-keygen and sftp-server. No PAM, no utmp, wtmp or lastlog. sshd links
# crypt(3), which glibc 2.39 dropped, from libxcrypt.
tar -xf "$SRC/$SOURCES"
cd openssh-$VERSION
./configure --prefix=/ --sysconfdir=/etc/ssh --libexecdir="$LIVE/libexec" \
	--with-privsep-user=void --with-privsep-path=/var/empty --with-pid-dir=/run \
	--disable-strip --without-pam --without-selinux --without-kerberos5 --without-ldns \
	--without-security-key-builtin --with-default-path=/bin --with-superuser-path=/bin \
	--disable-lastlog --disable-utmp --disable-utmpx --disable-wtmp --disable-wtmpx \
	--disable-pututline --disable-pututxline
make -j"$JOBS" sshd sshd-session sshd-auth ssh scp sftp sftp-server ssh-keygen
mkdir -p /out/bin /out/libexec
cp sshd ssh scp sftp sftp-server ssh-keygen /out/bin/
cp sshd-session sshd-auth /out/libexec/
