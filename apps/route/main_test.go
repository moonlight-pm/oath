package main

import (
	"bufio"
	"encoding/json"
	"fmt"
	"io"
	"net"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"sync/atomic"
	"syscall"
	"testing"
	"time"
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

// backend answers with what it was asked, as JSON.
func backend(t *testing.T) string {
	t.Helper()
	s := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, q *http.Request) {
		w.Header().Set("Set-Cookie", "a=1")
		w.Header().Set("X-Server", "backend")
		_ = json.NewEncoder(w).Encode(map[string]string{
			"path":   q.URL.Path,
			"raw":    q.URL.RawPath,
			"query":  q.URL.RawQuery,
			"host":   q.Host,
			"cookie": q.Header.Get("Cookie"),
			"prefix": q.Header.Get("X-Forwarded-Prefix"),
			"for":    q.Header.Get("X-Forwarded-For"),
			"proto":  q.Header.Get("X-Forwarded-Proto"),
		})
	}))
	t.Cleanup(s.Close)
	return strings.TrimPrefix(s.URL, "http://")
}

func router(t *testing.T, config string) *httptest.Server {
	t.Helper()
	c, err := parse([]byte(config))
	if err != nil {
		t.Fatal(err)
	}
	var current atomic.Pointer[Config]
	current.Store(c)
	s := httptest.NewServer(serve(&current))
	t.Cleanup(s.Close)
	return s
}

func ask(t *testing.T, s *httptest.Server, host, path string, headers map[string]string) (*http.Response, map[string]string) {
	t.Helper()
	q, err := http.NewRequest("GET", s.URL+path, nil)
	if err != nil {
		t.Fatal(err)
	}
	q.Host = host
	for k, v := range headers {
		q.Header.Set(k, v)
	}
	client := &http.Client{CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse }}
	r, err := client.Do(q)
	if err != nil {
		t.Fatal(err)
	}
	defer r.Body.Close()
	body, _ := io.ReadAll(r.Body)
	var got map[string]string
	_ = json.Unmarshal(body, &got)
	return r, got
}

func TestRoutes(t *testing.T) {
	b := backend(t)
	s := router(t, fmt.Sprintf(`{"listen": ":0", "routes": [
		{"host": "a.test", "to": %[1]q},
		{"host": "a.test", "path": "/admin", "to": %[1]q, "strip": "/admin",
		 "request": {"X-Forwarded-Prefix": "/admin"}, "response": {"X-Frame-Options": "DENY"}},
		{"host": "a.test", "path": "/api", "to": %[1]q, "prefix": "/v2",
		 "request": {"Cookie": ""}, "response": {"Set-Cookie": ""}},
		{"host": "a.test", "path": "/gone", "status": 404, "body": "not found\n"},
		{"host": "old.test", "redirect": "https://new.test", "status": 301},
		{"host": "*", "path": "/any", "status": 204}
	]}`, b))
	cookie := map[string]string{"Cookie": "s=1"}

	r, got := ask(t, s, "a.test", "/x?y=1", cookie)
	if got["path"] != "/x" || got["query"] != "y=1" || got["cookie"] != "s=1" || got["host"] != "a.test" {
		t.Errorf("plain: %v", got)
	}
	if r.Header.Get("Set-Cookie") != "a=1" {
		t.Errorf("plain keeps Set-Cookie: %v", r.Header)
	}

	r, got = ask(t, s, "A.TEST:443", "/admin/x", nil)
	if got["path"] != "/x" || got["prefix"] != "/admin" {
		t.Errorf("strip: %v", got)
	}
	if r.Header.Get("X-Frame-Options") != "DENY" {
		t.Errorf("response header: %v", r.Header)
	}
	if _, got = ask(t, s, "a.test", "/admin", nil); got["path"] != "/" {
		t.Errorf("strip to nothing: %v", got)
	}
	// A segment's edge, not a string's.
	if _, got = ask(t, s, "a.test", "/adminx", nil); got["path"] != "/adminx" {
		t.Errorf("/adminx is not under /admin: %v", got)
	}

	r, got = ask(t, s, "a.test", "/api/y", cookie)
	if got["path"] != "/v2/api/y" || got["cookie"] != "" {
		t.Errorf("prefix, and a request header removed: %v", got)
	}
	if r.Header.Get("Set-Cookie") != "" || r.Header.Get("X-Server") != "backend" {
		t.Errorf("a response header removed: %v", r.Header)
	}

	if r, _ = ask(t, s, "a.test", "/gone/z", nil); r.StatusCode != 404 {
		t.Errorf("status: %d", r.StatusCode)
	}
	r, _ = ask(t, s, "old.test", "/p?q=1", nil)
	if r.StatusCode != 301 || r.Header.Get("Location") != "https://new.test/p?q=1" {
		t.Errorf("redirect: %d %v", r.StatusCode, r.Header)
	}
	if r, _ = ask(t, s, "else.test", "/any", nil); r.StatusCode != 204 {
		t.Errorf("* stands in for another host: %d", r.StatusCode)
	}
	if r, _ = ask(t, s, "else.test", "/none", nil); r.StatusCode != 404 {
		t.Errorf("no route: %d", r.StatusCode)
	}
}

