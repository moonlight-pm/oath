# Output: lib/libclang.so and clang's built-in headers in lib/clang/<major>/,
# where libclang looks beside itself; the rest of LLVM's release stays out.
# The vendor's RUNPATH ($ORIGIN/../lib) goes (R17): what it loads, libstdc++,
# libgcc_s and libz, is in /lib. Its window is 1 GiB, hence --long.
major=${VERSION%%.*}
d=LLVM-$VERSION-Linux-X64
zstd -dc --long=31 "$SRC/$d.tar.zst" | tar -x --no-same-owner --wildcards \
	"$d/lib/libclang.so*" "$d/lib/clang/$major/include/*" "$d/include/clang-c/*"
mkdir -p /out/lib/clang/$major /out/share/licenses/libclang
cp -P $d/lib/libclang.so* /out/lib/
cp -R $d/lib/clang/$major/include /out/lib/clang/$major/
cp "$SRC/LLVM-LICENSE.TXT" /out/share/licenses/libclang/LICENSE.TXT
patchelf --remove-rpath /out/lib/libclang.so.$VERSION

# It loads with Oath's libstdc++ and parses C with no diagnostics, given
# what bindgen must be given here: the built-in headers by -resource-dir
# (libclang finds them by a clang binary, and there is none) and glibc's.
cat >/tmp/t.c <<'EOF'
#include <clang-c/Index.h>
#include <stdio.h>
int main(int argc, char **argv)
{
	CXIndex idx = clang_createIndex(0, 0);
	CXTranslationUnit tu = clang_parseTranslationUnit(idx, argv[1],
		(const char *const *)argv + 2, argc - 2, 0, 0, 0);
	if (!tu)
		return 2;
	unsigned n = clang_getNumDiagnostics(tu);
	for (unsigned i = 0; i < n; i++) {
		CXString s = clang_formatDiagnostic(clang_getDiagnostic(tu, i),
			clang_defaultDiagnosticDisplayOptions());
		fprintf(stderr, "%s\n", clang_getCString(s));
	}
	printf("%s\n", clang_getCString(clang_getClangVersion()));
	return n != 0;
}
EOF
printf '#include <stdarg.h>\n#include <stddef.h>\n#include <stdint.h>\nsize_t f(va_list a, uint64_t b);\n' >/tmp/h.c
gcc -I$d/include /tmp/t.c -L/out/lib -lclang -o /tmp/t
v=$(LD_LIBRARY_PATH=/out/lib /tmp/t /tmp/h.c -resource-dir=/out/lib/clang/$major \
	-idirafter /oath/store/pkg/glibc/live/include)
case $v in "clang version $VERSION "*) ;; *) exit 1 ;; esac
