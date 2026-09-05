#!/bin/sh
# Zig cc/c++ for glibc on Oath. rustc 1.98 passes -fuse-ld=lld and
# -B…/gcc-ld; Zig 0.16 cannot combine those with --dynamic-linker
# (ObjectFilesCannotSpecifyDynamicLinker /
# LldCannotSpecifyDynamicLinkerForSharedLibraries). Drop them; Zig
# links with its own lld and the interp/rpath we pass.
cmd=${1:-cc}
shift
zig=/oath/store/pkg/cc/libexec/zig/zig
interp=/oath/store/pkg/glibc/lib/ld-linux-x86-64.so.2
rpath=/oath/store/pkg/glibc/lib
n=0
link=1
for a in "$@"; do
	case "$a" in
	-fuse-ld=*|*gcc-ld|--target=*) continue ;;
	-c|-S|-E) link=0 ;;
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
if [ "$link" -eq 1 ]; then
	exec "$zig" "$cmd" -target x86_64-linux-gnu \
		-Wl,--dynamic-linker="$interp" \
		-Wl,-rpath,"$rpath" \
		"$@"
fi
exec "$zig" "$cmd" -target x86_64-linux-gnu "$@"