func TestDotSegmentsCannotPassARoute(t *testing.T) {
	b := backend(t)
	s := router(t, fmt.Sprintf(`{"listen": ":0", "routes": [
		{"host": "a.test", "path": "/public", "to": %q},
		{"host": "a.test", "status": 403}
	]}`, b))
	// By hand, since a client would clean these up.
	for _, path := range []string{
		"/public/../admin", "/public/%2e%2e/admin", "/public/..%2fadmin",
		"/public/./x", `/public/..\admin`, "/public/..%5cadmin",
	} {
		conn, err := net.Dial("tcp", strings.TrimPrefix(s.URL, "http://"))
		if err != nil {
			t.Fatal(err)
		}
		fmt.Fprintf(conn, "GET %s HTTP/1.1\r\nHost: a.test\r\n\r\n", path)
		r, err := http.ReadResponse(bufio.NewReader(conn), nil)
		if err != nil {
			t.Fatal(err)
		}
		if r.StatusCode != 400 {
			t.Errorf("%s: %d", path, r.StatusCode)
		}
		_ = r.Body.Close()
		_ = conn.Close()
	}
}

func TestRepeatedSlashesCannotPassARoute(t *testing.T) {
	b := backend(t)
	s := router(t, fmt.Sprintf(`{"listen": ":0", "routes": [
		{"host": "a.test", "to": %q},
		{"host": "a.test", "path": "/admin", "status": 403}
	]}`, b))
	for _, path := range []string{"//admin", "//admin/x", "/%2Fadmin"} {
		if r, _ := ask(t, s, "a.test", path, nil); r.StatusCode != 403 {
			t.Errorf("%s: %d", path, r.StatusCode)
		}
	}
	if _, got := ask(t, s, "a.test", "/a//b", nil); got["path"] != "/a/b" {
		t.Errorf("the service gets what matched: %v", got)
	}
}

func TestAPrefixInEscapesIsStillStripped(t *testing.T) {
	b := backend(t)
	s := router(t, fmt.Sprintf(`{"listen": ":0", "routes": [
		{"host": "a.test", "path": "/admin", "to": %q, "strip": "/admin"}
	]}`, b))
	if _, got := ask(t, s, "a.test", "/%61dmin/x", nil); got["path"] != "/x" {
		t.Errorf("%v", got)
	}
}

func TestAQueryGoIsWaryOfDoesNotPass(t *testing.T) {
	b := backend(t)
	s := router(t, fmt.Sprintf(`{"listen": ":0", "routes": [{"host": "a.test", "to": %q}]}`, b))
	if _, got := ask(t, s, "a.test", "/x?a=1;b=2&c=3", nil); got["query"] != "c=3" {
		t.Errorf("%v", got)
	}
}

func TestANamedHostDoesNotFallToStar(t *testing.T) {
	s := router(t, `{"listen": ":0", "routes": [
		{"host": "a.test", "path": "/x", "status": 204},
		{"host": "*", "status": 200}
	]}`)
	if r, _ := ask(t, s, "a.test", "/y", nil); r.StatusCode != 404 {
		t.Errorf("%d", r.StatusCode)
	}
	if r, _ := ask(t, s, "b.test", "/y", nil); r.StatusCode != 200 {
		t.Errorf("%d", r.StatusCode)
	}
}

func TestARedirectWithAPathGoesThere(t *testing.T) {
	s := router(t, `{"listen": ":0", "routes": [{"host": "a.test", "redirect": "https://b.test/home"}]}`)
	r, _ := ask(t, s, "a.test", "/x?y=1", nil)
	if r.StatusCode != 302 || r.Header.Get("Location") != "https://b.test/home" {
		t.Errorf("%d %v", r.StatusCode, r.Header)
	}
}

