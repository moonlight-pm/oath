package main

import (
	"bufio"
	"bytes"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"io"
	"os"
	"os/exec"
	"os/signal"
	"path/filepath"
	"regexp"
	"runtime"
	"strconv"
	"strings"
	"syscall"
	"time"
)

var cellName = regexp.MustCompile(`^[a-z0-9][a-z0-9_.-]*$`)

// dnsAddress is where a cell finds a name server: pasta answers there, from the
// host's own.
const dnsAddress = "169.254.1.1"

// tiedCommand is the hidden command that pasta runs: see tied.
const tiedCommand = ".tied"

type list []string

func (l *list) String() string     { return strings.Join(*l, ",") }
func (l *list) Set(v string) error { *l = append(*l, v); return nil }

type options struct {
	name       string
	image      string
	args       []string
	publish    list
	reach      list
	volumes    list
	tmpfs      list
	env        list
	envFile    list
	workdir    string
	entrypoint string
	user       string
	readOnly   bool
	noNet      bool
	tty        bool
	stopAfter  time.Duration
	with       list
	binds      []mount
}

func (s *store) runCommand(args []string) (int, error) {
	o := &options{}
	fs := flag.NewFlagSet("run", flag.ExitOnError)
	fs.StringVar(&o.name, "name", "", "the cell's name; two cannot share one")
	fs.Var(&o.publish, "p", "[addr:]host:port[/udp], on 127.0.0.1 unless addr says else")
	fs.Var(&o.reach, "reach", "port[:hostport]: this cell's 127.0.0.1:port reaches the host's")
	fs.Var(&o.volumes, "v", "/host/path:/path[:ro]")
	fs.Var(&o.tmpfs, "tmpfs", "/path, a memory file system")
	fs.Var(&o.env, "e", "NAME=value, which every process listing shows")
	fs.Var(&o.envFile, "env-file", "a file of NAME=value lines, for what should not show")
	fs.Var(&o.with, "with", "pkg:name, another package a pkg: cell runs with")
	fs.StringVar(&o.workdir, "w", "", "the directory to start in")
	fs.StringVar(&o.entrypoint, "entrypoint", "", "the program, over the image's")
	fs.StringVar(&o.user, "user", "", "uid[:gid] or a name in the image, over its USER: the number this cell's one uid wears")
	fs.BoolVar(&o.readOnly, "read-only", false, "the rootfs is read-only")
	fs.BoolVar(&o.noNet, "no-net", false, "no way out: only -p and --reach")
	fs.BoolVar(&o.tty, "t", false, "a terminal")
	fs.DurationVar(&o.stopAfter, "stop-after", 10*time.Second, "how long a stop waits before SIGKILL")
	_ = fs.Parse(args)
	if fs.NArg() < 1 {
		usage()
	}
	o.image, o.args = fs.Arg(0), fs.Args()[1:]
	return s.run(o)
}

