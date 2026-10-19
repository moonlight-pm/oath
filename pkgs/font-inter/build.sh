# share/fonts/inter: the static faces (Inter.ttc) and the variable fonts,
# as upstream advises installing them.
python3 -c 'import sys, zipfile; zipfile.ZipFile(sys.argv[1]).extractall(sys.argv[2])' \
	"$SRC/$SOURCES" /tmp/inter
mkdir -p /out/share/fonts/inter
cp /tmp/inter/Inter.ttc /tmp/inter/InterVariable.ttf /tmp/inter/InterVariable-Italic.ttf \
	/tmp/inter/LICENSE.txt /out/share/fonts/inter/
