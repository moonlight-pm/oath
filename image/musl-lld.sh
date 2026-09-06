#!/bin/sh
# rustc musl linker on Oath. Zig cc injects crt1.o and collides with
# rust-std rcrt1.o. Drive rust-lld; unwrap gcc -Wl,* flags.
interp=/oath/store/pkg/glibc/lib/ld-linux-x86-64.so.2
glibc=/oath/store/pkg/glibc/lib
rlib=/oath/store/pkg/rustc/lib
zlib=/oath/store/pkg/git/lib
real=/oath/store/pkg/rustc/lib/rustlib/x86_64-unknown-linux-gnu/bin/rust-lld
n=0
for a in "$@"; do
	case "$a" in
	-fuse-ld=*|*gcc-ld*|-B*|-nostartfiles|-nodefaultlibs|-m64|-static-pie|-static) continue ;;
	-Wl,*)
		a=${a#-Wl,}
		IFS=,
		# shellcheck disable=SC2086
		for p in $a; do
			n=$((n + 1))
			eval "arg_$n=\$p"
		done
		unset IFS
		continue
		;;
	esac
	n=$((n + 1))
	eval "arg_$n=\$a"
done
i=1
set --
while [ "$i" -le "$n" ]; do
	eval "set -- \"\$@\" \"\$arg_$i\""
	i=$((i + 1))
done
exec "$interp" --library-path "$rlib:$glibc:$zlib" "$real" \
	-flavor gnu -static -m elf_x86_64 "$@"
