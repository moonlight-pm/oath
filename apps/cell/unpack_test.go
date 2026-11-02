package main

import (
	"archive/tar"
	"bytes"
	"io/fs"
	"os"
	"path/filepath"
	"testing"
	"time"
)

type entry struct {
	name string
	kind byte
	mode int64
	body string
	link string
}

func layerOf(t *testing.T, entries ...entry) *bytes.Buffer {
	t.Helper()
	var buf bytes.Buffer
	w := tar.NewWriter(&buf)
	for _, e := range entries {
		if e.kind == 0 {
			e.kind = tar.TypeReg
		}
		if e.mode == 0 {
			e.mode = 0o644
		}
		h := &tar.Header{Name: e.name, Typeflag: e.kind, Mode: e.mode, Linkname: e.link, Size: int64(len(e.body)),
			ModTime: time.Unix(1000000000, 0)}
		if err := w.WriteHeader(h); err != nil {
			t.Fatal(err)
		}
		if _, err := w.Write([]byte(e.body)); err != nil {
			t.Fatal(err)
		}
	}
	if err := w.Close(); err != nil {
		t.Fatal(err)
	}
	return &buf
}

// rootfsOf is a rootfs inside a directory, so that an escape has somewhere to land.
func rootfsOf(t *testing.T, layers ...[]entry) (string, string) {
	t.Helper()
	outer := t.TempDir()
	dir := filepath.Join(outer, "rootfs")
	if err := os.Mkdir(dir, 0o755); err != nil {
		t.Fatal(err)
	}
	root, err := os.OpenRoot(dir)
	if err != nil {
		t.Fatal(err)
	}
	defer root.Close()
	for _, l := range layers {
		if err := unpackLayer(root, layerOf(t, l...)); err != nil {
			t.Fatal(err)
		}
	}
	return outer, dir
}

func has(t *testing.T, path string, want bool) {
	t.Helper()
	_, err := os.Lstat(path)
	if (err == nil) != want {
		t.Errorf("%s: exists is %v", path, err == nil)
	}
}

func TestPathsStayInTheRootfs(t *testing.T) {
	outer, dir := rootfsOf(t, []entry{
		{name: "../../up"},
		{name: "/abs"},
		{name: "a/../../../sideways"},
		{name: "abslink", kind: tar.TypeSymlink, link: "/etc"},
		{name: "abslink/through"},
		{name: "rellink", kind: tar.TypeSymlink, link: "../../.."},
		{name: "rellink/past"},
		{name: "etc/file", body: "inside"},
		{name: "hard", kind: tar.TypeLink, link: "../../etc/file"},
	})
	has(t, filepath.Join(outer, "up"), false)
	has(t, filepath.Join(filepath.Dir(outer), "up"), false)
	for _, inside := range []string{"up", "abs", "sideways", "etc/through", "past", "hard"} {
		has(t, filepath.Join(dir, inside), true)
	}
	if data, _ := os.ReadFile(filepath.Join(dir, "hard")); string(data) != "inside" {
		t.Errorf("hard link: %q", data)
	}
	// The links stay as the image has them: only the container reads them.
	if link, _ := os.Readlink(filepath.Join(dir, "abslink")); link != "/etc" {
		t.Errorf("abslink: %q", link)
	}
}

func TestAHardLinkToWhatIsNotThereFails(t *testing.T) {
	dir := t.TempDir()
	root, err := os.OpenRoot(dir)
	if err != nil {
		t.Fatal(err)
	}
	defer root.Close()
	if err := unpackLayer(root, layerOf(t, entry{name: "x", kind: tar.TypeLink, link: "/etc/passwd"})); err == nil {
		t.Error("linked to nothing")
	}
}

func TestModes(t *testing.T) {
	_, dir := rootfsOf(t, []entry{
		{name: "setuid", mode: 0o4755},
		{name: "closed", mode: 0o000 | 0o1000},
		{name: "shut/", kind: tar.TypeDir, mode: 0o2500},
		{name: "shut/file"},
		{name: "null", kind: tar.TypeChar},
		{name: "pipe", kind: tar.TypeFifo},
	})
	mode := func(name string) fs.FileMode {
		fi, err := os.Lstat(filepath.Join(dir, name))
		if err != nil {
			t.Fatal(err)
		}
		return fi.Mode()
	}
	if got := mode("setuid"); got != 0o755 {
		t.Errorf("setuid: %v", got)
	}
	if got := mode("closed"); got != 0o600 {
		t.Errorf("a closed file stays this uid's: %v", got)
	}
	if got := mode("shut").Perm(); got != 0o700 {
		t.Errorf("a closed directory stays this uid's: %v", got)
	}
	has(t, filepath.Join(dir, "null"), false)
	has(t, filepath.Join(dir, "pipe"), false)
	fi, err := os.Stat(filepath.Join(dir, "setuid"))
	if err != nil {
		t.Fatal(err)
	}
	if !fi.ModTime().Equal(time.Unix(1000000000, 0)) {
		t.Errorf("mtime: %v", fi.ModTime())
	}
}

func TestLayersReplace(t *testing.T) {
	_, dir := rootfsOf(t,
		[]entry{
			{name: "file", body: "old"},
			{name: "was-file", body: "old"},
			{name: "was-dir/inner"},
			{name: "was-link", kind: tar.TypeSymlink, link: "/etc"},
			{name: "target", body: "old"},
			{name: "through", kind: tar.TypeSymlink, link: "target"},
		},
		[]entry{
			{name: "file", body: "new"},
			{name: "was-file/", kind: tar.TypeDir, mode: 0o755},
			{name: "was-dir", body: "now a file"},
			{name: "was-link/", kind: tar.TypeDir, mode: 0o755},
			{name: "through", body: "new"},
		})
	read := func(name string) string {
		data, err := os.ReadFile(filepath.Join(dir, name))
		if err != nil {
			t.Fatal(err)
		}
		return string(data)
	}
	if read("file") != "new" || read("was-dir") != "now a file" {
		t.Error("a later layer did not replace")
	}
	if fi, _ := os.Lstat(filepath.Join(dir, "was-file")); fi == nil || !fi.IsDir() {
		t.Error("a file did not become a directory")
	}
	if fi, _ := os.Lstat(filepath.Join(dir, "was-link")); fi == nil || !fi.IsDir() {
		t.Error("a link did not become a directory")
	}
	if read("target") != "old" {
		t.Error("a file wrote through the link it replaced")
	}
}

func TestWhiteouts(t *testing.T) {
	_, dir := rootfsOf(t,
		[]entry{
			{name: "gone"},
			{name: "kept"},
			{name: "d/old"},
			{name: "d/sub/old"},
			{name: "e/old"},
		},
		[]entry{
			{name: ".wh.gone"},
			{name: "d/new"},
			{name: "d/sub/new"},
			{name: "d/.wh..wh..opq"},
			{name: "e/.wh..wh..opq"},
			{name: "e/after"},
			{name: "same"},
			{name: ".wh.same"},
		})
	for name, want := range map[string]bool{
		"gone": false, "kept": true,
		"d/old": false, "d/sub/old": false, "d/new": true, "d/sub/new": true,
		"e/old": false, "e/after": true,
		"same": true, ".wh.same": false, "d/.wh..wh..opq": false,
	} {
		has(t, filepath.Join(dir, name), want)
	}
}
