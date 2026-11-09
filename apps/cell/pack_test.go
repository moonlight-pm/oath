package main

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"io/fs"
	"os"
	"path/filepath"
	"reflect"
	"slices"
	"strings"
	"testing"
)

func hashOf(name string) string {
	sum := sha256.Sum256([]byte(name))
	return "sha256-" + hex.EncodeToString(sum[:])
}

// catalogOf answers as oath would, for packages named with what they need.
func catalogOf(needs map[string][]string) func(string) (packInfo, error) {
	return func(id string) (packInfo, error) {
		var info packInfo
		name := strings.TrimPrefix(id, "pkg:")
		list, ok := needs[name]
		if !ok {
			return info, nil
		}
		info.Actual.Present = true
		info.Actual.Hash = hashOf(name)
		for _, n := range list {
			info.Actual.Needs = append(info.Actual.Needs, need("pkg:"+n))
		}
		return info, nil
	}
}

func TestClosureTakesEachOnceWithThePackageFirst(t *testing.T) {
	get := catalogOf(map[string][]string{
		"app": {"one", "two"}, "one": {"two"}, "two": nil, "extra": nil, "glibc": nil, "bash": {"ncurses"}, "ncurses": nil,
	})
	packs, err := closure([]string{"app", "extra", "glibc", "bash"}, get)
	if err != nil {
		t.Fatal(err)
	}
	var names []string
	for _, p := range packs {
		names = append(names, p.name)
		if p.hash != hashOf(p.name) {
			t.Errorf("%s at %s", p.name, p.hash)
		}
	}
	want := []string{"app", "one", "two", "extra", "glibc", "bash", "ncurses"}
	if !reflect.DeepEqual(names, want) {
		t.Errorf("got %v, want %v", names, want)
	}
}

func TestClosureRefusesWhatIsNotInstalled(t *testing.T) {
	get := catalogOf(map[string][]string{"app": {"gone"}})
	_, err := closure([]string{"app"}, get)
	if err == nil || !strings.Contains(err.Error(), "pkg:gone is not installed") {
		t.Errorf("got %v", err)
	}
}

func TestClosureSaysWhatOathSaid(t *testing.T) {
	want := errors.New("no object pkg:app")
	_, err := closure([]string{"app"}, func(string) (packInfo, error) { return packInfo{}, want })
	if !errors.Is(err, want) {
		t.Errorf("got %v", err)
	}
}

func TestClosureRefusesANameThatIsAPath(t *testing.T) {
	asked := false
	_, err := closure([]string{"../../etc"}, func(string) (packInfo, error) {
		asked = true
		return packInfo{}, nil
	})
	if err == nil || asked {
		t.Errorf("got %v, asked %v", err, asked)
	}
}

func TestClosureRefusesAHashThatIsAPath(t *testing.T) {
	_, err := closure([]string{"app"}, func(string) (packInfo, error) {
		var info packInfo
		info.Actual.Present = true
		info.Actual.Hash = "../../../etc"
		return info, nil
	})
	if err == nil {
		t.Error("a hash that is a path was taken")
	}
}

// machineWith lays out a store, each file named relative to its pack.
func machineWith(t *testing.T, packs map[string][]string) string {
	t.Helper()
	machine := t.TempDir()
	for name, files := range packs {
		for _, f := range files {
			kind, slot := "pkg", name
			if cell, ok := strings.CutPrefix(name, "cell:"); ok {
				kind, slot = "cell", cell
			}
			p := filepath.Join(machine, "store", kind, slot, hashOf(name), f)
			if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
				t.Fatal(err)
			}
			if err := os.WriteFile(p, []byte("the only copy"), 0o755); err != nil {
				t.Fatal(err)
			}
		}
	}
	return machine
}

