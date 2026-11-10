package main

import (
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"os"
	"os/exec"
	"path"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
)

// A cell can run a package this machine has installed, or a cell: object, which is
// one project's own software (R78), with no image (R76). Its root is
// a directory of its own that holds links and little else, and each pack the program
// runs with is bound in read-only at the path it has on the machine. Nothing is copied,
// so a cell costs its links, and the store stays the only copy of the software. It sees
// what its package runs with and not the machine's /etc, catalog, homes or other packages.

var (
	packName = regexp.MustCompile(`^[a-z0-9][a-z0-9._+-]*$`)
	packHash = regexp.MustCompile(`^sha256-[0-9a-f]{64}$`)
)

// floor is what a package runs on without naming it: the C library, a shell and its
// tools for the #!/bin/sh that a pack's program so often is, and curl, whose bundle is
// where OpenSSL's default CA path links. A mount costs nothing, so the floor is cheap.
var floor = []string{"glibc", "bash", "coreutils", "curl"}

// notLinked are the names in a pack's lib/ that a machine's /lib leaves out too.
var notLinked = map[string]bool{"oath": true, "modules": true, "firmware": true, "pkgconfig": true, "python": true}

// pack is one installed package or cell, at the hash this machine links.
type pack struct{ kind, name, hash string }

// need is what a package or a cell runs with: an object in a package's status, and the
// id alone in a cell's, which names packages and pins none.
type need string

func (n *need) UnmarshalJSON(data []byte) error {
	var id string
	if json.Unmarshal(data, &id) == nil {
		*n = need(id)
		return nil
	}
	var object struct {
		ID string `json:"id"`
	}
	if err := json.Unmarshal(data, &object); err != nil {
		return err
	}
	*n = need(object.ID)
	return nil
}

// packInfo is the part of `oath --json get` that a cell reads.
type packInfo struct {
	Actual struct {
		Present bool   `json:"present"`
		Hash    string `json:"hash"`
		Needs   []need `json:"needs"`
	} `json:"actual"`
}

// oathGet is what this machine has of a package. Only oath says: the catalog is its.
var oathGet = func(id string) (packInfo, error) {
	var info packInfo
	out, err := exec.Command("oath", "--json", "get", id).Output()
	if err != nil {
		var failed struct {
			Error string `json:"error"`
		}
		if json.Unmarshal(out, &failed) == nil && failed.Error != "" {
			return info, errors.New(failed.Error)
		}
		return info, fmt.Errorf("oath get %s: %w", id, err)
	}
	if err := json.Unmarshal(out, &info); err != nil {
		return info, fmt.Errorf("oath get %s: %w", id, err)
	}
	return info, nil
}

// closure is every pack the roots run with, each once, in the order they are met: where
// two hold one name the first has it, as in a build's sandbox, so a package's own programs
// win. A need is taken at the hash this machine links, which is what its /lib would give
// the program, and not at the hash some plan pinned. Only the first root may be a cell:
// a cell runs with packages.
func closure(roots []string, get func(string) (packInfo, error)) ([]pack, error) {
	var out []pack
	seen := map[string]bool{}
	var walk func(id string, first bool) error
	walk = func(id string, first bool) error {
		if !strings.Contains(id, ":") {
			id = "pkg:" + id
		}
		if seen[id] {
			return nil
		}
		seen[id] = true
		kind, name, _ := strings.Cut(id, ":")
		if (kind != "pkg" && (kind != "cell" || !first)) || !packName.MatchString(name) {
			return fmt.Errorf("%q is not the name of a package", id)
		}
		info, err := get(id)
		if err != nil {
			return err
		}
		if !info.Actual.Present || !packHash.MatchString(info.Actual.Hash) {
			return fmt.Errorf("%s is not installed on this machine", id)
		}
		out = append(out, pack{kind, name, info.Actual.Hash})
		for _, n := range info.Actual.Needs {
			if err := walk(string(n), false); err != nil {
				return err
			}
		}
		return nil
	}
	for i, id := range roots {
		if err := walk(id, i == 0); err != nil {
			return nil, err
		}
	}
	return out, nil
}

// machineRoot is the catalog a cell's packages come from, as for oath itself.
func machineRoot() string {
	if root := os.Getenv("OATH_ROOT"); root != "" {
		return root
	}
	return "/oath"
}

