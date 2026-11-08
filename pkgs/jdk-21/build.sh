# Output: libexec/jdk-21/, Temurin's tree as it ships, without its source
# archive (lib/src.zip) and debug-symbol archives. Its programs are pointed
# at /lib's loader, and lib/libjvm.so names lib/server's, as in pkg:jre-21.
tar -xzf "$SRC/$SOURCES" --no-same-owner
d=/out/libexec/jdk-21
mkdir -p /out/libexec
mv "jdk-$VERSION" "$d"
grep -qx "JAVA_RUNTIME_VERSION=\"$VERSION-LTS\"" "$d/release"
rm -f "$d/lib/src.zip"
find "$d" -name '*.debuginfo' -delete
for f in "$d"/bin/* "$d"/lib/jexec "$d"/lib/jspawnhelper; do
	patchelf --set-interpreter /lib/ld-linux-x86-64.so.2 "$f"
done
ln -s server/libjvm.so "$d/lib/libjvm.so"

# It compiles and runs a class.
mkdir /tmp/t
printf '%s\n' 'public class T { public static void main(String[] a) {' \
	'System.out.println("ok"); } }' >/tmp/t/T.java
"$d/bin/javac" -d /tmp/t /tmp/t/T.java
[ "$("$d/bin/java" -cp /tmp/t T)" = ok ]
