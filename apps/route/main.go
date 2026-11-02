// route is a small HTTP router for what sits behind a tunnel or other proxy: by
// host and path, to a local service, a redirect or a fixed answer. It holds no TLS,
// and is meant for a loopback address. Go's standard library only, so it has nothing
// to pin but pkg:go (R57).
//
//	route <config.json>          run; SIGHUP reads the file again, SIGTERM drains
//	route -check <config.json>   say whether the file is good
//
// The file, in which a route has one of "to", "redirect" or "status":
//
//	{
//	  "listen": "127.0.0.1:8080",
//	  "routes": [
//	    {"host": "a.example", "to": "127.0.0.1:9000"},
//	    {"host": "a.example", "path": "/admin", "to": "127.0.0.1:9001", "strip": "/admin",
//	     "request": {"X-Forwarded-Prefix": "/admin", "Cookie": ""},
//	     "response": {"X-Frame-Options": "DENY", "Set-Cookie": ""}},
//	    {"host": "a.example", "path": "/api", "to": "127.0.0.1:9002", "prefix": "/v2"},
//	    {"host": "a.example", "path": "/gone", "status": 404, "body": "not found\n"},
//	    {"host": "old.example", "redirect": "https://new.example", "status": 301},
//	    {"host": "*", "status": 404}
//	  ]
//	}
//
// The longest matching path wins, at a path segment's edge, and "*" stands in for a
// host no route names. "strip" takes a prefix off the path, then "prefix" puts one
// on. An empty header value removes that header. A redirect with no path of its own
// keeps the request's. X-Forwarded-For, -Proto and -Host that arrive are kept, since
// what stands in front knows them.
//
// What a route matched is what its service gets: repeated slashes collapse, and a
// path with a . or .. segment is refused, so neither can reach past a route.
package main

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"log"
	"net"
	"net/http"
	"net/http/httputil"
	"net/url"
	"os"
	"os/signal"
	"sort"
	"strings"
	"sync/atomic"
	"syscall"
	"time"
)

// Route is one rule of the file.
type Route struct {
	Host     string            `json:"host"`
	Path     string            `json:"path"`
	To       string            `json:"to"`
	Strip    string            `json:"strip"`
	Prefix   string            `json:"prefix"`
	Request  map[string]string `json:"request"`
	Response map[string]string `json:"response"`
	Redirect string            `json:"redirect"`
	Status   int               `json:"status"`
	Body     string            `json:"body"`

	proxy     *httputil.ReverseProxy
	keepsPath bool
}

// Config is the file.
type Config struct {
	Listen string   `json:"listen"`
	Routes []*Route `json:"routes"`

	byHost map[string][]*Route
}

func parse(data []byte) (*Config, error) {
	d := json.NewDecoder(bytes.NewReader(data))
	d.DisallowUnknownFields()
	c := &Config{byHost: map[string][]*Route{}}
	if err := d.Decode(c); err != nil {
		return nil, err
	}
	if c.Listen == "" {
		return nil, errors.New(`no "listen"`)
	}
	for i, r := range c.Routes {
		if err := r.prepare(); err != nil {
			return nil, fmt.Errorf("route %d (%s%s): %w", i, r.Host, r.Path, err)
		}
		c.byHost[r.Host] = append(c.byHost[r.Host], r)
	}
	for _, rs := range c.byHost {
		sort.SliceStable(rs, func(i, j int) bool { return len(rs[i].Path) > len(rs[j].Path) })
	}
	return c, nil
}

