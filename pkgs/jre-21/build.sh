# Output: libexec/jre-21/, Temurin's tree as it ships. Its programs name
# /lib64/ld-linux-x86-64.so.2; Oath has no /lib64 (R17), so patchelf points
# them at /lib's loader. Its libraries load libjvm.so, which is in
# lib/server/ and found there because java loads it first by path; the link
# in lib/ says the same to the check of what each library loads.
tar -xzf "$SRC/$SOURCES" --no-same-owner
d=/out/libexec/jre-21
mkdir -p /out/libexec
mv "jdk-$VERSION-jre" "$d"
grep -qx "JAVA_RUNTIME_VERSION=\"$VERSION-LTS\"" "$d/release"
for f in "$d"/bin/* "$d"/lib/jexec "$d"/lib/jspawnhelper; do
	patchelf --set-interpreter /lib/ld-linux-x86-64.so.2 "$f"
done
ln -s server/libjvm.so "$d/lib/libjvm.so"

# It runs a class (compiled by pkg:jdk-21) that starts a process, which
# goes through lib/jspawnhelper.
"$d/bin/java" -version 2>&1 | grep -q "Temurin-$VERSION"
mkdir /tmp/t
printf '%s\n' 'public class T { public static void main(String[] a) throws Exception {' \
	'System.out.println(new ProcessBuilder("true").start().waitFor() == 0 ? "ok" : "no"); } }' >/tmp/t/T.java
/oath/store/pkg/jdk-21/live/libexec/jdk-21/bin/javac --release 21 -d /tmp/t /tmp/t/T.java
[ "$("$d/bin/java" -cp /tmp/t T)" = ok ]
