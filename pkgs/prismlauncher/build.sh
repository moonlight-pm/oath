# Output: libexec/prismlauncher/ (bin/prismlauncher, share/PrismLauncher with
# its Java parts, NewLaunch.jar and JavaCheck.jar, which it finds beside its
# binary), bin/prismlauncher (the box, prismlauncher.sh), and its desktop
# entry, icons and MIME type in share/. No updater (Prism builds one only for
# its own release artifacts), no Java downloader, no tests.
tar -xf "$SRC/PrismLauncher-$VERSION.tar.gz"
cd "PrismLauncher-$VERSION"
# The same bytes from every build: Qt's rcc and CMake's build date read this,
# the release's own time, as the tarball has it before anything below edits it.
export SOURCE_DATE_EPOCH=$(stat -c %Y CMakeLists.txt)
nbt=3538933614059f0f44388a2b16f3db25ce42285b
rmdir libraries/libnbtplusplus
tar -xf "$SRC/libnbtplusplus-$nbt.tar.gz" -C libraries
mv "libraries/libnbtplusplus-$nbt" libraries/libnbtplusplus

# GameMode: the stub header in $FILES, as an interface target of that name.
f=CMakeLists.txt
sed -i 's|^    pkg_check_modules(gamemode REQUIRED IMPORTED_TARGET gamemode)$|    add_library(gamemode INTERFACE)\n    target_include_directories(gamemode INTERFACE "'"$FILES"'")|' "$f"
grep -q 'add_library(gamemode INTERFACE)' "$f"
# D-Bus: Prism asks switcheroo-control for a laptop's discrete GPU over it,
# and only with Qt's D-Bus module, which pkg:qtbase leaves out; it links
# the module even when it was not found.
sed -i 's|^    list(APPEND Launcher_QT_DBUS Qt6::DBus)$|    if(Qt6DBus_FOUND)\n        list(APPEND Launcher_QT_DBUS Qt6::DBus)\n    endif()|' "$f"
grep -q '^    if(Qt6DBus_FOUND)$' "$f"
# Its two Java parts (NewLaunch, JavaCheck) are compiled for Java 7, which
# JDK 21 no longer targets; Java 8 is the oldest Minecraft runs on.
for f in libraries/launcher/CMakeLists.txt libraries/javacheck/CMakeLists.txt; do
	sed -i 's|^set(CMAKE_JAVA_COMPILE_FLAGS -target 7 -source 7|set(CMAKE_JAVA_COMPILE_FLAGS --release 8|' "$f"
	grep -q '^set(CMAKE_JAVA_COMPILE_FLAGS --release 8' "$f"
done

export JAVA_HOME=/oath/store/pkg/jdk-21/live/libexec/jdk-21
# CMake finds each build pack by its prefix: the libraries on PKG_CONFIG_PATH,
# and the packs with no pkg-config file (ECM, toml++) by name.
s=/oath/store/pkg
prefixes=$(printf %s "$PKG_CONFIG_PATH" | tr : '\n' | sed 's|/lib/pkgconfig$||' | paste -sd ';')
for n in qtbase qtsvg qtnetworkauth extra-cmake-modules tomlplusplus; do prefixes="$s/$n/live;$prefixes"; done
d=$LIVE/libexec/prismlauncher
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$d" \
	-DCMAKE_PREFIX_PATH="$prefixes" \
	-DQT_ADDITIONAL_PACKAGES_PREFIX_PATH="$s/qtsvg/live;$s/qtnetworkauth/live" \
	-DLauncher_QT_VERSION_MAJOR=6 -DLauncher_BUILD_PLATFORM=Oath \
	-DLauncher_ENABLE_JAVA_DOWNLOADER=OFF -DBUILD_TESTING=OFF -DENABLE_LTO=OFF
cmake --build build -j"$JOBS"
DESTDIR=/tmp/dest cmake --install build
mkdir -p /out/libexec /out/bin /out/share
cp -a "/tmp/dest$d" /out/libexec/
p=/out/libexec/prismlauncher
[ -f "$p/share/PrismLauncher/NewLaunch.jar" ] && [ -f "$p/share/PrismLauncher/JavaCheck.jar" ]
# CMake's jar stamps each entry with this build's time, and jar reads no
# SOURCE_DATE_EPOCH: pack each again, sorted, at the release's.
when=$(date -u -d "@$SOURCE_DATE_EPOCH" +%Y-%m-%dT%H:%M:%SZ)
for j in "$p"/share/PrismLauncher/*.jar; do
	rm -rf /tmp/jar
	mkdir /tmp/jar
	(
		cd /tmp/jar
		"$JAVA_HOME/bin/jar" --extract --file "$j"
		"$JAVA_HOME/bin/jar" --create --file "$j" --manifest META-INF/MANIFEST.MF --date "$when" \
			$(find . -type f ! -path ./META-INF/MANIFEST.MF | LC_ALL=C sort)
	)
done
for s in applications icons metainfo mime; do
	[ -d "$p/share/$s" ] && mv "$p/share/$s" /out/share/
done
rm -rf "$p/share/man"
sed "s|@LIVE@|$LIVE|" "$FILES/prismlauncher.sh" >/out/bin/prismlauncher
chmod 755 /out/bin/prismlauncher
bash -n /out/bin/prismlauncher