func (s *store) run(o *options) (int, error) {
	if os.Geteuid() == 0 {
		return 0, errors.New("not as root: a cell's root is the account that ran it")
	}
	if o.name == "" {
		o.name = fmt.Sprintf("run-%d", os.Getpid())
	}
	if !cellName.MatchString(o.name) {
		return 0, fmt.Errorf("%q is not a name", o.name)
	}
	var from *source
	var err error
	if strings.HasPrefix(o.image, "pkg:") {
		from, err = packSource(o)
	} else if len(o.with) > 0 {
		err = errors.New("--with is for a pkg: cell")
	} else {
		from, err = s.imageSource(o)
	}
	if err != nil {
		return 0, err
	}
	image := &from.config
	dir := s.cell(o.name)
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return 0, err
	}
	held, err := lock(dir)
	if errors.Is(err, syscall.EWOULDBLOCK) {
		return 0, fmt.Errorf("%s is running", o.name)
	}
	if err != nil {
		return 0, err
	}
	defer held.Close()
	// What a crash left.
	for _, old := range []string{"rootfs", "state"} {
		if err := forceRemove(filepath.Join(dir, old)); err != nil {
			return 0, err
		}
	}
	rootfs := filepath.Join(dir, "rootfs")
	if err := from.fill(rootfs); err != nil {
		return 0, err
	}
	defer forceRemove(dir)
	if err := writeEtc(dir, rootfs, o.name); err != nil {
		return 0, err
	}
	u, err := findUser(rootfs, o.user, image.Config.User)
	if err != nil {
		return 0, err
	}
	config, err := spec(o, image, u, dir)
	if err != nil {
		return 0, err
	}
	data, err := json.MarshalIndent(config, "", "  ")
	if err != nil {
		return 0, err
	}
	if err := os.WriteFile(filepath.Join(dir, "config.json"), data, 0o600); err != nil {
		return 0, err
	}
	if err := os.WriteFile(filepath.Join(dir, "image"), []byte(from.name+"\n"), 0o600); err != nil {
		return 0, err
	}
	if err := os.WriteFile(filepath.Join(dir, "cell.pid"), []byte(strconv.Itoa(os.Getpid())+"\n"), 0o600); err != nil {
		return 0, err
	}
	pasta, err := pastaArgs(o)
	if err != nil {
		return 0, err
	}
	self, err := os.Executable()
	if err != nil {
		return 0, err
	}
	pasta = append(pasta, "--", self, tiedCommand,
		"crun", "--root", filepath.Join(dir, "state"), "--cgroup-manager=disabled",
		"run", "--bundle", dir, o.name)
	cmd := exec.Command("pasta", pasta...)
	wire(cmd, o.tty)
	// If this process is killed, its lock goes, and so must what the lock stood for.
	cmd.SysProcAttr = &syscall.SysProcAttr{Pdeathsig: syscall.SIGKILL}
	return supervise(cmd, filepath.Join(dir, "pasta.pid"), stopSignal(image.Config.StopSignal), o.stopAfter)
}

// source is where a cell's root comes from: an image, or this machine's packages.
type source struct {
	name   string // what cell ls shows
	config imageConfig
	fill   func(rootfs string) error
}

// imageSource is an image's, pulled if it is not on disk. Each run gets a copy.
func (s *store) imageSource(o *options) (*source, error) {
	r, err := parseRef(o.image)
	if err != nil {
		return nil, err
	}
	digest := s.lookup(r)
	if digest == "" {
		if digest, err = s.pull(r); err != nil {
			return nil, err
		}
	}
	data, err := os.ReadFile(filepath.Join(s.image(digest), "config.json"))
	if err != nil {
		return nil, err
	}
	var image imageConfig
	if err := json.Unmarshal(data, &image); err != nil {
		return nil, err
	}
	return &source{r.String(), image, func(rootfs string) error {
		cp := exec.Command("cp", "-a", "--reflink=auto", filepath.Join(s.image(digest), "rootfs"), rootfs)
		cp.Stderr = os.Stderr
		if err := cp.Run(); err != nil {
			return fmt.Errorf("copying the image: %w", err)
		}
		return nil
	}}, nil
}

// wire gives cmd this process's standard files. Without a terminal its output goes through
// pipes made here: a program often logs by opening /dev/stderr, which opens the pipe anew
// under its owner's permissions, and the pipe PID 1 gives a service is root's.
func wire(cmd *exec.Cmd, tty bool) {
	cmd.Stdin, cmd.Stdout, cmd.Stderr = os.Stdin, os.Stdout, os.Stderr
	if tty {
		return
	}
	out, errs := passOn{os.Stdout}, passOn{os.Stderr}
	// One pipe for one file, so that the two stay in order.
	a, aerr := os.Stdout.Stat()
	b, berr := os.Stderr.Stat()
	if aerr == nil && berr == nil && os.SameFile(a, b) {
		errs = out
	}
	cmd.Stdout, cmd.Stderr = out, errs
	cmd.WaitDelay = time.Second
	// A reader that goes away ends the copying, not this process and its cleanup.
	signal.Notify(make(chan os.Signal, 1), syscall.SIGPIPE)
}

