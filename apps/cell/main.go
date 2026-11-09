// cell runs a container image: it pulls and unpacks the image itself, then runs it
// over crun and pasta, in the foreground, so that whatever started it supervises it.
// There is no daemon and no state that outlives the run but a store of images. Go's
// standard library only, so it has nothing to pin but pkg:go (R57, R59).
//
//	cell pull <image>              fetch an image, or move its tag
//	cell run [flags] <image> [args...]
//	cell run [flags] pkg:<name> [args...]  a package this machine has: no image, nothing copied
//	cell exec <name> <command...>  run a command in a running cell
//	cell stop <name>               stop a running cell, as SIGTERM to its cell does
//	cell ls                        the cells
//	cell images                    the images on disk
//	cell rmi <image>               forget an image
//	cell prune                     delete images no tag names, and what crashed cells left
//	cell login <registry> <user>   keep a password, read from stdin
//
// A cell holds one uid (R53): the account that ran it. That uid is root inside, or
// the number the image's USER or --user names, and owns every file. Each run starts
// from a fresh copy of the image, cheap on btrfs, and leaves nothing behind but what
// went to a volume. Run it as an account with no sudo, not as root (R55).
package main

import (
	"errors"
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"syscall"
	"time"
)

func main() {
	if len(os.Args) < 2 {
		usage()
	}
	if os.Args[1] == tiedCommand && len(os.Args) > 2 {
		fmt.Fprintf(os.Stderr, "cell: %v\n", tied(os.Args[2:]))
		os.Exit(1)
	}
	code, err := command(os.Args[1], os.Args[2:])
	if err != nil {
		fmt.Fprintf(os.Stderr, "cell: %v\n", err)
		os.Exit(1)
	}
	os.Exit(code)
}

func usage() {
	fmt.Fprintln(os.Stderr, "usage: cell pull|run|exec|stop|ls|images|rmi|prune|login ...")
	os.Exit(2)
}

func command(name string, args []string) (int, error) {
	s, err := openStore()
	if err != nil {
		return 0, err
	}
	switch name {
	case "pull":
		if len(args) != 1 {
			usage()
		}
		r, err := parseRef(args[0])
		if err != nil {
			return 0, err
		}
		digest, err := s.pull(r)
		if err == nil {
			fmt.Println(digest)
		}
		return 0, err
	case "run":
		return s.runCommand(args)
	case "exec":
		fs := flag.NewFlagSet("exec", flag.ExitOnError)
		tty := fs.Bool("t", false, "give the command a terminal")
		workdir := fs.String("w", "", "the directory to run it in")
		_ = fs.Parse(args)
		if fs.NArg() < 2 {
			usage()
		}
		return s.exec(fs.Arg(0), fs.Args()[1:], *tty, *workdir)
	case "stop":
		if len(args) != 1 {
			usage()
		}
		return 0, s.stop(args[0])
	case "ls":
		return 0, s.list()
	case "images":
		return 0, s.listImages()
	case "rmi":
		if len(args) != 1 {
			usage()
		}
		return 0, s.removeImage(args[0])
	case "prune":
		return 0, s.prune()
	case "login":
		if len(args) != 2 {
			usage()
		}
		return 0, s.login(args[0], args[1])
	}
	usage()
	return 0, nil
}

// store is a directory: images/<hex>/{rootfs,config.json}, refs/<image>, holding a
// digest, cells/<name>/, and auth.json.
type store struct{ root string }

func openStore() (*store, error) {
	root := os.Getenv("CELL_ROOT")
	if root == "" {
		home, err := os.UserHomeDir()
		if err != nil {
			return nil, err
		}
		root = filepath.Join(home, ".local/share/cell")
	}
	s := &store{root}
	for _, d := range []string{s.root, s.images(), s.refs(), s.cells()} {
		if err := os.MkdirAll(d, 0o700); err != nil {
			return nil, err
		}
	}
	return s, nil
}

func (s *store) images() string { return filepath.Join(s.root, "images") }
func (s *store) refs() string   { return filepath.Join(s.root, "refs") }
func (s *store) cells() string  { return filepath.Join(s.root, "cells") }

func (s *store) image(digest string) string {
	return filepath.Join(s.images(), strings.TrimPrefix(digest, "sha256:"))
}

func (s *store) cell(name string) string { return filepath.Join(s.cells(), name) }

func refFile(s *store, key string) string {
	return filepath.Join(s.refs(), strings.ReplaceAll(key, "/", "%"))
}

