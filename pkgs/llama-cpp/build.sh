# One program, llama-server, with its CPU and Vulkan backends linked in. Left out: plugins (the patch, and
# no GGML_BACKEND_DL), the HTTPS client that downloads models, subprocesses (router mode and
# the shell tool), the RPC backend, the web UI and its prebuilt download. What it still has
# behind flags, its file tools under --tools for one, `writ serve` does not turn on. A model
# is untrusted input to a C++ parser, so the build turns on what catches a bad index or copy.
mkdir -p /tmp/src /out/bin /out/share/doc/llama-cpp
cd /tmp/src
tar -xzf "$SRC/$SOURCES" --no-same-owner --strip-components=1
patch -p1 <"$FILES/patches/0001-no-plugins-from-a-path.patch"
# The shaders compile with glslang, which Oath has. glslc is its front end, and packaging it
# takes SPIRV-Tools and shaderc for one thing, spirv-opt's -O. Upstream already leaves that off
# the matrix shaders a driver with cooperative matrices runs. Without it on the rest, the speed
# has not been compared.
mkdir /tmp/bin
cat >/tmp/bin/glslc <<'GLSLC'
#!/bin/sh
# glslc's arguments, as llama.cpp's build uses them, for glslangValidator. glslc allows #include
# without asking. Success must print nothing to stderr, and errors, which glslangValidator
# prints to stdout, must go there.
n=$#
while [ "$n" -gt 0 ]; do
	a=$1
	shift
	n=$((n - 1))
	case $a in
	-fshader-stage=compute) set -- "$@" -S comp ;;
	--target-env=*) set -- "$@" --target-env "${a#--target-env=}" ;;
	-O | -MD) ;;
	-o | -MF)
		v=$1
		shift
		n=$((n - 1))
		if [ "$a" = -MF ]; then
			set -- "$@" --depfile "$v"
		elif [ "$v" = - ]; then
			set -- "$@" -o /dev/null
		else
			set -- "$@" -o "$v"
		fi
		;;
	*) set -- "$@" "$a" ;;
	esac
done
said=$(glslangValidator -V --quiet -P'#extension GL_GOOGLE_include_directive : enable' "$@") || {
	printf '%s\n' "$said" >&2
	exit 1
}
GLSLC
chmod +x /tmp/bin/glslc
hard='-D_FORTIFY_SOURCE=3 -fstack-protector-strong -fstack-clash-protection -fcf-protection'
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=OFF \
	-DCMAKE_POSITION_INDEPENDENT_CODE=ON \
	-DCMAKE_C_FLAGS="$hard" -DCMAKE_CXX_FLAGS="$hard -D_GLIBCXX_ASSERTIONS -isystem /oath/store/pkg/spirv-headers/live/include" \
	-DLLAMA_BUILD_NUMBER="${VERSION#b}" -DLLAMA_BUILD_COMMIT=8642541 \
	-DLLAMA_BUILD_TESTS=OFF -DLLAMA_BUILD_EXAMPLES=OFF -DLLAMA_BUILD_TOOLS=ON -DLLAMA_BUILD_SERVER=ON \
	-DLLAMA_BUILD_APP=OFF -DLLAMA_BUILD_UI=OFF -DLLAMA_USE_PREBUILT_UI=OFF -DLLAMA_CONNECT=OFF \
	-DLLAMA_OPENSSL=OFF -DLLAMA_SUBPROCESS=OFF \
	-DGGML_BACKEND_DL=OFF -DGGML_RPC=OFF -DGGML_OPENMP=OFF -DGGML_CCACHE=OFF \
	-DGGML_NATIVE=OFF -DGGML_AVX=ON -DGGML_AVX2=ON -DGGML_BMI2=ON -DGGML_FMA=ON -DGGML_F16C=ON \
	-DGGML_VULKAN=ON -DVulkan_GLSLC_EXECUTABLE=/tmp/bin/glslc \
	-DCMAKE_PREFIX_PATH='/oath/store/pkg/vulkan-headers/live;/oath/store/pkg/vulkan-loader/live;/oath/store/pkg/spirv-headers/live'
cmake --build build -j"$JOBS" --target llama-server
cp build/bin/llama-server /out/bin/
strip /out/bin/llama-server
cp LICENSE /out/share/doc/llama-cpp/LICENSE
cp licenses/LICENSE-jsonhpp /out/share/doc/llama-cpp/
cp vendor/cpp-httplib/LICENSE /out/share/doc/llama-cpp/LICENSE-httplib
cp vendor/hash/xxhash/LICENSE /out/share/doc/llama-cpp/LICENSE-xxhash
cp vendor/hash/sha256/LICENSE /out/share/doc/llama-cpp/LICENSE-sha256
cp vendor/hash/rotate-bits/LICENSE.md /out/share/doc/llama-cpp/LICENSE-rotate-bits

/out/bin/llama-server --version 2>&1 | grep -q "(build ${VERSION#b}, commit 8642541)"
grep -q ggml_vulkan /out/bin/llama-server
# Nothing is loaded from the current directory, or from where the environment says.
# --list-devices looks for plugins as a model's load does, and an unpatched build loads this.
mkdir /tmp/plant
cd /tmp/plant
echo 'int close(int); int open(const char *, int, ...);
__attribute__((constructor)) static void f(void) { close(open("/tmp/plant/loaded", 0101, 0644)); }' >p.c
cc -shared -fPIC p.c -o libggml-cpu-plant.so
/out/bin/llama-server --list-devices >/dev/null 2>&1
GGML_BACKEND_PATH=/tmp/plant/libggml-cpu-plant.so /out/bin/llama-server --list-devices >/dev/null 2>&1
[ ! -e /tmp/plant/loaded ]