// passOn is a writer and not an *os.File, which is what makes exec.Cmd make a pipe.
type passOn struct{ io.Writer }

// tied becomes a command that dies with its parent, which is what pasta's child needs:
// killed, pasta would leave it running with no network. Both calls need one thread.
func tied(args []string) error {
	runtime.LockOSThread()
	if _, _, errno := syscall.RawSyscall(syscall.SYS_PRCTL, syscall.PR_SET_PDEATHSIG, uintptr(syscall.SIGKILL), 0); errno != 0 {
		return errno
	}
	path, err := exec.LookPath(args[0])
	if err != nil {
		return err
	}
	return syscall.Exec(path, args, os.Environ())
}

// supervise waits for a cell, and hands it the signals that come here. The first
// SIGTERM becomes what the image asked to be stopped with, and SIGKILL follows it.
func supervise(cmd *exec.Cmd, pidFile string, stop syscall.Signal, after time.Duration) (int, error) {
	signals := make(chan os.Signal, 8)
	signal.Notify(signals, syscall.SIGTERM, syscall.SIGINT, syscall.SIGHUP, syscall.SIGQUIT, syscall.SIGUSR1, syscall.SIGUSR2)
	defer signal.Stop(signals)
	// Pdeathsig belongs to the thread that starts it, so that thread stays.
	runtime.LockOSThread()
	defer runtime.UnlockOSThread()
	if err := cmd.Start(); err != nil {
		return 0, err
	}
	if err := os.WriteFile(pidFile, []byte(strconv.Itoa(cmd.Process.Pid)+"\n"), 0o600); err != nil {
		_ = cmd.Process.Kill()
		_ = cmd.Wait()
		return 0, err
	}
	done := make(chan error, 1)
	go func() { done <- cmd.Wait() }()
	var kill <-chan time.Time
	for {
		select {
		case err := <-done:
			// The program ended well, and something it left holds the pipes.
			if errors.Is(err, exec.ErrWaitDelay) {
				return 0, nil
			}
			var exit *exec.ExitError
			if errors.As(err, &exit) {
				if status, ok := exit.Sys().(syscall.WaitStatus); ok && status.Signaled() {
					return 128 + int(status.Signal()), nil
				}
				return exit.ExitCode(), nil
			}
			return 0, err
		case got := <-signals:
			sig := got.(syscall.Signal)
			if sig == syscall.SIGTERM {
				sig = stop
				if kill == nil {
					kill = time.After(after)
				}
			}
			send(cmd, sig)
		case <-kill:
			send(cmd, syscall.SIGKILL)
		}
	}
}

// send signals the container's first process, or if it has none yet, what starts it.
func send(cmd *exec.Cmd, sig syscall.Signal) {
	crun, first := chain(cmd.Process.Pid)
	switch {
	case first > 0:
		_ = syscall.Kill(first, sig)
	case crun > 0:
		_ = syscall.Kill(crun, sig)
	default:
		_ = cmd.Process.Signal(sig)
	}
}

