package main

import (
	"bytes"
	"compress/gzip"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"net"
	"net/http"
	"net/http/httptest"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"syscall"
	"testing"
	"unsafe"
)

// TestMain brings loopback up, as a package build's sandbox leaves it down. Elsewhere
// it is up, or this is not allowed, and either way the tests go on.
func TestMain(m *testing.M) {
	if fd, err := syscall.Socket(syscall.AF_INET, syscall.SOCK_DGRAM, 0); err == nil {
		var ifreq [40]byte
		copy(ifreq[:], "lo")
		flags := (*uint16)(unsafe.Pointer(&ifreq[16]))
		*flags = syscall.IFF_UP | syscall.IFF_LOOPBACK | syscall.IFF_RUNNING
		_, _, _ = syscall.Syscall(syscall.SYS_IOCTL, uintptr(fd), syscall.SIOCSIFFLAGS, uintptr(unsafe.Pointer(&ifreq)))
		_ = syscall.Close(fd)
	}
	os.Exit(m.Run())
}

// registry is a fake one, holding one repository's blobs and manifests.
type registry struct {
	*httptest.Server
	blobs     map[string][]byte
	manifests map[string][]byte
	token     string // what a bearer has to hold; the challenge is Basic if empty
	password  string // what a token or a Basic login has to have, if anything
	asked     []string
}

func newRegistry(t *testing.T) *registry {
	t.Helper()
	r := &registry{blobs: map[string][]byte{}, manifests: map[string][]byte{}}
	r.Server = httptest.NewServer(http.HandlerFunc(r.serve))
	t.Cleanup(r.Close)
	return r
}

func (r *registry) add(data []byte) string {
	digest := digestOf(data)
	r.blobs[digest] = data
	return digest
}

func basicOf(user, pass string) string {
	return "Basic " + base64.StdEncoding.EncodeToString([]byte(user+":"+pass))
}

func (r *registry) serve(w http.ResponseWriter, q *http.Request) {
	r.asked = append(r.asked, q.URL.Path)
	if q.URL.Path == "/token" {
		if r.password != "" && q.Header.Get("Authorization") != basicOf("me", r.password) {
			w.WriteHeader(http.StatusForbidden)
			return
		}
		_ = json.NewEncoder(w).Encode(map[string]string{"token": r.token})
		return
	}
	auth := q.Header.Get("Authorization")
	switch {
	case r.token != "" && auth != "Bearer "+r.token:
		w.Header().Set("WWW-Authenticate", fmt.Sprintf(`Bearer realm="%s/token",service="fake"`, r.URL))
		w.WriteHeader(http.StatusUnauthorized)
		return
	case r.token == "" && r.password != "" && auth != basicOf("me", r.password):
		w.Header().Set("WWW-Authenticate", `Basic realm="fake"`)
		w.WriteHeader(http.StatusUnauthorized)
		return
	}
	if _, name, ok := strings.Cut(q.URL.Path, "/manifests/"); ok {
		if data, ok := r.manifests[name]; ok {
			_, _ = w.Write(data)
			return
		}
	}
	if _, name, ok := strings.Cut(q.URL.Path, "/blobs/"); ok {
		if data, ok := r.blobs[name]; ok {
			_, _ = w.Write(data)
			return
		}
	}
	w.WriteHeader(http.StatusNotFound)
}

func gzipped(t *testing.T, data []byte) []byte {
	t.Helper()
	var buf bytes.Buffer
	w := gzip.NewWriter(&buf)
	if _, err := w.Write(data); err != nil {
		t.Fatal(err)
	}
	if err := w.Close(); err != nil {
		t.Fatal(err)
	}
	return buf.Bytes()
}

func sum(data []byte) string {
	h := sha256.Sum256(data)
	return "sha256:" + hex.EncodeToString(h[:])
}

