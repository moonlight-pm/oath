# pkg:linux's build tree, for modules built outside it: the same source
# and patches, pkg:linux's exact config, `make modules_prepare` by
# pkg:gcc, and pkg:linux's Module.symvers, so a module built against
# build/ has pkg:linux's vermagic and symbols. Only what kbuild reads for
# an external module stays (as Arch's linux-headers keeps): the Makefiles
# and Kconfig files, include/ and arch/x86's headers, scripts/ with its
# host programs, objtool, .config and Module.symvers.
unset CFLAGS CXXFLAGS CPPFLAGS LDFLAGS CC CXX AR RANLIB NM STRIP
tar -xf "$SRC/$SOURCES"
cd linux-$VERSION
for p in "$FILES"/patches/*.patch; do
	echo "patch ${p##*/}"
	patch -p1 -i "$p"
done
export TZ=UTC KBUILD_BUILD_USER=oath KBUILD_BUILD_HOST=oath KBUILD_BUILD_VERSION=1 \
	KBUILD_BUILD_TIMESTAMP='1970-01-01 00:00:00'
K=/oath/store/pkg/linux/live
k() { make ARCH=x86_64 "$@"; }
cp "$K/boot/config" .config
k olddefconfig
cmp .config "$K/boot/config"
k -j"$JOBS" modules_prepare
cp "$K/boot/Module.symvers" Module.symvers
rel=$(k -s kernelrelease)
[ -d "$K/lib/modules/$rel" ]

b=/out/build
mkdir -p "$b/arch/x86/kernel" "$b/kernel" "$b/tools/objtool"
cp -a .config Makefile Module.symvers "$b/"
cp -a kernel/Makefile "$b/kernel/"
cp -a arch/x86/Makefile* "$b/arch/x86/"
cp -a arch/x86/kernel/asm-offsets.s "$b/arch/x86/kernel/"
cp -a scripts include "$b/"
cp -a arch/x86/include "$b/arch/x86/"
cp -a tools/objtool/objtool "$b/tools/objtool/"
find . -path ./Documentation -prune -o \( -name 'Kconfig*' -o -name 'Kbuild' -o -name 'Makefile*' \) \
	-type f -print | while read -r f; do
	case $f in ./arch/*) case $f in ./arch/x86/* | ./arch/Kconfig) ;; *) continue ;; esac ;; esac
	[ -e "$b/$f" ] || { mkdir -p "$b/${f%/*}"; cp -a "$f" "$b/$f"; }
done
find "$b" -name '*.o' -delete
find "$b" -name '*.cmd' -delete
find -L "$b" -type l -delete
echo "$rel" >"$b/kernelrelease"