func (r *Route) prepare() error {
	r.Host = strings.ToLower(r.Host)
	if r.Host == "" {
		return errors.New(`no "host"`)
	}
	if r.Path == "" {
		r.Path = "/"
	}
	if !strings.HasPrefix(r.Path, "/") {
		return errors.New(`"path" must start with /`)
	}
	switch {
	case r.To != "" && (r.Redirect != "" || r.Status != 0):
		return errors.New(`"to" takes no "redirect" or "status"`)
	case r.To != "":
		if _, _, err := net.SplitHostPort(r.To); err != nil {
			return fmt.Errorf(`"to": %w`, err)
		}
		r.proxy = r.reverseProxy()
	case r.Redirect != "":
		if r.Status == 0 {
			r.Status = http.StatusFound
		}
		if r.Status < 300 || r.Status > 399 {
			return errors.New("a redirect's status is 3xx")
		}
		u, err := url.Parse(r.Redirect)
		if err != nil {
			return fmt.Errorf(`"redirect": %w`, err)
		}
		r.keepsPath = u.Path == "" && u.RawQuery == ""
	case r.Status != 0:
		if r.Status < 100 || r.Status > 599 {
			return errors.New("not a status")
		}
	default:
		return errors.New(`one of "to", "redirect" or "status"`)
	}
	return nil
}

var transport = &http.Transport{
	DialContext:         (&net.Dialer{Timeout: 5 * time.Second}).DialContext,
	MaxIdleConnsPerHost: 32,
	IdleConnTimeout:     90 * time.Second,
	DisableCompression:  true,
}

func (r *Route) reverseProxy() *httputil.ReverseProxy {
	to := &url.URL{Scheme: "http", Host: r.To}
	return &httputil.ReverseProxy{
		Transport:     transport,
		FlushInterval: -1,
		Rewrite: func(p *httputil.ProxyRequest) {
			esc := forwardedPath(p.In.URL, r.Strip)
			p.SetURL(to)
			p.Out.Host = p.In.Host
			setPath(p.Out.URL, r.Prefix+esc)
			forwarded(p)
			setHeaders(p.Out.Header, r.Request)
		},
		ModifyResponse: func(res *http.Response) error {
			setHeaders(res.Header, r.Response)
			return nil
		},
		ErrorHandler: func(w http.ResponseWriter, q *http.Request, err error) {
			log.Printf("%s%s to %s: %v", q.Host, q.URL.Path, r.To, err)
			http.Error(w, "bad gateway", http.StatusBadGateway)
		},
	}
}

// forwardedPath is the escaped path a service gets, its prefix stripped. A prefix
// spelled in escapes matched by its decoded path, so it is stripped from that.
func forwardedPath(u *url.URL, strip string) string {
	esc := clean(u.EscapedPath())
	if strip == "" {
		return esc
	}
	if under(esc, strip) {
		return trimPrefix(esc, strip)
	}
	if path := clean(u.Path); under(path, strip) {
		return (&url.URL{Path: trimPrefix(path, strip)}).EscapedPath()
	}
	return esc
}

// trimPrefix takes a prefix that path is under off it, leaving a path.
func trimPrefix(path, strip string) string {
	rest := strings.TrimPrefix(path, strings.TrimSuffix(strip, "/"))
	if rest == "" {
		return "/"
	}
	return rest
}

func setPath(u *url.URL, esc string) {
	path, err := url.PathUnescape(esc)
	if err != nil {
		path = esc
	}
	u.Path, u.RawPath = path, esc
}

// forwarded keeps what stands in front's account, and adds this hop.
func forwarded(p *httputil.ProxyRequest) {
	if v, ok := p.In.Header["X-Forwarded-For"]; ok {
		p.Out.Header["X-Forwarded-For"] = v
	}
	p.SetXForwarded()
	for _, h := range []string{"X-Forwarded-Host", "X-Forwarded-Proto"} {
		if v := p.In.Header.Get(h); v != "" {
			p.Out.Header.Set(h, v)
		}
	}
}

func setHeaders(h http.Header, set map[string]string) {
	for k, v := range set {
		if v == "" {
			h.Del(k)
		} else {
			h.Set(k, v)
		}
	}
}

func under(path, prefix string) bool {
	prefix = strings.TrimSuffix(prefix, "/")
	return prefix == "" || path == prefix || strings.HasPrefix(path, prefix+"/")
}

