package main

import (
	"archive/tar"
	"errors"
	"fmt"
	"io"
	"io/fs"
	"os"
	"path"
	"strings"
)

// unpackLayer lays one tar over a rootfs, as its one owner. A layer is what a stranger
// sent, so os.Root holds every write inside the rootfs. Modes keep u+rw, u+rwx on a
// directory, since the owner bits only ever bind this one uid: it has to read and
// delete all of it. Setuid and setgid bits, devices and xattrs do not land.
func unpackLayer(root *os.Root, r io.Reader) error {
	seen := map[string]bool{}
	tr := tar.NewReader(r)
	for {
		h, err := tr.Next()
		if errors.Is(err, io.EOF) {
			return nil
		}
		if err != nil {
			return err
		}
		if err := unpackEntry(root, tr, h, seen); err != nil {
			return fmt.Errorf("%s: %w", h.Name, err)
		}
	}
}

func unpackEntry(root *os.Root, r io.Reader, h *tar.Header, seen map[string]bool) error {
	dir, base := path.Split(path.Clean("/" + h.Name))
	if base == "" {
		return nil
	}
	parent, err := resolve(root, dir)
	if err != nil {
		return err
	}
	target := path.Join(parent, base)
	if base == ".wh..wh..opq" {
		return clearDir(root, parent, seen)
	}
	if hidden, ok := strings.CutPrefix(base, ".wh."); ok {
		// A whiteout speaks to the layers below, not to its own.
		if hidden = path.Join(parent, hidden); !seen[hidden] {
			return root.RemoveAll(hidden)
		}
		return nil
	}
	switch h.Typeflag {
	case tar.TypeDir, tar.TypeReg, tar.TypeSymlink, tar.TypeLink:
	case tar.TypeChar, tar.TypeBlock, tar.TypeFifo:
		fmt.Fprintf(os.Stderr, "cell: left out %s, a device or pipe\n", h.Name)
		return nil
	default:
		return nil
	}
	if err := makeParents(root, parent, seen); err != nil {
		return err
	}
	seen[target] = true
	perm := fs.FileMode(h.Mode) & 0o777
	if h.Typeflag == tar.TypeDir {
		if fi, err := root.Lstat(target); err != nil || !fi.IsDir() {
			if err := root.RemoveAll(target); err != nil {
				return err
			}
			if err := root.Mkdir(target, 0o700); err != nil {
				return err
			}
		}
		return root.Chmod(target, perm|0o700)
	}
	if err := root.RemoveAll(target); err != nil {
		return err
	}
	switch h.Typeflag {
	case tar.TypeSymlink:
		return root.Symlink(h.Linkname, target)
	case tar.TypeLink:
		from, err := resolve(root, h.Linkname)
		if err != nil {
			return err
		}
		return root.Link(from, target)
	}
	f, err := root.OpenFile(target, os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0o600)
	if err != nil {
		return err
	}
	if _, err := io.Copy(f, r); err != nil {
		_ = f.Close()
		return err
	}
	if err := f.Close(); err != nil {
		return err
	}
	if err := root.Chmod(target, perm|0o600); err != nil {
		return err
	}
	return root.Chtimes(target, h.AccessTime, h.ModTime)
}

// resolve follows a path's links as the container will, where an absolute link and
// a .. above the top stay in the rootfs. os.Root would refuse those, and images have them.
func resolve(root *os.Root, name string) (string, error) {
	todo := strings.Split(name, "/")
	var done []string
	for hops := 0; len(todo) > 0; {
		c := todo[0]
		todo = todo[1:]
		switch c {
		case "", ".":
			continue
		case "..":
			if len(done) > 0 {
				done = done[:len(done)-1]
			}
			continue
		}
		at := path.Join(path.Join(done...), c)
		fi, err := root.Lstat(at)
		if err != nil || fi.Mode()&fs.ModeSymlink == 0 {
			done = append(done, c)
			continue
		}
		if hops++; hops > 40 {
			return "", errors.New("too many links")
		}
		link, err := root.Readlink(at)
		if err != nil {
			return "", err
		}
		if path.IsAbs(link) {
			done = nil
		}
		todo = append(strings.Split(link, "/"), todo...)
	}
	return path.Join(done...), nil
}

// makeParents makes a directory and what it is in, and marks them this layer's.
func makeParents(root *os.Root, dir string, seen map[string]bool) error {
	if dir != "" {
		if err := root.MkdirAll(dir, 0o755); err != nil {
			return err
		}
	}
	for ; dir != "" && dir != "."; dir = path.Dir(dir) {
		seen[dir] = true
	}
	return nil
}

// clearDir empties a directory of what the layers below put there.
func clearDir(root *os.Root, dir string, seen map[string]bool) error {
	if dir == "" {
		dir = "."
	}
	d, err := root.Open(dir)
	if errors.Is(err, fs.ErrNotExist) {
		return nil
	}
	if err != nil {
		return err
	}
	entries, err := d.ReadDir(-1)
	_ = d.Close()
	if err != nil {
		return err
	}
	for _, e := range entries {
		at := path.Join(dir, e.Name())
		switch {
		case !seen[at]:
			if err := root.RemoveAll(at); err != nil {
				return err
			}
		case e.IsDir():
			if err := clearDir(root, at, seen); err != nil {
				return err
			}
		}
	}
	return nil
}