func TestAnEscapedSlashStaysEscaped(t *testing.T) {
	b := backend(t)
	s := router(t, fmt.Sprintf(`{"listen": ":0", "routes": [
		{"host": "a.test", "path": "/v2", "to": %q, "strip": "/v2"}
	]}`, b))
	if _, got := ask(t, s, "a.test", "/v2/a%2Fb/c", nil); got["raw"] != "/a%2Fb/c" {
		t.Errorf("%v", got)
	}
}

func TestForwardedHeadersThatArriveAreKept(t *testing.T) {
	b := backend(t)
	s := router(t, fmt.Sprintf(`{"listen": ":0", "routes": [{"host": "a.test", "to": %q}]}`, b))
	_, got := ask(t, s, "a.test", "/", map[string]string{"X-Forwarded-For": "203.0.113.9", "X-Forwarded-Proto": "https"})
	if !strings.HasPrefix(got["for"], "203.0.113.9, ") || got["proto"] != "https" {
		t.Errorf("%v", got)
	}
}

func TestAnUpgradePassesThrough(t *testing.T) {
	up := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, q *http.Request) {
		conn, rw, err := w.(http.Hijacker).Hijack()
		if err != nil {
			t.Error(err)
			return
		}
		defer conn.Close()
		fmt.Fprint(rw, "HTTP/1.1 101 Switching Protocols\r\nConnection: Upgrade\r\nUpgrade: echo\r\n\r\n")
		_ = rw.Flush()
		line, _ := rw.ReadString('\n')
		fmt.Fprint(rw, "echo "+line)
		_ = rw.Flush()
	}))
	defer up.Close()
	s := router(t, fmt.Sprintf(`{"listen": ":0", "routes": [{"host": "a.test", "to": %q}]}`, strings.TrimPrefix(up.URL, "http://")))
	conn, err := net.Dial("tcp", strings.TrimPrefix(s.URL, "http://"))
	if err != nil {
		t.Fatal(err)
	}
	defer conn.Close()
	_ = conn.SetDeadline(time.Now().Add(5 * time.Second))
	fmt.Fprint(conn, "GET / HTTP/1.1\r\nHost: a.test\r\nConnection: Upgrade\r\nUpgrade: echo\r\n\r\n")
	br := bufio.NewReader(conn)
	r, err := http.ReadResponse(br, nil)
	if err != nil || r.StatusCode != 101 {
		t.Fatalf("%v %v", r, err)
	}
	fmt.Fprint(conn, "hello\n")
	if line, _ := br.ReadString('\n'); line != "echo hello\n" {
		t.Errorf("%q", line)
	}
}

func TestAStreamIsNotHeldBack(t *testing.T) {
	release := make(chan struct{})
	up := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, q *http.Request) {
		fmt.Fprintln(w, "first")
		w.(http.Flusher).Flush()
		<-release
	}))
	defer up.Close()
	defer close(release)
	s := router(t, fmt.Sprintf(`{"listen": ":0", "routes": [{"host": "a.test", "to": %q}]}`, strings.TrimPrefix(up.URL, "http://")))
	q, _ := http.NewRequest("GET", s.URL+"/", nil)
	q.Host = "a.test"
	r, err := http.DefaultClient.Do(q)
	if err != nil {
		t.Fatal(err)
	}
	defer r.Body.Close()
	got := make(chan string, 1)
	go func() {
		line, _ := bufio.NewReader(r.Body).ReadString('\n')
		got <- line
	}()
	select {
	case line := <-got:
		if line != "first\n" {
			t.Errorf("%q", line)
		}
	case <-time.After(5 * time.Second):
		t.Error("the first line waited for the end of the response")
	}
}

func TestABadFileIsRefused(t *testing.T) {
	for _, bad := range []string{
		`{}`,
		`{"listen": ":0", "routes": [{"host": "a", "to": "x"}]}`,
		`{"listen": ":0", "routes": [{"to": "a:1"}]}`,
		`{"listen": ":0", "routes": [{"host": "a"}]}`,
		`{"listen": ":0", "routes": [{"host": "a", "to": "a:1", "status": 200}]}`,
		`{"listen": ":0", "routes": [{"host": "a", "redirect": "b", "status": 200}]}`,
		`{"listen": ":0", "routes": [{"host": "a", "path": "x", "status": 200}]}`,
		`{"listen": ":0", "routes": [{"host": "a", "status": 200, "typo": 1}]}`,
	} {
		if _, err := parse([]byte(bad)); err == nil {
			t.Errorf("accepted %s", bad)
		}
	}
}