// image puts one layered image in a registry under a tag, behind an index, and says
// what its digest is.
func (r *registry) image(t *testing.T, tag string, layers ...[]entry) string {
	t.Helper()
	var diffs []string
	var descriptors []map[string]any
	for _, l := range layers {
		plain := layerOf(t, l...).Bytes()
		diffs = append(diffs, sum(plain))
		blob := gzipped(t, plain)
		descriptors = append(descriptors, map[string]any{
			"mediaType": "application/vnd.oci.image.layer.v1.tar+gzip", "digest": r.add(blob), "size": len(blob),
			// Fetching this would send the login somewhere else, and it is never read.
			"urls": []string{"http://127.0.0.1:1/never"},
		})
	}
	config, _ := json.Marshal(map[string]any{
		"architecture": "amd64", "os": "linux", "rootfs": map[string]any{"diff_ids": diffs},
		"config": map[string]any{"Cmd": []string{"/bin/true"}},
	})
	configDigest := r.add(config)
	data, _ := json.Marshal(map[string]any{
		"schemaVersion": 2,
		"config":        map[string]any{"mediaType": "application/vnd.oci.image.config.v1+json", "digest": configDigest},
		"layers":        descriptors,
	})
	digest := digestOf(data)
	r.manifests[digest] = data
	arm, _ := json.Marshal(map[string]any{"schemaVersion": 2})
	r.manifests[digestOf(arm)] = arm
	index, _ := json.Marshal(map[string]any{
		"schemaVersion": 2,
		"manifests": []map[string]any{
			{"digest": digestOf(arm), "platform": map[string]string{"os": "linux", "architecture": "arm64"}},
			{"digest": digest, "platform": map[string]string{"os": "linux", "architecture": "amd64"}},
		},
	})
	r.manifests[tag] = index
	return digest
}

func storeOf(t *testing.T) *store {
	t.Helper()
	t.Setenv("CELL_ROOT", t.TempDir())
	s, err := openStore()
	if err != nil {
		t.Fatal(err)
	}
	return s
}

func refAt(t *testing.T, r *registry, tag string) ref {
	t.Helper()
	got, err := parseRef(strings.TrimPrefix(r.URL, "http://") + "/some/image:" + tag)
	if err != nil {
		t.Fatal(err)
	}
	return got
}

func TestPullChoosesThisPlatformAndAppliesLayers(t *testing.T) {
	r := newRegistry(t)
	r.token, r.password = "tok", ""
	digest := r.image(t, "v1", []entry{{name: "old"}, {name: "bin/app", body: "app"}}, []entry{{name: ".wh.old"}})
	s := storeOf(t)
	got, err := s.pull(refAt(t, r, "v1"))
	if err != nil {
		t.Fatal(err)
	}
	if got != digest {
		t.Errorf("%s, not %s", got, digest)
	}
	if s.lookup(refAt(t, r, "v1")) != digest {
		t.Error("the name does not hold it")
	}
	rootfs := filepath.Join(s.image(digest), "rootfs")
	has(t, filepath.Join(rootfs, "old"), false)
	has(t, filepath.Join(rootfs, "bin/app"), true)
	has(t, filepath.Join(s.image(digest), "layer"), false)
	for _, asked := range r.asked {
		if strings.Contains(asked, "never") {
			t.Error("fetched a url the image named")
		}
	}
	// And a second pull needs no blobs.
	r.asked = nil
	if _, err := s.pull(refAt(t, r, "v1")); err != nil {
		t.Fatal(err)
	}
	for _, asked := range r.asked {
		if strings.Contains(asked, "/blobs/") {
			t.Errorf("fetched %s again", asked)
		}
	}
}

func onlyPull(s *store) (int, error) {
	entries, err := os.ReadDir(s.images())
	return len(entries), err
}

