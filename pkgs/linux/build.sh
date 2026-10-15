# The one Oath kernel, built by pkg:llvm. Output: boot/vmlinuz,
# boot/config and lib/modules/<release>/ with depmod's index.
unset CFLAGS CXXFLAGS CPPFLAGS LDFLAGS CC CXX AR RANLIB NM STRIP
tar -xf "$SRC/$SOURCES"
cd linux-$VERSION
for p in "$FILES"/patches/*.patch; do
	echo "patch ${p##*/}"
	patch -p1 -i "$p"
done
# No build user, host, time or counter: the same plan gives the same image.
# The built-in initramfs takes its mtimes from the timestamp through
# busybox `date -d`, which reads this form (and silently uses now if not).
export TZ=UTC KBUILD_BUILD_USER=oath KBUILD_BUILD_HOST=oath KBUILD_BUILD_VERSION=1 \
	KBUILD_BUILD_TIMESTAMP='1970-01-01 00:00:00'
# Host tools link statically: pkg-config --static adds libelf's zlib.
# depmod is busybox's small one; the index is written below.
k() { make ARCH=x86_64 LLVM=1 HOSTPKG_CONFIG='pkg-config --static' DEPMOD=true "$@"; }
k defconfig
scripts/kconfig/merge_config.sh -m .config "$FILES/linux.fragment"
k olddefconfig
k -j"$JOBS" bzImage modules
rel=$(k -s kernelrelease)
k INSTALL_MOD_PATH=/tmp/mods INSTALL_MOD_STRIP=1 modules_install
mkdir -p /out/boot /out/lib/modules
cp arch/x86/boot/bzImage /out/boot/vmlinuz
cp .config /out/boot/config
mod=/out/lib/modules/$rel
mv "/tmp/mods/lib/modules/$rel" "$mod"
rm -f "$mod/build" "$mod/source"

# depmod's index from each module's .modinfo: modules.dep (direct needs;
# PID 1 follows them), modules.alias and modules.softdep.
cd "$mod"
awk '{ n = $0; sub(/.*\//, "", n); sub(/\.ko$/, "", n); gsub(/-/, "_", n); print n, $0 }' \
	modules.order >/tmp/names
: >modules.dep
: >modules.alias
: >modules.softdep
while read -r p; do
	n=${p##*/}
	n=$(echo "${n%.ko}" | tr - _)
	llvm-objcopy -O binary --only-section=.modinfo "$p" /tmp/modinfo
	tr '\0' '\n' </tmp/modinfo >/tmp/modinfo.txt
	needs=
	for d in $(sed -n 's/^depends=//p' /tmp/modinfo.txt | tr , ' '); do
		d=$(echo "$d" | tr - _)
		needs="$needs $(awk -v n="$d" '$1 == n { print $2 }' /tmp/names)"
	done
	echo "$p:$needs" >>modules.dep
	sed -n "s/^alias=\(.*\)/alias \1 $n/p" /tmp/modinfo.txt >>modules.alias
	sed -n "s/^softdep=\(.*\)/softdep $n \1/p" /tmp/modinfo.txt >>modules.softdep
done <modules.order