// chain finds, by this machine's numbers, crun under pasta and the container's first
// process under crun. pasta gives them a pid namespace of its own, so what crun itself
// says, in a pid file or its state, means nothing out here.
func chain(pasta int) (crun, first int) {
	entries, err := os.ReadDir("/proc")
	if err != nil {
		return 0, 0
	}
	children := map[int][]int{}
	names := map[int]string{}
	for _, e := range entries {
		pid, err := strconv.Atoi(e.Name())
		if err != nil {
			continue
		}
		data, err := os.ReadFile("/proc/" + e.Name() + "/stat")
		if err != nil {
			continue
		}
		open, end := bytes.IndexByte(data, '('), bytes.LastIndexByte(data, ')')
		if open < 0 || end < open {
			continue
		}
		fields := strings.Fields(string(data[end+1:]))
		if len(fields) < 2 {
			continue
		}
		parent, err := strconv.Atoi(fields[1])
		if err != nil {
			continue
		}
		names[pid] = string(data[open+1 : end])
		children[parent] = append(children[parent], pid)
	}
	var find func(pid, depth int) int
	find = func(pid, depth int) int {
		if depth > 3 {
			return 0
		}
		for _, c := range children[pid] {
			if names[c] == "crun" {
				return c
			}
			if got := find(c, depth+1); got > 0 {
				return got
			}
		}
		return 0
	}
	crun = find(pasta, 0)
	if crun > 0 && len(children[crun]) > 0 {
		first = children[crun][0]
	}
	return crun, first
}

func readPid(file string) int {
	data, err := os.ReadFile(file)
	if err != nil {
		return 0
	}
	pid, err := strconv.Atoi(strings.TrimSpace(string(data)))
	if err != nil {
		return 0
	}
	return pid
}

func stopSignal(name string) syscall.Signal {
	switch strings.TrimPrefix(strings.ToUpper(name), "SIG") {
	case "INT", "2":
		return syscall.SIGINT
	case "QUIT", "3":
		return syscall.SIGQUIT
	case "HUP", "1":
		return syscall.SIGHUP
	case "USR1", "10":
		return syscall.SIGUSR1
	case "USR2", "12":
		return syscall.SIGUSR2
	case "WINCH", "28":
		return syscall.SIGWINCH
	}
	return syscall.SIGTERM
}

// pastaArgs lets in only what -p names, and out to this machine only what --reach
// names. pasta's own default would follow every port the host listens on.
func pastaArgs(o *options) ([]string, error) {
	args := []string{"--config-net", "--quiet", "--no-map-gw"}
	if o.noNet {
		args = append(args, "--splice-only")
	} else {
		args = append(args, "--dns-forward", dnsAddress)
	}
	tcp, udp, err := publishArgs(o.publish)
	if err != nil {
		return nil, err
	}
	reach := []string{}
	for _, v := range o.reach {
		port, hostPort, ok := strings.Cut(v, ":")
		if !ok {
			hostPort = port
		}
		if !isPort(port) || !isPort(hostPort) {
			return nil, fmt.Errorf("--reach %s: not port[:hostport]", v)
		}
		reach = append(reach, "-T", port+":"+hostPort)
	}
	if len(reach) == 0 {
		reach = []string{"-T", "none"}
	}
	args = append(args, tcp...)
	args = append(args, udp...)
	args = append(args, reach...)
	return append(args, "-U", "none"), nil
}

func publishArgs(publish []string) (tcp, udp []string, err error) {
	for _, v := range publish {
		spec, proto, _ := strings.Cut(v, "/")
		parts := strings.Split(spec, ":")
		addr := "127.0.0.1"
		if len(parts) == 3 {
			addr, parts = parts[0], parts[1:]
		}
		if len(parts) != 2 || !isPort(parts[0]) || !isPort(parts[1]) {
			return nil, nil, fmt.Errorf("-p %s: not [addr:]host:port[/udp]", v)
		}
		arg := addr + "/" + parts[0] + ":" + parts[1]
		switch proto {
		case "", "tcp":
			tcp = append(tcp, "-t", arg)
		case "udp":
			udp = append(udp, "-u", arg)
		default:
			return nil, nil, fmt.Errorf("-p %s: not tcp or udp", v)
		}
	}
	if len(tcp) == 0 {
		tcp = []string{"-t", "none"}
	}
	if len(udp) == 0 {
		udp = []string{"-u", "none"}
	}
	return tcp, udp, nil
}

func isPort(s string) bool {
	n, err := strconv.Atoi(s)
	return err == nil && n > 0 && n < 65536
}