func TestWhatIsNotWhatItSaysIsRefused(t *testing.T) {
	cases := map[string]func(r *registry){
		"a layer": func(r *registry) {
			for d, b := range r.blobs {
				if bytes.HasPrefix(b, []byte{0x1f, 0x8b}) {
					r.blobs[d] = append(bytes.Clone(b), 0)
				}
			}
		},
		"a config": func(r *registry) {
			for d, b := range r.blobs {
				if bytes.HasPrefix(b, []byte("{")) {
					r.blobs[d] = append(bytes.Clone(b), ' ')
				}
			}
		},
		"a manifest": func(r *registry) {
			for d, b := range r.manifests {
				if strings.HasPrefix(d, "sha256:") {
					r.manifests[d] = append(bytes.Clone(b), ' ')
				}
			}
		},
	}
	for name, spoil := range cases {
		t.Run(name, func(t *testing.T) {
			r := newRegistry(t)
			r.image(t, "v1", []entry{{name: "file"}})
			spoil(r)
			s := storeOf(t)
			if _, err := s.pull(refAt(t, r, "v1")); err == nil {
				t.Error("pulled")
			}
			if n, err := onlyPull(s); err != nil || n != 0 {
				t.Errorf("left %d images, %v", n, err)
			}
			if s.lookup(refAt(t, r, "v1")) != "" {
				t.Error("the name holds something")
			}
		})
	}
}

func TestALayerWithTheWrongContentsIsRefused(t *testing.T) {
	r := newRegistry(t)
	plain := layerOf(t, entry{name: "file"}).Bytes()
	blob := gzipped(t, plain)
	config, _ := json.Marshal(map[string]any{
		"architecture": "amd64", "os": "linux", "rootfs": map[string]any{"diff_ids": []string{sum([]byte("else"))}},
	})
	data, _ := json.Marshal(map[string]any{
		"schemaVersion": 2,
		"config":        map[string]any{"digest": r.add(config)},
		"layers":        []map[string]any{{"mediaType": "application/vnd.oci.image.layer.v1.tar+gzip", "digest": r.add(blob)}},
	})
	r.manifests["v1"] = data
	s := storeOf(t)
	if _, err := s.pull(refAt(t, r, "v1")); err == nil || !strings.Contains(err.Error(), "its config says") {
		t.Errorf("%v", err)
	}
}

func TestAPinnedDigestMustMatch(t *testing.T) {
	r := newRegistry(t)
	digest := r.image(t, "v1", []entry{{name: "file"}})
	s := storeOf(t)
	pinned := refAt(t, r, "v1")
	pinned.tag, pinned.digest = "", digest
	if got, err := s.pull(pinned); err != nil || got != digest {
		t.Errorf("%v %v", got, err)
	}
	wrong := pinned
	wrong.digest = digestOf([]byte("else"))
	r.manifests[wrong.digest] = r.manifests[digest]
	if _, err := s.pull(wrong); err == nil {
		t.Error("took a manifest that is not the digest")
	}
}

func TestLogin(t *testing.T) {
	r := newRegistry(t)
	r.password = "secret"
	digest := r.image(t, "v1", []entry{{name: "file"}})
	s := storeOf(t)
	if _, err := s.pull(refAt(t, r, "v1")); err == nil || !strings.Contains(err.Error(), "cell login") {
		t.Fatalf("no login, and: %v", err)
	}
	stdin, err := os.CreateTemp(t.TempDir(), "")
	if err != nil {
		t.Fatal(err)
	}
	fmt.Fprintln(stdin, "secret")
	_, _ = stdin.Seek(0, 0)
	saved := os.Stdin
	os.Stdin = stdin
	defer func() { os.Stdin = saved }()
	host := strings.TrimPrefix(r.URL, "http://")
	if err := s.login(host, "me"); err != nil {
		t.Fatal(err)
	}
	fi, err := os.Stat(filepath.Join(s.root, "auth.json"))
	if err != nil || fi.Mode().Perm() != 0o600 {
		t.Errorf("auth.json: %v %v", fi, err)
	}
	if got, err := s.pull(refAt(t, r, "v1")); err != nil || got != digest {
		t.Errorf("%v %v", got, err)
	}
}

func TestATokenFromElsewhereHasToBeHTTPS(t *testing.T) {
	c := &client{http: http.DefaultClient, host: "example.test", repo: "x"}
	if err := c.authorize(`Bearer realm="http://attacker.test/token"`); err == nil {
		t.Error("asked a token server over plain HTTP")
	}
}