func (s *store) setRef(key, digest string) error {
	tmp := refFile(s, key) + ".tmp"
	if err := os.WriteFile(tmp, []byte(digest+"\n"), 0o600); err != nil {
		return err
	}
	return os.Rename(tmp, refFile(s, key))
}

// lookup is the digest an image's name holds on disk, or "".
func (s *store) lookup(r ref) string {
	data, err := os.ReadFile(refFile(s, r.String()))
	if err != nil {
		return ""
	}
	digest := strings.TrimSpace(string(data))
	if _, err := os.Stat(s.image(digest)); err != nil {
		return ""
	}
	return digest
}

func (s *store) listImages() error {
	names, err := s.tags()
	if err != nil {
		return err
	}
	for _, name := range names {
		fmt.Printf("%s\t%s\n", name[0], name[1])
	}
	return nil
}

// tags is each name on disk with its digest, sorted.
func (s *store) tags() ([][2]string, error) {
	files, err := os.ReadDir(s.refs())
	if err != nil {
		return nil, err
	}
	var out [][2]string
	for _, f := range files {
		data, err := os.ReadFile(filepath.Join(s.refs(), f.Name()))
		if err != nil {
			return nil, err
		}
		out = append(out, [2]string{strings.ReplaceAll(f.Name(), "%", "/"), strings.TrimSpace(string(data))})
	}
	sort.Slice(out, func(i, j int) bool { return out[i][0] < out[j][0] })
	return out, nil
}

func (s *store) removeImage(image string) error {
	r, err := parseRef(image)
	if err != nil {
		return err
	}
	if err := os.Remove(refFile(s, r.String())); err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return fmt.Errorf("no image %s", r)
		}
		return err
	}
	return s.prune()
}

// prune deletes what no name holds, along with what a killed pull or cell left. A
// running cell has its own copy, so it does not mind.
func (s *store) prune() error {
	cells, err := os.ReadDir(s.cells())
	if err != nil {
		return err
	}
	for _, c := range cells {
		if s.running(c.Name()) {
			continue
		}
		if err := forceRemove(s.cell(c.Name())); err != nil {
			return err
		}
		fmt.Println("deleted", c.Name())
	}
	names, err := s.tags()
	if err != nil {
		return err
	}
	kept := map[string]bool{}
	for _, name := range names {
		kept[strings.TrimPrefix(name[1], "sha256:")] = true
	}
	dirs, err := os.ReadDir(s.images())
	if err != nil {
		return err
	}
	for _, d := range dirs {
		if kept[d.Name()] {
			continue
		}
		if err := forceRemove(filepath.Join(s.images(), d.Name())); err != nil {
			return err
		}
		fmt.Println("deleted", d.Name())
	}
	return nil
}

// forceRemove removes a tree a container may have left closed to its own owner.
func forceRemove(dir string) error {
	_ = filepath.WalkDir(dir, func(path string, d os.DirEntry, err error) error {
		// A directory it could not read arrives here with the error, once.
		if err != nil || d.IsDir() {
			_ = os.Chmod(path, 0o700)
		}
		return nil
	})
	return os.RemoveAll(dir)
}

// lock takes a cell's lock, held for as long as its run lasts. Nobody holds it
// after a crash, so it says truly whether a cell is running.
func lock(dir string) (*os.File, error) {
	f, err := os.OpenFile(filepath.Join(dir, "lock"), os.O_CREATE|os.O_RDWR, 0o600)
	if err != nil {
		return nil, err
	}
	if err := syscall.Flock(int(f.Fd()), syscall.LOCK_EX|syscall.LOCK_NB); err != nil {
		_ = f.Close()
		return nil, err
	}
	return f, nil
}

func (s *store) running(name string) bool {
	f, err := lock(s.cell(name))
	if err != nil {
		return errors.Is(err, syscall.EWOULDBLOCK)
	}
	_ = f.Close()
	return false
}

func (s *store) list() error {
	dirs, err := os.ReadDir(s.cells())
	if err != nil {
		return err
	}
	for _, d := range dirs {
		state := "stopped"
		if s.running(d.Name()) {
			state = "running"
		}
		image, _ := os.ReadFile(filepath.Join(s.cell(d.Name()), "image"))
		since := ""
		if fi, err := os.Stat(filepath.Join(s.cell(d.Name()), "image")); err == nil {
			since = fi.ModTime().Format(time.RFC3339)
		}
		fmt.Printf("%s\t%s\t%s\t%s\n", d.Name(), state, strings.TrimSpace(string(image)), since)
	}
	return nil
}
