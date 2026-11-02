package main

import (
	"encoding/json"
	"os"
	"path/filepath"
	"reflect"
	"slices"
	"strings"
	"testing"
)

func TestPastaOpensOnlyWhatIsNamed(t *testing.T) {
	args, err := pastaArgs(&options{})
	if err != nil {
		t.Fatal(err)
	}
	joined := strings.Join(args, " ")
	for _, closed := range []string{"-t none", "-u none", "-T none", "-U none", "--no-map-gw"} {
		if !strings.Contains(joined, closed) {
			t.Errorf("no %q in %s", closed, joined)
		}
	}
	args, err = pastaArgs(&options{publish: list{"8080:80", "0.0.0.0:8443:443", "5353:53/udp"}, reach: list{"5432", "6000:7000"}})
	if err != nil {
		t.Fatal(err)
	}
	joined = strings.Join(args, " ")
	for _, opened := range []string{"-t 127.0.0.1/8080:80", "-t 0.0.0.0/8443:443", "-u 127.0.0.1/5353:53", "-T 5432:5432", "-T 6000:7000"} {
		if !strings.Contains(joined, opened) {
			t.Errorf("no %q in %s", opened, joined)
		}
	}
	for _, closed := range []string{"-t none", "-u none", "-T none"} {
		if strings.Contains(joined, closed) {
			t.Errorf("%q in %s", closed, joined)
		}
	}
	args, err = pastaArgs(&options{noNet: true})
	if err != nil {
		t.Fatal(err)
	}
	if !slices.Contains(args, "--splice-only") || slices.Contains(args, "--dns-forward") {
		t.Errorf("no network: %v", args)
	}
	for _, bad := range []options{
		{publish: list{"80"}}, {publish: list{"x:80"}}, {publish: list{"80:99999"}}, {publish: list{"80:80/sctp"}},
		{reach: list{"x"}}, {reach: list{"0"}},
	} {
		if _, err := pastaArgs(&bad); err == nil {
			t.Errorf("took %+v", bad)
		}
	}
}

func specOf(t *testing.T, o *options, image *imageConfig, u userIDs) map[string]any {
	t.Helper()
	if image == nil {
		image = &imageConfig{}
	}
	got, err := spec(o, image, u, "/dir")
	if err != nil {
		t.Fatal(err)
	}
	data, err := json.Marshal(got)
	if err != nil {
		t.Fatal(err)
	}
	var out map[string]any
	if err := json.Unmarshal(data, &out); err != nil {
		t.Fatal(err)
	}
	return out
}

func process(spec map[string]any) map[string]any { return spec["process"].(map[string]any) }

func strs(v any) []string {
	var out []string
	for _, s := range v.([]any) {
		out = append(out, s.(string))
	}
	return out
}

func TestArgs(t *testing.T) {
	image := &imageConfig{}
	image.Config.Entrypoint = []string{"/entry"}
	image.Config.Cmd = []string{"default"}
	for _, c := range []struct {
		o    options
		want []string
	}{
		{options{}, []string{"/entry", "default"}},
		{options{args: []string{"mine"}}, []string{"/entry", "mine"}},
		{options{entrypoint: "/other"}, []string{"/other"}},
		{options{entrypoint: "/other", args: []string{"mine"}}, []string{"/other", "mine"}},
	} {
		if got := strs(process(specOf(t, &c.o, image, userIDs{}))["args"]); !reflect.DeepEqual(got, c.want) {
			t.Errorf("%+v: %v", c.o, got)
		}
	}
	if _, err := spec(&options{}, &imageConfig{}, userIDs{}, "/dir"); err == nil {
		t.Error("ran nothing")
	}
}

func TestEnvironment(t *testing.T) {
	got, err := environment([]string{"PATH=/image", "A=1"}, []string{"A=2", "B=x=y"}, false)
	if err != nil {
		t.Fatal(err)
	}
	want := []string{"PATH=/image", "HOME=/root", "A=2", "B=x=y"}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("%v", got)
	}
	if _, err := environment(nil, []string{"NOEQUALS"}, false); err == nil {
		t.Error("took NOEQUALS")
	}
}