// clean collapses repeated slashes, which a service behind could collapse itself.
func clean(path string) string {
	for strings.Contains(path, "//") {
		path = strings.ReplaceAll(path, "//", "/")
	}
	return path
}

// dotted says whether a decoded path has a . or .. segment, which a service behind
// could resolve past the route that matched it. A backslash counts as a slash here,
// as some services take it for one.
func dotted(path string) bool {
	for _, s := range strings.Split(strings.ReplaceAll(path, `\`, "/"), "/") {
		if s == "." || s == ".." {
			return true
		}
	}
	return false
}

func (c *Config) find(host, path string) *Route {
	if h, _, err := net.SplitHostPort(host); err == nil {
		host = h
	}
	routes, named := c.byHost[strings.ToLower(host)]
	if !named {
		routes = c.byHost["*"]
	}
	for _, r := range routes {
		if under(path, r.Path) {
			return r
		}
	}
	return nil
}

func serve(current *atomic.Pointer[Config]) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, q *http.Request) {
		if dotted(q.URL.Path) {
			http.Error(w, "bad path", http.StatusBadRequest)
			return
		}
		r := current.Load().find(q.Host, clean(q.URL.Path))
		switch {
		case r == nil:
			http.NotFound(w, q)
		case r.proxy != nil:
			r.proxy.ServeHTTP(w, q)
		case r.Redirect != "":
			to := r.Redirect
			if r.keepsPath {
				to += q.URL.RequestURI()
			}
			http.Redirect(w, q, to, r.Status)
		default:
			setHeaders(w.Header(), r.Response)
			w.WriteHeader(r.Status)
			_, _ = w.Write([]byte(r.Body))
		}
	})
}

func load(file string) (*Config, error) {
	data, err := os.ReadFile(file)
	if err != nil {
		return nil, err
	}
	c, err := parse(data)
	if err != nil {
		return nil, fmt.Errorf("%s: %w", file, err)
	}
	return c, nil
}

func main() {
	check := flag.Bool("check", false, "say whether the file is good, and exit")
	flag.Parse()
	if flag.NArg() != 1 {
		fmt.Fprintln(os.Stderr, "usage: route [-check] <config.json>")
		os.Exit(2)
	}
	file := flag.Arg(0)
	c, err := load(file)
	if err != nil {
		log.Fatal(err)
	}
	if *check {
		fmt.Printf("%s: %d routes, on %s\n", file, len(c.Routes), c.Listen)
		return
	}
	var current atomic.Pointer[Config]
	current.Store(c)
	srv := &http.Server{
		Addr:              c.Listen,
		Handler:           serve(&current),
		ReadHeaderTimeout: 10 * time.Second,
		IdleTimeout:       2 * time.Minute,
		MaxHeaderBytes:    1 << 20,
	}
	hup := make(chan os.Signal, 1)
	signal.Notify(hup, syscall.SIGHUP)
	go func() {
		for range hup {
			next, err := load(file)
			switch {
			case err != nil:
				log.Printf("kept the routes it had: %v", err)
			case next.Listen != current.Load().Listen:
				log.Printf("kept the routes it had: listen moved to %s, which takes a restart", next.Listen)
			default:
				current.Store(next)
				log.Printf("%d routes", len(next.Routes))
			}
		}
	}()
	ctx, stop := signal.NotifyContext(context.Background(), syscall.SIGTERM, syscall.SIGINT)
	defer stop()
	go func() {
		<-ctx.Done()
		drain, cancel := context.WithTimeout(context.Background(), 10*time.Second)
		defer cancel()
		_ = srv.Shutdown(drain)
	}()
	log.Printf("%d routes, on %s", len(c.Routes), c.Listen)
	if err := srv.ListenAndServe(); !errors.Is(err, http.ErrServerClosed) {
		log.Fatal(err)
	}
}
