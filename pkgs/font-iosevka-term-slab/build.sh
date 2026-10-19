# share/fonts/iosevka-term-slab: the Regular and Bold collections.
d=/out/share/fonts/iosevka-term-slab
mkdir -p "$d"
python3 -c 'import sys, zipfile
z = zipfile.ZipFile(sys.argv[1])
for w in ("Regular", "Bold"):
    z.extract(f"SGr-IosevkaTermSlab-{w}.ttc", sys.argv[2])' \
	"$SRC/PkgTTC-SGr-IosevkaTermSlab-$VERSION.zip" "$d"
cp "$SRC/Iosevka-LICENSE.md" "$d/LICENSE.md"
