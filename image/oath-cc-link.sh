#!/bin/sh
# rustc gcc-flavor linker on Oath. Zig cc adds glibc crt; we strip
# rustc's -fuse-ld=lld (Zig 0.16 cannot combine it with a dynamic
# linker) and patchelf the -o output onto pkg:glibc.
zig=/oath/store/pkg/cc/libexec/zig/zig
interp=/oath/store/pkg/glibc/lib/ld-linux-x86-64.so.2
glibc=/oath/store/pkg/glibc/lib
pe=/oath/store/pkg/cc/libexec/patchelf
n=0
out=
prev=
for a in "$@"; do
	case "$a" in
	-fuse-ld=*|*gcc-ld) prev=$a; continue ;;
	esac
	if [ "$prev" = "-o" ]; then
		out=$a
	fi
	prev=$a
	n=$((n + 1))
	eval "arg_$n=\$a"
done
i=1
set --
while [ "$i" -le "$n" ]; do
	eval "set -- \"\$@\" \"\$arg_$i\""
	i=$((i + 1))
done
"$zig" cc -target x86_64-linux-gnu "$@"
st=$?
if [ "$st" -eq 0 ] && [ -n "$out" ] && [ -f "$out" ]; then
	"$interp" --library-path "$glibc" "$pe" --set-interpreter "$interp" "$out" || true
	"$interp" --library-path "$glibc" "$pe" --set-rpath "$glibc" "$out" || true
fi
exit "$st"