// writeEtc writes the three files a container needs from its host, and makes sure
// that a link in the image does not carry their mounts somewhere else.
func writeEtc(dir, rootfs, name string) error {
	files := map[string]string{
		"resolv.conf": "nameserver " + dnsAddress + "\n",
		"hosts":       "127.0.0.1 localhost " + name + "\n::1 localhost " + name + "\n",
		"hostname":    name + "\n",
	}
	root, err := os.OpenRoot(rootfs)
	if err != nil {
		return err
	}
	defer root.Close()
	if err := root.MkdirAll("etc", 0o755); err != nil {
		return err
	}
	for file, content := range files {
		if err := os.WriteFile(filepath.Join(dir, file), []byte(content), 0o644); err != nil {
			return err
		}
		target := "etc/" + file
		if err := root.RemoveAll(target); err != nil {
			return err
		}
		if err := root.WriteFile(target, nil, 0o644); err != nil {
			return err
		}
	}
	return nil
}

type userIDs struct{ uid, gid uint32 }

// findUser is the number this cell's uid wears: --user over the image's USER, by
// number or by the image's own /etc/passwd and /etc/group.
func findUser(rootfs, flagged, imaged string) (userIDs, error) {
	want := flagged
	if want == "" {
		want = imaged
	}
	if want == "" {
		return userIDs{}, nil
	}
	root, err := os.OpenRoot(rootfs)
	if err != nil {
		return userIDs{}, err
	}
	defer root.Close()
	userPart, groupPart, _ := strings.Cut(want, ":")
	uid, gid, err := lookupID(root, "etc/passwd", userPart)
	if err != nil {
		return userIDs{}, fmt.Errorf("user %q: %w", want, err)
	}
	if groupPart != "" {
		if gid, _, err = lookupID(root, "etc/group", groupPart); err != nil {
			return userIDs{}, fmt.Errorf("group %q: %w", want, err)
		}
	}
	return userIDs{uid, gid}, nil
}

// lookupID reads a name's or a number's line from the image's passwd or group file,
// through the rootfs's own root, since the image chose where that file points. It
// gives the id and the one after it; a number not in the file is its own group too.
func lookupID(root *os.Root, file, name string) (uint32, uint32, error) {
	number, numberErr := strconv.ParseUint(name, 10, 31)
	f, err := root.Open(file)
	if err != nil {
		if numberErr == nil {
			return uint32(number), uint32(number), nil
		}
		return 0, 0, err
	}
	defer f.Close()
	lines := bufio.NewScanner(f)
	for lines.Scan() {
		parts := strings.Split(lines.Text(), ":")
		if len(parts) < 3 || (parts[0] != name && parts[2] != name) {
			continue
		}
		id, err := strconv.ParseUint(parts[2], 10, 31)
		if err != nil {
			continue
		}
		next := id
		if len(parts) > 3 {
			if g, err := strconv.ParseUint(parts[3], 10, 31); err == nil {
				next = g
			}
		}
		return uint32(id), uint32(next), nil
	}
	if numberErr == nil {
		return uint32(number), uint32(number), nil
	}
	return 0, 0, fmt.Errorf("not in the image's %s", file)
}

// mount is one entry of the OCI runtime spec's mounts.
type mount struct {
	Destination string   `json:"destination"`
	Type        string   `json:"type,omitempty"`
	Source      string   `json:"source,omitempty"`
	Options     []string `json:"options,omitempty"`
}

// capabilities is the set Docker and Podman give by default. In a user namespace they
// reach only what that namespace owns, and never the host's.
var capabilities = []string{
	"CAP_CHOWN", "CAP_DAC_OVERRIDE", "CAP_FOWNER", "CAP_FSETID", "CAP_KILL", "CAP_NET_BIND_SERVICE",
	"CAP_SETFCAP", "CAP_SETGID", "CAP_SETPCAP", "CAP_SETUID", "CAP_SYS_CHROOT",
}