// packSource is the root of a cell that runs an installed package or cell. Its program
// is bin/<name>, unless the command line says another.
func packSource(o *options) (*source, error) {
	kind, name, _ := strings.Cut(o.image, ":")
	roots := append([]string{o.image}, o.with...)
	packs, err := closure(append(roots, floor...), oathGet)
	if err != nil {
		return nil, err
	}
	machine := machineRoot()
	var config imageConfig
	config.Config.Env = []string{"PATH=/bin"}
	bin := filepath.Join(machine, "store", kind, name, packs[0].hash, "bin")
	if _, err := os.Stat(filepath.Join(bin, name)); err == nil {
		config.Config.Entrypoint = []string{"/bin/" + name}
	} else if o.entrypoint == "" && len(o.args) == 0 {
		return nil, fmt.Errorf("%s has no bin/%s, so name a program: %s", o.image, name, programs(bin))
	}
	return &source{
		name:   o.image,
		config: config,
		fill: func(rootfs string) error {
			binds, err := packRoot(rootfs, machine, packs, o.user)
			o.binds = binds
			return err
		},
	}, nil
}

func programs(bin string) string {
	entries, err := os.ReadDir(bin)
	if err != nil || len(entries) == 0 {
		return "it has none"
	}
	names := make([]string, len(entries))
	for i, e := range entries {
		names[i] = e.Name()
	}
	return strings.Join(names, ", ")
}

// packRoot makes a root of links, and returns the mounts that give them something to
// point at. A pack goes in at the path it has on a machine, since its programs name
// /oath/store/<kind>/<name>/live. /bin and /lib are this cell's own, made as a machine's
// are, so the program finds what it runs with and no more.
func packRoot(rootfs, machine string, packs []pack, user string) ([]mount, error) {
	if err := os.Mkdir(rootfs, 0o755); err != nil {
		return nil, err
	}
	root, err := os.OpenRoot(rootfs)
	if err != nil {
		return nil, err
	}
	defer root.Close()
	for _, dir := range []string{"bin", "etc", "lib", "root", "run", "tmp", "var"} {
		if err := root.Mkdir(dir, 0o755); err != nil {
			return nil, err
		}
	}
	if err := root.Chmod("tmp", 0o777|fs.ModeSticky); err != nil {
		return nil, err
	}
	var mounts []mount
	for _, p := range packs {
		slot := path.Join("oath/store", p.kind, p.name)
		if err := root.MkdirAll(path.Join(slot, p.hash), 0o755); err != nil {
			return nil, err
		}
		if err := root.Symlink(p.hash, path.Join(slot, "live")); err != nil {
			return nil, err
		}
		source := filepath.Join(machine, "store", p.kind, p.name, p.hash)
		for _, dir := range []string{"bin", "lib"} {
			entries, err := os.ReadDir(filepath.Join(source, dir))
			if errors.Is(err, fs.ErrNotExist) {
				continue
			}
			if err != nil {
				return nil, err
			}
			for _, e := range entries {
				at := path.Join(dir, e.Name())
				if dir == "lib" && notLinked[e.Name()] {
					continue
				}
				if _, err := root.Lstat(at); err == nil {
					continue
				}
				if err := root.Symlink("/"+path.Join(slot, p.hash, dir, e.Name()), at); err != nil {
					return nil, err
				}
			}
		}
		mounts = append(mounts, mount{"/" + path.Join(slot, p.hash), "bind", source, []string{"rbind", "ro", "nosuid", "nodev"}})
	}
	passwd, group := "root:x:0:0:root:/root:/bin/sh\n", "root:x:0:\n"
	// A program that looks itself up, as PostgreSQL does, finds the number --user gave it.
	if uid, gid, ok := numericUser(user); ok && uid != 0 {
		passwd += fmt.Sprintf("cell:x:%d:%d:cell:/root:/bin/sh\n", uid, gid)
		group += fmt.Sprintf("cell:x:%d:\n", gid)
	}
	if err := root.WriteFile("etc/passwd", []byte(passwd), 0o644); err != nil {
		return nil, err
	}
	if err := root.WriteFile("etc/group", []byte(group), 0o644); err != nil {
		return nil, err
	}
	return mounts, nil
}

// numericUser reads uid[:gid], where a lone uid is its own group too.
func numericUser(user string) (uint64, uint64, bool) {
	u, g, both := strings.Cut(user, ":")
	uid, err := strconv.ParseUint(u, 10, 31)
	if err != nil {
		return 0, 0, false
	}
	if !both {
		return uid, uid, true
	}
	gid, err := strconv.ParseUint(g, 10, 31)
	if err != nil {
		return 0, 0, false
	}
	return uid, gid, true
}
