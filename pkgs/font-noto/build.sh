# share/fonts/noto: every font file of the source pack, and the licence.
mkdir -p /out/share/fonts/noto
for f in "$SRC"/*.ttf; do
	cp "$f" /out/share/fonts/noto/
done
cp "$SRC/Noto-LICENSE" /out/share/fonts/noto/LICENSE