func spec(o *options, image *imageConfig, u userIDs, dir string) (map[string]any, error) {
	args := image.Config.Entrypoint
	if o.entrypoint != "" {
		args = []string{o.entrypoint}
	}
	if len(o.args) > 0 {
		args = append(append([]string{}, args...), o.args...)
	} else if o.entrypoint == "" {
		args = append(append([]string{}, args...), image.Config.Cmd...)
	}
	if len(args) == 0 {
		return nil, errors.New("the image names no program, so give one")
	}
	cwd := o.workdir
	if cwd == "" {
		cwd = image.Config.WorkingDir
	}
	if cwd == "" {
		cwd = "/"
	}
	if !strings.HasPrefix(cwd, "/") {
		return nil, fmt.Errorf("the working directory %q is not absolute", cwd)
	}
	set, err := readEnvFiles(o.envFile)
	if err != nil {
		return nil, err
	}
	env, err := environment(image.Config.Env, append(set, o.env...), o.tty)
	if err != nil {
		return nil, err
	}
	mounts, err := mountsFor(o, dir)
	if err != nil {
		return nil, err
	}
	process := map[string]any{
		"terminal":        o.tty,
		"user":            map[string]any{"uid": u.uid, "gid": u.gid},
		"args":            args,
		"env":             env,
		"cwd":             cwd,
		"noNewPrivileges": true,
	}
	if u.uid == 0 {
		process["capabilities"] = map[string]any{
			"bounding": capabilities, "effective": capabilities, "permitted": capabilities,
		}
	} else {
		process["capabilities"] = map[string]any{"bounding": []string{}, "effective": []string{}, "permitted": []string{}}
	}
	namespaces := []map[string]string{{"type": "pid"}, {"type": "ipc"}, {"type": "uts"}, {"type": "mount"}}
	linux := map[string]any{
		"namespaces": namespaces,
		"maskedPaths": []string{
			"/proc/acpi", "/proc/asound", "/proc/kcore", "/proc/keys", "/proc/latency_stats", "/proc/timer_list",
			"/proc/timer_stats", "/proc/sched_debug", "/proc/scsi", "/sys/firmware", "/sys/dev/block",
		},
		"readonlyPaths": []string{"/proc/bus", "/proc/fs", "/proc/irq", "/proc/sys", "/proc/sysrq-trigger"},
		"seccomp":       seccomp(),
	}
	if u.uid != 0 || u.gid != 0 {
		// The second namespace is what lets the one uid wear another number: pasta's
		// holds it as 0, and this one maps that 0 to the number the image asked for.
		linux["namespaces"] = append(namespaces, map[string]string{"type": "user"})
		linux["uidMappings"] = []map[string]uint32{{"containerID": u.uid, "hostID": 0, "size": 1}}
		linux["gidMappings"] = []map[string]uint32{{"containerID": u.gid, "hostID": 0, "size": 1}}
	}
	return map[string]any{
		"ociVersion": "1.0.2",
		"process":    process,
		"root":       map[string]any{"path": "rootfs", "readonly": o.readOnly},
		"hostname":   o.name,
		"mounts":     mounts,
		"linux":      linux,
	}, nil
}

func environment(image, set []string, tty bool) ([]string, error) {
	values := map[string]string{}
	var order []string
	put := func(kv string) error {
		k, v, ok := strings.Cut(kv, "=")
		if !ok || k == "" {
			return fmt.Errorf("%q is not NAME=value", kv)
		}
		if _, had := values[k]; !had {
			order = append(order, k)
		}
		values[k] = v
		return nil
	}
	base := []string{"PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin", "HOME=/root"}
	if tty {
		base = append(base, "TERM=xterm")
	}
	for _, group := range [][]string{base, image, set} {
		for _, kv := range group {
			if err := put(kv); err != nil {
				return nil, err
			}
		}
	}
	env := make([]string, 0, len(order))
	for _, k := range order {
		env = append(env, k+"="+values[k])
	}
	return env, nil
}

