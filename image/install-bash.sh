#!/bin/sh
# Live-install pkg:bash on this Oath box.
set -eu

here=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
root=$(CDPATH= cd -- "$here/.." && pwd)
fetchdir=${OATH_FETCH:-$root/build/fetch}
stagedir=${OATH_STAGE:-$root/build/arcade-stage}
store=/oath/store/pkg
objects=/oath/objects/pkg

mkdir -p "$fetchdir" "$stagedir"
export OATH_FETCH=$fetchdir

if [ -x /tmp/bash-linux-x86_64 ]; then
	export OATH_BASH_ELF=/tmp/bash-linux-x86_64
fi
echo "==> pack bash"
sh "$here/pack-bash.sh" "$stagedir/bash"

as_root() {
	if [ "$(id -u)" = 0 ]; then
		"$@"
	else
		sudo -n "$@"
	fi
}

write_obj() {
	name=$1
	dir=$objects/$name
	as_root mkdir -p "$dir"
	printf '%s\n' '{
  "present": true
}' | as_root tee "$dir/desired.json" >/dev/null
	printf '%s\n' '{
  "present": false,
  "links": [],
  "removable": true
}' | as_root tee "$dir/actual.json" >/dev/null
	printf '%s\n' "{
  \"id\": \"pkg:$name\",
  \"kind\": \"pkg\",
  \"name\": \"$name\",
  \"safety\": \"mutate\",
  \"status\": \"drift\"
}" | as_root tee "$dir/meta.json" >/dev/null
}

echo "==> install pkg:bash"
as_root rm -rf "$store/bash"
as_root mkdir -p "$store"
as_root cp -a "$stagedir/bash" "$store/bash"
as_root chmod -R u+rX "$store/bash"
write_obj bash
# Live /bin/bash may be a regular ELF from the one-shot fetch; apply
# refuses to clobber a name it does not own.
if [ -e /bin/bash ] && [ ! -L /bin/bash ]; then
	as_root rm -f /bin/bash
fi
if [ "$(id -u)" = 0 ]; then
	oath apply pkg:bash
else
	sudo -n oath apply pkg:bash
fi
echo "==> courage"
test -x /bin/bash
readlink /bin/bash
/bin/bash --version | head -n1