func TestPackRootIsLinksAndMounts(t *testing.T) {
	machine := machineWith(t, map[string][]string{
		"app":   {"bin/app", "bin/shared", "lib/libapp.so.1", "lib/pkgconfig/app.pc", "lib/oath/helper"},
		"glibc": {"bin/shared", "lib/libc.so.6", "include/stdio.h"},
	})
	rootfs := filepath.Join(t.TempDir(), "rootfs")
	packs := []pack{{"pkg", "app", hashOf("app")}, {"pkg", "glibc", hashOf("glibc")}}
	mounts, err := packRoot(rootfs, machine, packs, "")
	if err != nil {
		t.Fatal(err)
	}
	store := "/oath/store/pkg/"
	links := map[string]string{
		"bin/app":                 store + "app/" + hashOf("app") + "/bin/app",
		"bin/shared":              store + "app/" + hashOf("app") + "/bin/shared",
		"lib/libapp.so.1":         store + "app/" + hashOf("app") + "/lib/libapp.so.1",
		"lib/libc.so.6":           store + "glibc/" + hashOf("glibc") + "/lib/libc.so.6",
		"oath/store/pkg/app/live": hashOf("app"),
	}
	for at, want := range links {
		got, err := os.Readlink(filepath.Join(rootfs, at))
		if err != nil || got != want {
			t.Errorf("%s: got %q, %v, want %q", at, got, err, want)
		}
	}
	for _, absent := range []string{"lib/pkgconfig", "lib/oath", "include", "home", "work", "oath/objects"} {
		if _, err := os.Lstat(filepath.Join(rootfs, absent)); err == nil {
			t.Errorf("%s is in the root", absent)
		}
	}
	if len(mounts) != 2 {
		t.Fatalf("got %d mounts", len(mounts))
	}
	m := mounts[1]
	if m.Destination != store+"glibc/"+hashOf("glibc") || m.Source != filepath.Join(machine, "store/pkg/glibc", hashOf("glibc")) {
		t.Errorf("got %+v", m)
	}
	for _, o := range []string{"ro", "nosuid", "nodev"} {
		if !slices.Contains(m.Options, o) {
			t.Errorf("no %s in %v", o, m.Options)
		}
	}
	// Nothing of a pack is copied: the only regular files are the two it writes.
	var files []string
	err = filepath.WalkDir(rootfs, func(p string, d fs.DirEntry, err error) error {
		if err == nil && d.Type().IsRegular() {
			rel, _ := filepath.Rel(rootfs, p)
			files = append(files, rel)
		}
		return err
	})
	if err != nil {
		t.Fatal(err)
	}
	slices.Sort(files)
	if want := []string{"etc/group", "etc/passwd"}; !reflect.DeepEqual(files, want) {
		t.Errorf("got %v, want %v", files, want)
	}
	fi, err := os.Stat(filepath.Join(rootfs, "tmp"))
	if err != nil || fi.Mode()&fs.ModeSticky == 0 || fi.Mode().Perm() != 0o777 {
		t.Errorf("/tmp: %v, %v", fi, err)
	}
}

func TestPackRootGivesANumberAName(t *testing.T) {
	machine := machineWith(t, map[string][]string{"app": {"bin/app"}})
	rootfs := filepath.Join(t.TempDir(), "rootfs")
	if _, err := packRoot(rootfs, machine, []pack{{"pkg", "app", hashOf("app")}}, "999:998"); err != nil {
		t.Fatal(err)
	}
	u, err := findUser(rootfs, "999:998", "")
	if err != nil || u.uid != 999 || u.gid != 998 {
		t.Errorf("got %+v, %v", u, err)
	}
	passwd, _ := os.ReadFile(filepath.Join(rootfs, "etc/passwd"))
	group, _ := os.ReadFile(filepath.Join(rootfs, "etc/group"))
	if !strings.Contains(string(passwd), "cell:x:999:998:") || !strings.Contains(string(group), "cell:x:998:") {
		t.Errorf("passwd %q, group %q", passwd, group)
	}
}

func TestPackRootAsRootNamesNoOne(t *testing.T) {
	machine := machineWith(t, map[string][]string{"app": {"bin/app"}})
	rootfs := filepath.Join(t.TempDir(), "rootfs")
	if _, err := packRoot(rootfs, machine, []pack{{"pkg", "app", hashOf("app")}}, ""); err != nil {
		t.Fatal(err)
	}
	passwd, _ := os.ReadFile(filepath.Join(rootfs, "etc/passwd"))
	if string(passwd) != "root:x:0:0:root:/root:/bin/sh\n" {
		t.Errorf("got %q", passwd)
	}
}

// withMachine points a pkg: cell at a store and a catalog made for a test.
func withMachine(t *testing.T, machine string, needs map[string][]string) {
	t.Helper()
	t.Setenv("OATH_ROOT", machine)
	saved := oathGet
	oathGet = catalogOf(needs)
	t.Cleanup(func() { oathGet = saved })
}

var floorNeeds = map[string][]string{"glibc": nil, "bash": nil, "coreutils": nil, "curl": nil}

func needsWith(extra map[string][]string) map[string][]string {
	all := map[string][]string{}
	for k, v := range floorNeeds {
		all[k] = v
	}
	for k, v := range extra {
		all[k] = v
	}
	return all
}