// readEnvFiles reads NAME=value lines. Blank lines and those starting with # are left
// out, and a value is taken as it stands, quotes and all.
func readEnvFiles(files []string) ([]string, error) {
	var out []string
	for _, file := range files {
		data, err := os.ReadFile(file)
		if err != nil {
			return nil, err
		}
		for _, line := range strings.Split(string(data), "\n") {
			line = strings.TrimSuffix(line, "\r")
			if line != "" && !strings.HasPrefix(line, "#") {
				out = append(out, line)
			}
		}
	}
	return out, nil
}

func mountsFor(o *options, dir string) ([]mount, error) {
	mounts := []mount{
		{"/proc", "proc", "proc", nil},
		{"/dev", "tmpfs", "tmpfs", []string{"nosuid", "strictatime", "mode=755", "size=65536k"}},
		{"/dev/pts", "devpts", "devpts", []string{"nosuid", "noexec", "newinstance", "ptmxmode=0666", "mode=0620"}},
		{"/dev/shm", "tmpfs", "shm", []string{"nosuid", "noexec", "nodev", "mode=1777", "size=65536k"}},
		{"/dev/mqueue", "mqueue", "mqueue", []string{"nosuid", "noexec", "nodev"}},
		{"/sys", "sysfs", "sysfs", []string{"nosuid", "noexec", "nodev", "ro"}},
	}
	for _, file := range []string{"resolv.conf", "hosts", "hostname"} {
		mounts = append(mounts, mount{"/etc/" + file, "bind", filepath.Join(dir, file), []string{"bind", "ro", "nosuid", "nodev"}})
	}
	mounts = append(mounts, o.binds...)
	for _, v := range o.volumes {
		parts := strings.Split(v, ":")
		if len(parts) < 2 || len(parts) > 3 || !filepath.IsAbs(parts[0]) || !filepath.IsAbs(parts[1]) {
			return nil, fmt.Errorf("-v %s: not /host/path:/path[:ro]", v)
		}
		access := "rw"
		if len(parts) == 3 {
			if parts[2] != "ro" && parts[2] != "rw" {
				return nil, fmt.Errorf("-v %s: not ro or rw", v)
			}
			access = parts[2]
		}
		if _, err := os.Stat(parts[0]); err != nil {
			return nil, fmt.Errorf("-v %s: %w", v, err)
		}
		mounts = append(mounts, mount{parts[1], "bind", parts[0], []string{"rbind", access, "nosuid", "nodev"}})
	}
	for _, p := range o.tmpfs {
		if !filepath.IsAbs(p) {
			return nil, fmt.Errorf("--tmpfs %s: not absolute", p)
		}
		mounts = append(mounts, mount{p, "tmpfs", "tmpfs", []string{"nosuid", "nodev", "mode=1777"}})
	}
	return mounts, nil
}

