#!/bin/sh
# Write pack-root components.json (T44). Args after DEST are
# triples: name version purl. Entries are sorted by purl.
set -eu
out=${1:?out}
shift
if [ $(($# % 3)) -ne 0 ] || [ $# -lt 3 ]; then
	echo "usage: write-components.sh DEST name version purl [name version purl ...]" >&2
	exit 1
fi
list=$out/components.json.list
: >"$list"
while [ $# -ge 3 ]; do
	printf '%s\t%s\t%s\n' "$3" "$1" "$2" >>"$list"
	shift 3
done
tmp=$out/components.json.tmp
{
	printf '%s\n' '{'
	printf '%s\n' '  "components": ['
	first=1
	LC_ALL=C sort -t "$(printf '\t')" -k1,1 "$list" | while IFS="$(printf '\t')" read -r purl name ver; do
		if [ "$first" -eq 1 ]; then
			first=0
		else
			printf ',\n'
		fi
		printf '    {\n'
		printf '      "name": "%s",\n' "$name"
		printf '      "version": "%s",\n' "$ver"
		printf '      "purl": "%s"\n' "$purl"
		printf '    }'
	done
	printf '\n'
	printf '%s\n' '  ]'
	printf '%s\n' '}'
} >"$tmp"
rm -f "$list"
mv "$tmp" "$out/components.json"