func TestPackSourceRunsTheProgramNamedForThePackage(t *testing.T) {
	machine := machineWith(t, map[string][]string{"ghost": {"bin/ghost"}})
	withMachine(t, machine, needsWith(map[string][]string{"ghost": nil}))
	o := &options{image: "pkg:ghost"}
	from, err := packSource(o)
	if err != nil {
		t.Fatal(err)
	}
	config, err := spec(o, &from.config, userIDs{}, t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	process := config["process"].(map[string]any)
	if got := process["args"].([]string); !reflect.DeepEqual(got, []string{"/bin/ghost"}) {
		t.Errorf("args %v", got)
	}
	if env := process["env"].([]string); !slices.Contains(env, "PATH=/bin") {
		t.Errorf("env %v", env)
	}
}

func TestPackSourceAsksForAProgramWhenThePackageHasNoneOfItsName(t *testing.T) {
	machine := machineWith(t, map[string][]string{"python": {"bin/python3", "bin/pydoc3"}})
	withMachine(t, machine, needsWith(map[string][]string{"python": nil}))
	_, err := packSource(&options{image: "pkg:python"})
	if err == nil || !strings.Contains(err.Error(), "python3") {
		t.Errorf("got %v", err)
	}
	o := &options{image: "pkg:python", args: []string{"python3", "-V"}}
	from, err := packSource(o)
	if err != nil {
		t.Fatal(err)
	}
	config, err := spec(o, &from.config, userIDs{}, t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	if got := config["process"].(map[string]any)["args"].([]string); !reflect.DeepEqual(got, []string{"python3", "-V"}) {
		t.Errorf("args %v", got)
	}
}

func TestPackSourceTakesWhatWithNamesAndTheFloor(t *testing.T) {
	machine := machineWith(t, map[string][]string{
		"app": {"bin/app"}, "jq": {"bin/jq"}, "curl": {"bin/curl"}, "glibc": {"lib/libc.so.6"}, "bash": {"bin/sh"}, "coreutils": {"bin/cat"},
	})
	withMachine(t, machine, needsWith(map[string][]string{"app": nil, "jq": nil}))
	o := &options{image: "pkg:app", with: list{"pkg:jq"}}
	from, err := packSource(o)
	if err != nil {
		t.Fatal(err)
	}
	rootfs := filepath.Join(t.TempDir(), "rootfs")
	if err := from.fill(rootfs); err != nil {
		t.Fatal(err)
	}
	if len(o.binds) != 6 {
		t.Errorf("got %d binds", len(o.binds))
	}
	for _, at := range []string{"bin/app", "bin/jq", "bin/curl", "bin/sh", "bin/cat", "lib/libc.so.6"} {
		if _, err := os.Readlink(filepath.Join(rootfs, at)); err != nil {
			t.Error(err)
		}
	}
	mounts, err := mountsFor(o, t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	for _, b := range o.binds {
		if !slices.ContainsFunc(mounts, func(m mount) bool { return m.Destination == b.Destination && m.Source == b.Source }) {
			t.Errorf("no mount for %s", b.Destination)
		}
	}
}

func TestANeedIsAnIdOrAnObject(t *testing.T) {
	var info packInfo
	doc := `{"actual":{"present":true,"hash":"x","needs":["pkg:a",{"id":"pkg:b","hash":"y"}]}}`
	if err := json.Unmarshal([]byte(doc), &info); err != nil {
		t.Fatal(err)
	}
	if want := []need{"pkg:a", "pkg:b"}; !reflect.DeepEqual(info.Actual.Needs, want) {
		t.Errorf("got %v", info.Actual.Needs)
	}
}

func TestACellRunsWithPackagesOnly(t *testing.T) {
	get := catalogOf(map[string][]string{"cell:hamlet": {"node"}, "node": nil, "cell:other": nil})
	packs, err := closure([]string{"cell:hamlet"}, get)
	if err != nil {
		t.Fatal(err)
	}
	if want := []pack{{"cell", "hamlet", hashOf("cell:hamlet")}, {"pkg", "node", hashOf("node")}}; !reflect.DeepEqual(packs, want) {
		t.Errorf("got %v", packs)
	}
	if _, err := closure([]string{"pkg:node", "cell:other"}, get); err == nil {
		t.Error("a cell was taken as what another runs with")
	}
}

func TestACellIsBoundFromItsOwnStore(t *testing.T) {
	machine := machineWith(t, map[string][]string{"cell:hamlet": {"bin/hamlet", "lib/libham.so"}})
	withMachine(t, machine, needsWith(map[string][]string{"cell:hamlet": nil}))
	o := &options{image: "cell:hamlet"}
	from, err := packSource(o)
	if err != nil {
		t.Fatal(err)
	}
	if got := from.config.Config.Entrypoint; !reflect.DeepEqual(got, []string{"/bin/hamlet"}) {
		t.Errorf("entrypoint %v", got)
	}
	rootfs := filepath.Join(t.TempDir(), "rootfs")
	if err := from.fill(rootfs); err != nil {
		t.Fatal(err)
	}
	hash := hashOf("cell:hamlet")
	if m := o.binds[0]; m.Destination != "/oath/store/cell/hamlet/"+hash || m.Source != filepath.Join(machine, "store/cell/hamlet", hash) {
		t.Errorf("got %+v", m)
	}
	link, err := os.Readlink(filepath.Join(rootfs, "bin/hamlet"))
	if err != nil || link != "/oath/store/cell/hamlet/"+hash+"/bin/hamlet" {
		t.Errorf("got %q, %v", link, err)
	}
	if live, _ := os.Readlink(filepath.Join(rootfs, "oath/store/cell/hamlet/live")); live != hash {
		t.Errorf("live is %q", live)
	}
}