// seccomp allows by default and refuses what a workload has no use for and where the
// kernel's own bugs are likeliest to be reached. New namespaces come first: any process
// could otherwise make itself a user namespace, and hold capabilities in it.
func seccomp() map[string]any {
	refused := []string{
		"acct", "add_key", "bpf", "clock_adjtime", "clock_settime", "create_module", "delete_module", "finit_module",
		"fsconfig", "fsmount", "fsopen", "fspick", "get_kernel_syms", "init_module", "io_uring_enter",
		"io_uring_register", "io_uring_setup", "ioperm", "iopl", "kexec_file_load", "kexec_load", "keyctl",
		"lookup_dcookie", "mount", "mount_setattr", "move_mount", "name_to_handle_at", "nfsservctl",
		"open_by_handle_at", "open_tree", "perf_event_open", "pivot_root", "process_vm_readv", "process_vm_writev",
		"ptrace", "query_module", "quotactl", "quotactl_fd", "reboot", "request_key", "setns", "settimeofday",
		"stime", "swapoff", "swapon", "sysfs", "_sysctl", "umount", "umount2", "unshare", "uselib", "userfaultfd",
		"ustat", "vm86", "vm86old",
	}
	syscalls := []map[string]any{{"names": refused, "action": "SCMP_ACT_ERRNO", "errnoRet": 1}}
	// clone with a new-namespace flag. clone3 hides its flags from a filter, so it is
	// answered as a kernel that lacks it, and libc goes back to clone.
	syscalls = append(syscalls, map[string]any{"names": []string{"clone3"}, "action": "SCMP_ACT_ERRNO", "errnoRet": 38})
	for _, flag := range []uint64{0x00020000, 0x02000000, 0x04000000, 0x08000000, 0x10000000, 0x20000000, 0x40000000} {
		syscalls = append(syscalls, map[string]any{
			"names":    []string{"clone"},
			"action":   "SCMP_ACT_ERRNO",
			"errnoRet": 1,
			"args":     []map[string]any{{"index": 0, "value": flag, "valueTwo": flag, "op": "SCMP_CMP_MASKED_EQ"}},
		})
	}
	return map[string]any{
		"defaultAction": "SCMP_ACT_ALLOW",
		"architectures": []string{"SCMP_ARCH_X86_64"},
		"syscalls":      syscalls,
	}
}

// exec runs a command in a running cell, under the same limits as its first process.
// crun has to do it from where the first crun is, in pasta's namespaces: the pid in
// its state, and the network it does not know of, only mean anything there.
func (s *store) exec(name string, command []string, tty bool, workdir string) (int, error) {
	if !s.running(name) {
		return 0, fmt.Errorf("%s is not running", name)
	}
	dir := s.cell(name)
	crun, _ := chain(readPid(filepath.Join(dir, "pasta.pid")))
	if crun <= 0 {
		return 0, fmt.Errorf("%s has not started", name)
	}
	data, err := os.ReadFile(filepath.Join(dir, "config.json"))
	if err != nil {
		return 0, err
	}
	var config struct {
		Process struct {
			User struct {
				UID uint32 `json:"uid"`
				GID uint32 `json:"gid"`
			} `json:"user"`
			Env []string `json:"env"`
			Cwd string   `json:"cwd"`
		} `json:"process"`
	}
	if err := json.Unmarshal(data, &config); err != nil {
		return 0, err
	}
	if workdir == "" {
		workdir = config.Process.Cwd
	}
	args := []string{"-U", "-n", "-p", "-m", "-u", "-i", "--preserve-credentials", "-t", strconv.Itoa(crun), "--",
		"crun", "--root", filepath.Join(dir, "state"), "exec", "--no-new-privs", "--cwd", workdir,
		"--user", fmt.Sprintf("%d:%d", config.Process.User.UID, config.Process.User.GID)}
	for _, kv := range config.Process.Env {
		args = append(args, "--env", kv)
	}
	if tty {
		args = append(args, "--tty")
	}
	args = append(args, name)
	args = append(args, command...)
	cmd := exec.Command("nsenter", args...)
	wire(cmd, tty)
	if err := cmd.Run(); err != nil && !errors.Is(err, exec.ErrWaitDelay) {
		var exit *exec.ExitError
		if errors.As(err, &exit) {
			return exit.ExitCode(), nil
		}
		return 0, err
	}
	return 0, nil
}

// stop asks a running cell's own process to stop, and leaves the waiting to it.
func (s *store) stop(name string) error {
	if !s.running(name) {
		return fmt.Errorf("%s is not running", name)
	}
	pid := readPid(filepath.Join(s.cell(name), "cell.pid"))
	if pid <= 0 {
		return fmt.Errorf("%s: no process recorded", name)
	}
	return syscall.Kill(pid, syscall.SIGTERM)
}
