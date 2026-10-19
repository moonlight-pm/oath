# cef/Release (libcef.so, ANGLE, SwiftShader, the V8 snapshot) and
# cef/Resources, whose files are also linked into Release, where CEF
# looks beside libcef.so. libcef.so keeps its symbols but not its 1 GB
# of DWARF. sola/cef-<version> is the layout Sola's build looks for;
# lib/libcef.so is for the load check: sola-browser loads libcef.so by
# RUNPATH from Release, beside what CEF opens from there. No
# chrome-sandbox: Sola runs CEF without its setuid sandbox.
tar -xf "$SRC/$SOURCES" -C /tmp
d=$(echo /tmp/cef_binary_*_linux64_minimal)
mkdir -p /out/cef /out/lib /out/sola
cp -a "$d/Release" "$d/Resources" "$d/LICENSE.txt" "$d/CREDITS.html" /out/cef/
rm -f /out/cef/Release/chrome-sandbox
strip --strip-debug /out/cef/Release/libcef.so
for f in /out/cef/Resources/*; do
	ln -s "../Resources/${f##*/}" "/out/cef/Release/${f##*/}"
done
ln -s ../cef/Release/libcef.so /out/lib/libcef.so
ln -s ../cef "/out/sola/cef-$VERSION"