func TestLimitsAlwaysHold(t *testing.T) {
	for _, u := range []userIDs{{}, {101, 101}} {
		s := specOf(t, &options{args: []string{"x"}}, nil, u)
		if process(s)["noNewPrivileges"] != true {
			t.Error("noNewPrivileges")
		}
		linux := s["linux"].(map[string]any)
		if !slices.Contains(strs(linux["maskedPaths"]), "/proc/kcore") || !slices.Contains(strs(linux["readonlyPaths"]), "/proc/sys") {
			t.Error("masks")
		}
		var types []string
		for _, n := range linux["namespaces"].([]any) {
			types = append(types, n.(map[string]any)["type"].(string))
		}
		for _, want := range []string{"pid", "ipc", "uts", "mount"} {
			if !slices.Contains(types, want) {
				t.Errorf("no %s namespace", want)
			}
		}
		if slices.Contains(types, "network") {
			t.Error("a network namespace, which would lose pasta's")
		}
	}
	root := process(specOf(t, &options{args: []string{"x"}}, nil, userIDs{}))["capabilities"].(map[string]any)
	if caps := strs(root["bounding"]); slices.Contains(caps, "CAP_SYS_ADMIN") || !slices.Contains(caps, "CAP_CHOWN") {
		t.Errorf("%v", caps)
	}
	other := process(specOf(t, &options{args: []string{"x"}}, nil, userIDs{101, 101}))["capabilities"].(map[string]any)
	if len(other["bounding"].([]any)) != 0 {
		t.Errorf("a number holds %v", other)
	}
}

func TestSeccompRefusesNewNamespaces(t *testing.T) {
	syscalls := seccomp()["syscalls"].([]map[string]any)
	refused := map[string]bool{}
	flags := map[uint64]bool{}
	for _, s := range syscalls {
		args, masked := s["args"].([]map[string]any)
		for _, name := range s["names"].([]string) {
			if masked {
				flags[args[0]["value"].(uint64)] = true
			} else {
				refused[name] = true
			}
		}
	}
	for _, name := range []string{"unshare", "setns", "mount", "bpf", "io_uring_setup", "clone3", "keyctl", "ptrace"} {
		if !refused[name] {
			t.Errorf("%s is allowed", name)
		}
	}
	if !flags[0x10000000] {
		t.Error("clone(CLONE_NEWUSER) is allowed")
	}
}

func TestANumberIsMappedAndRootIsNot(t *testing.T) {
	s := specOf(t, &options{args: []string{"x"}}, nil, userIDs{})
	if _, ok := s["linux"].(map[string]any)["uidMappings"]; ok {
		t.Error("root needs no second namespace")
	}
	linux := specOf(t, &options{args: []string{"x"}}, nil, userIDs{101, 102})["linux"].(map[string]any)
	uid := linux["uidMappings"].([]any)[0].(map[string]any)
	gid := linux["gidMappings"].([]any)[0].(map[string]any)
	if uid["containerID"] != 101.0 || uid["hostID"] != 0.0 || uid["size"] != 1.0 || gid["containerID"] != 102.0 {
		t.Errorf("%v %v", uid, gid)
	}
}

