#!/usr/bin/env bash
# Relocate a glibc Git (HTTPS helper + ELF deps) into $out for pkg:git.
# Guest rpath includes pkg:glibc. Not packed into pkg:sola.
set -euo pipefail

out=${1:?out}
guest_glibc=/oath/store/pkg/glibc/lib
guest_git=/oath/store/pkg/git/lib
rpath="$guest_glibc:$guest_git"
interp_guest="$guest_glibc/ld-linux-x86-64.so.2"
GIT=${GIT:?}

if [[ -e $out ]]; then
	chmod -R u+w "$out" 2>/dev/null || true
	rm -rf "$out"
fi
mkdir -p "$out/bin" "$out/lib" "$out/libexec/git-core" "$out/share/git-core" "$out/ssl"

is_glibc() {
	case "$(basename "$1")" in
	ld-linux*|libc.so*|libm.so*|libdl.so*|libpthread.so*|librt.so*| \
	libgcc_s.so*|libstdc++.so*)
		return 0
		;;
	*) return 1 ;;
	esac
}

declare -A SEEN=()
declare -A SONAME=()
queue=()
loader=""

enqueue() {
	local f=$1
	[[ -e $f ]] || return 0
	local real
	real=$(readlink -f "$f")
	[[ -n ${SEEN[$real]+x} ]] && return 0
	SEEN[$real]=1
	SONAME[$real]=$(basename "$f")
	queue+=("$real")
}

cp -a "$GIT/libexec/git-core/." "$out/libexec/git-core/"
chmod -R u+w "$out/libexec/git-core"
if [[ -d $GIT/share/git-core/templates ]]; then
	cp -a "$GIT/share/git-core/templates" "$out/share/git-core/templates"
	chmod -R u+w "$out/share/git-core/templates"
fi

# Perl/python helpers need interpreters we do not ship.
rm -f "$out/libexec/git-core"/git-cvsserver \
	"$out/libexec/git-core"/git-cvsimport \
	"$out/libexec/git-core"/git-cvsexportcommit \
	"$out/libexec/git-core"/git-archimport \
	"$out/libexec/git-core"/git-p4 \
	"$out/libexec/git-core"/git-instaweb \
	"$out/libexec/git-core"/git-send-email \
	"$out/libexec/git-core"/git-add--interactive

while IFS= read -r -d '' f; do
	if file -b "$f" | grep -q ELF; then
		enqueue "$f"
	fi
	if head -n1 "$f" 2>/dev/null | grep -q '^#!/.*/nix/store/'; then
		sed -i '1s|^#!.*|#!/bin/sh|' "$f"
	fi
done < <(find "$out/libexec/git-core" -type f -print0)

i=0
while [[ $i -lt ${#queue[@]} ]]; do
	f=${queue[$i]}
	i=$((i + 1))
	if [[ -z $loader ]] && file -b "$f" | grep -q 'ELF'; then
		if loader=$(patchelf --print-interpreter "$f" 2>/dev/null); then
			enqueue "$loader"
		else
			loader=""
		fi
	fi
	if [[ -n $loader && -x $loader ]]; then
		while read -r dep; do
			[[ -e $dep ]] && enqueue "$dep"
		done < <("$loader" --list "$f" 2>/dev/null | awk '/=> \// {print $3} /^\//{print $1}')
	fi
done

for f in "${!SEEN[@]}"; do
	name=${SONAME[$f]:-$(basename "$f")}
	if is_glibc "$name"; then
		continue
	fi
	# Helpers already copied into libexec.
	if [[ $f == "$out"/libexec/git-core/* ]]; then
		continue
	fi
	d="$out/lib/$name"
	mkdir -p "$(dirname "$d")"
	cp -a "$f" "$d"
	chmod u+w "$d" 2>/dev/null || true
done

find "$out/libexec/git-core" "$out/lib" -type f | while read -r f; do
	file -b "$f" | grep -q ELF || continue
	chmod u+w "$f" || true
	if patchelf --print-interpreter "$f" >/dev/null 2>&1; then
		patchelf --set-interpreter "$interp_guest" "$f" || true
	fi
	patchelf --set-rpath "$rpath" "$f" 2>/dev/null || true
done

cat >"$out/bin/git" <<'WRAP'
#!/bin/sh
export GIT_EXEC_PATH=/oath/store/pkg/git/libexec/git-core
export GIT_TEMPLATE_DIR=/oath/store/pkg/git/share/git-core/templates
export GIT_SSL_CAINFO="${GIT_SSL_CAINFO:-/oath/store/pkg/git/ssl/cert.pem}"
export SSL_CERT_FILE="${SSL_CERT_FILE:-/oath/store/pkg/git/ssl/cert.pem}"
exec /oath/store/pkg/git/libexec/git-core/git "$@"
WRAP
chmod +x "$out/bin/git"