func TestARedirectToAnotherHostGetsNoLogin(t *testing.T) {
	var got string
	cdn := httptest.NewUnstartedServer(http.HandlerFunc(func(w http.ResponseWriter, q *http.Request) {
		got = q.Header.Get("Authorization")
		_, _ = w.Write([]byte("blob"))
	}))
	listener, err := net.Listen("tcp", "127.0.0.2:0")
	if err != nil {
		t.Fatal(err)
	}
	_ = cdn.Listener.Close()
	cdn.Listener = listener
	cdn.Start()
	defer cdn.Close()
	front := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, q *http.Request) {
		http.Redirect(w, q, cdn.URL+"/blob", http.StatusTemporaryRedirect)
	}))
	defer front.Close()
	c := &client{http: &http.Client{}, base: front.URL, repo: "x", header: "Bearer tok"}
	file := filepath.Join(t.TempDir(), "blob")
	if err := c.blob(descriptor{Digest: digestOf([]byte("blob"))}, file); err != nil {
		t.Fatal(err)
	}
	if got != "" {
		t.Errorf("sent %q", got)
	}
}

func TestAZstdLayer(t *testing.T) {
	if _, err := exec.LookPath("zstd"); err != nil {
		t.Skip("no zstd here")
	}
	plain := layerOf(t, entry{name: "file", body: "held"}).Bytes()
	file := filepath.Join(t.TempDir(), "layer")
	cmd := exec.Command("zstd", "-q", "-c")
	cmd.Stdin = bytes.NewReader(plain)
	out, err := cmd.Output()
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(file, out, 0o600); err != nil {
		t.Fatal(err)
	}
	dir := t.TempDir()
	root, err := os.OpenRoot(dir)
	if err != nil {
		t.Fatal(err)
	}
	defer root.Close()
	if err := unpackFile(root, file, "application/vnd.oci.image.layer.v1.tar+zstd", sum(plain)); err != nil {
		t.Fatal(err)
	}
	has(t, filepath.Join(dir, "file"), true)
}

func TestAForeignLayerIsRefused(t *testing.T) {
	file := filepath.Join(t.TempDir(), "layer")
	if err := os.WriteFile(file, nil, 0o600); err != nil {
		t.Fatal(err)
	}
	dir := t.TempDir()
	root, err := os.OpenRoot(dir)
	if err != nil {
		t.Fatal(err)
	}
	defer root.Close()
	if err := unpackFile(root, file, "application/vnd.docker.image.rootfs.foreign.diff.tar.gzip", ""); err == nil {
		t.Error("took it")
	}
}

func TestParseRef(t *testing.T) {
	for in, want := range map[string]string{
		"alpine":                "docker.io/library/alpine:latest",
		"alpine:3":              "docker.io/library/alpine:3",
		"nginxinc/nginx":        "docker.io/nginxinc/nginx:latest",
		"ghcr.io/a/b:v1.2":      "ghcr.io/a/b:v1.2",
		"localhost:5000/x":      "localhost:5000/x:latest",
		"127.0.0.1:5000/x:y":    "127.0.0.1:5000/x:y",
		"a/b@sha256:" + zeros:   "docker.io/a/b@sha256:" + zeros,
		"a/b:t@sha256:" + zeros: "docker.io/a/b:t@sha256:" + zeros,
		"quay.io/a/b.c-d_e/f:1": "quay.io/a/b.c-d_e/f:1",
	} {
		got, err := parseRef(in)
		if err != nil || got.String() != want {
			t.Errorf("%s: %s, %v", in, got, err)
		}
	}
	for _, bad := range []string{"", "UPPER", "a/../b", "a b", "a:b:c", "a@md5:00", "a@sha256:00", "../x", "x/..", "a?b", "a#b"} {
		if got, err := parseRef(bad); err == nil {
			t.Errorf("%q took as %s", bad, got)
		}
	}
}

const zeros = "0000000000000000000000000000000000000000000000000000000000000000"