func TestFindUser(t *testing.T) {
	rootfs := t.TempDir()
	if err := os.Mkdir(filepath.Join(rootfs, "etc"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(rootfs, "etc/passwd"), []byte("root:x:0:0::/root:/bin/sh\nweb:x:101:102::/:/bin/false\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(rootfs, "etc/group"), []byte("root:x:0:\nstaff:x:50:\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	for _, c := range []struct {
		flagged, imaged string
		want            userIDs
	}{
		{"", "", userIDs{0, 0}},
		{"", "web", userIDs{101, 102}},
		{"", "root", userIDs{0, 0}},
		{"", "101", userIDs{101, 102}},
		{"", "7", userIDs{7, 7}},
		{"", "7:8", userIDs{7, 8}},
		{"", "web:staff", userIDs{101, 50}},
		{"root", "web", userIDs{0, 0}},
		{"999", "web", userIDs{999, 999}},
	} {
		got, err := findUser(rootfs, c.flagged, c.imaged)
		if err != nil || got != c.want {
			t.Errorf("%q %q: %v %v", c.flagged, c.imaged, got, err)
		}
	}
	for _, bad := range []string{"ghost", "web:ghost", "-1"} {
		if _, err := findUser(rootfs, bad, ""); err == nil {
			t.Errorf("found %s", bad)
		}
	}
}

func TestAnImagesLinkedPasswdStaysInside(t *testing.T) {
	outer := t.TempDir()
	rootfs := filepath.Join(outer, "rootfs")
	if err := os.MkdirAll(filepath.Join(rootfs, "etc"), 0o755); err != nil {
		t.Fatal(err)
	}
	outside := filepath.Join(outer, "secret")
	if err := os.WriteFile(outside, []byte("web:x:101:102::/:/bin/false\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(outside, filepath.Join(rootfs, "etc/passwd")); err != nil {
		t.Fatal(err)
	}
	if _, err := findUser(rootfs, "web", ""); err == nil {
		t.Error("read a name through a link out of the image")
	}
}

func TestEnvFiles(t *testing.T) {
	file := filepath.Join(t.TempDir(), "env")
	if err := os.WriteFile(file, []byte("# a note\nA=1\n\nB=x y=z\r\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	got, err := readEnvFiles([]string{file})
	if err != nil || !reflect.DeepEqual(got, []string{"A=1", "B=x y=z"}) {
		t.Errorf("%q %v", got, err)
	}
	// -e goes over a file, which goes over the image.
	image := &imageConfig{}
	image.Config.Env = []string{"A=image", "C=image"}
	env := strs(process(specOf(t, &options{args: []string{"x"}, envFile: list{file}, env: list{"B=flag"}}, image, userIDs{}))["env"])
	for _, want := range []string{"A=1", "B=flag", "C=image"} {
		if !slices.Contains(env, want) {
			t.Errorf("%s not in %v", want, env)
		}
	}
	if _, err := readEnvFiles([]string{file + ".none"}); err == nil {
		t.Error("read a file that is not there")
	}
}

func TestMounts(t *testing.T) {
	host := t.TempDir()
	got, err := mountsFor(&options{volumes: list{host + ":/data", host + ":/ro:ro"}, tmpfs: list{"/tmp"}}, "/dir")
	if err != nil {
		t.Fatal(err)
	}
	held := map[string][]string{}
	for _, m := range got {
		held[m.Destination] = m.Options
	}
	if !slices.Contains(held["/data"], "rw") || !slices.Contains(held["/ro"], "ro") {
		t.Errorf("%v", held)
	}
	for _, dest := range []string{"/data", "/ro", "/tmp", "/etc/resolv.conf"} {
		if !slices.Contains(held[dest], "nosuid") || !slices.Contains(held[dest], "nodev") {
			t.Errorf("%s: %v", dest, held[dest])
		}
	}
	for _, bad := range []string{"relative:/x", host + ":relative", host, host + ":/x:wat", "/no/such/place:/x"} {
		if _, err := mountsFor(&options{volumes: list{bad}}, "/dir"); err == nil {
			t.Errorf("took %s", bad)
		}
	}
}

func TestWriteEtcDoesNotFollowTheImagesLinks(t *testing.T) {
	outer := t.TempDir()
	rootfs := filepath.Join(outer, "rootfs")
	if err := os.MkdirAll(filepath.Join(rootfs, "etc"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(filepath.Join(outer, "outside"), filepath.Join(rootfs, "etc/resolv.conf")); err != nil {
		t.Fatal(err)
	}
	if err := writeEtc(outer, rootfs, "x"); err != nil {
		t.Fatal(err)
	}
	has(t, filepath.Join(outer, "outside"), false)
	if fi, err := os.Lstat(filepath.Join(rootfs, "etc/resolv.conf")); err != nil || !fi.Mode().IsRegular() {
		t.Errorf("%v %v", fi, err)
	}
	if data, _ := os.ReadFile(filepath.Join(outer, "resolv.conf")); !strings.Contains(string(data), dnsAddress) {
		t.Errorf("%q", data)
	}
}

func TestStopSignal(t *testing.T) {
	for name, want := range map[string]string{
		"":        "terminated",
		"SIGQUIT": "quit",
		"QUIT":    "quit",
		"SIGINT":  "interrupt",
		"3":       "quit",
		"SIGWHAT": "terminated",
	} {
		if got := stopSignal(name).String(); got != want {
			t.Errorf("%q: %s", name, got)
		}
	}
}

func TestChainDoesNotFailWithoutAProcess(t *testing.T) {
	if crun, first := chain(1 << 30); crun != 0 || first != 0 {
		t.Errorf("%d %d", crun, first)
	}
}

func TestPruneKeepsWhatIsNamedAndRunning(t *testing.T) {
	s := storeOf(t)
	named, unnamed := s.image("sha256:aa"), s.image("sha256:bb")
	for _, d := range []string{named, unnamed} {
		if err := os.MkdirAll(d, 0o700); err != nil {
			t.Fatal(err)
		}
	}
	if err := s.setRef("x/y:z", "sha256:aa"); err != nil {
		t.Fatal(err)
	}
	for _, c := range []string{"dead", "alive"} {
		if err := os.MkdirAll(s.cell(c), 0o700); err != nil {
			t.Fatal(err)
		}
	}
	held, err := lock(s.cell("alive"))
	if err != nil {
		t.Fatal(err)
	}
	defer held.Close()
	if err := s.prune(); err != nil {
		t.Fatal(err)
	}
	has(t, named, true)
	has(t, unnamed, false)
	has(t, s.cell("alive"), true)
	has(t, s.cell("dead"), false)
}
