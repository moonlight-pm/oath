package main

import (
	"compress/gzip"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
	"net/url"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"
	"time"
)

var (
	refRegistry = regexp.MustCompile(`^[A-Za-z0-9]+([.-][A-Za-z0-9]+)*(:[0-9]+)?$`)
	refRepo     = regexp.MustCompile(`^[a-z0-9]+((\.|_|__|-+)[a-z0-9]+)*(/[a-z0-9]+((\.|_|__|-+)[a-z0-9]+)*)*$`)
	refTag      = regexp.MustCompile(`^[A-Za-z0-9_][A-Za-z0-9_.-]{0,127}$`)
	refDigest   = regexp.MustCompile(`^sha256:[0-9a-f]{64}$`)
	challenge   = regexp.MustCompile(`([a-zA-Z]+)="([^"]*)"`)
)

// ref is an image's name.
type ref struct{ registry, repo, tag, digest string }

func parseRef(s string) (ref, error) {
	r := ref{registry: "docker.io"}
	name := s
	if head, digest, ok := strings.Cut(name, "@"); ok {
		name, r.digest = head, digest
		if !refDigest.MatchString(digest) {
			return r, fmt.Errorf("%s: not a sha256 digest", s)
		}
	}
	if first, rest, ok := strings.Cut(name, "/"); ok && (strings.ContainsAny(first, ".:") || first == "localhost") {
		r.registry, name = first, rest
	}
	if i := strings.LastIndex(name, ":"); i >= 0 {
		name, r.tag = name[:i], name[i+1:]
		if !refTag.MatchString(r.tag) {
			return r, fmt.Errorf("%s: not a tag", s)
		}
	}
	if r.tag == "" && r.digest == "" {
		r.tag = "latest"
	}
	if r.registry == "docker.io" && !strings.Contains(name, "/") {
		name = "library/" + name
	}
	r.repo = name
	if !refRegistry.MatchString(r.registry) || !refRepo.MatchString(r.repo) {
		return r, fmt.Errorf("%s: not an image name", s)
	}
	return r, nil
}

func (r ref) String() string {
	s := r.registry + "/" + r.repo
	if r.tag != "" {
		s += ":" + r.tag
	}
	if r.digest != "" {
		s += "@" + r.digest
	}
	return s
}

// reference is what the registry's manifest path takes: a digest over a tag.
func (r ref) reference() string {
	if r.digest != "" {
		return r.digest
	}
	return r.tag
}

// loopback says whether a registry is this machine, the one place plain HTTP goes.
func loopback(hostport string) bool {
	host := hostport
	if h, _, err := net.SplitHostPort(hostport); err == nil {
		host = h
	}
	if host == "localhost" {
		return true
	}
	ip := net.ParseIP(host)
	return ip != nil && ip.IsLoopback()
}

// descriptor points at a blob. A descriptor's "urls" are never read: an image does
// not get to send cell, or a login, somewhere else.
type descriptor struct {
	MediaType string `json:"mediaType"`
	Digest    string `json:"digest"`
	Size      int64  `json:"size"`
	Platform  *struct {
		Architecture string `json:"architecture"`
		OS           string `json:"os"`
	} `json:"platform"`
}

type manifest struct {
	SchemaVersion int          `json:"schemaVersion"`
	Config        descriptor   `json:"config"`
	Layers        []descriptor `json:"layers"`
	Manifests     []descriptor `json:"manifests"`
}

// imageConfig is the part of an image's config that a run reads.
type imageConfig struct {
	Architecture string `json:"architecture"`
	OS           string `json:"os"`
	Config       struct {
		User       string
		Env        []string
		Entrypoint []string
		Cmd        []string
		WorkingDir string
		StopSignal string
	} `json:"config"`
	RootFS struct {
		DiffIDs []string `json:"diff_ids"`
	} `json:"rootfs"`
}

const (
	acceptManifest = "application/vnd.oci.image.index.v1+json, " +
		"application/vnd.docker.distribution.manifest.list.v2+json, " +
		"application/vnd.oci.image.manifest.v1+json, " +
		"application/vnd.docker.distribution.manifest.v2+json"
	maxMetadata = 8 << 20
)

type client struct {
	http   *http.Client
	base   string
	host   string
	repo   string
	user   string
	pass   string
	header string
}

func (s *store) newClient(r ref) (*client, error) {
	host := r.registry
	if host == "docker.io" {
		host = "registry-1.docker.io"
	}
	scheme := "https"
	if loopback(host) {
		scheme = "http"
	}
	user, pass, err := s.credentials(r.registry)
	if err != nil {
		return nil, err
	}
	return &client{
		http: &http.Client{Transport: &http.Transport{
			DialContext:           (&net.Dialer{Timeout: 15 * time.Second}).DialContext,
			TLSHandshakeTimeout:   15 * time.Second,
			ResponseHeaderTimeout: 60 * time.Second,
			DisableCompression:    true,
		}},
		base: scheme + "://" + host,
		host: host,
		repo: r.repo,
		user: user,
		pass: pass,
	}, nil
}

func (c *client) get(path, accept string) (*http.Response, error) {
	for try := 0; ; try++ {
		q, err := http.NewRequest("GET", c.base+path, nil)
		if err != nil {
			return nil, err
		}
		if accept != "" {
			q.Header.Set("Accept", accept)
		}
		if c.header != "" {
			q.Header.Set("Authorization", c.header)
		}
		res, err := c.http.Do(q)
		if err != nil {
			return nil, err
		}
		if res.StatusCode == http.StatusUnauthorized && try == 0 {
			how := res.Header.Get("WWW-Authenticate")
			_ = res.Body.Close()
			if err := c.authorize(how); err != nil {
				return nil, err
			}
			continue
		}
		if res.StatusCode != http.StatusOK {
			_ = res.Body.Close()
			return nil, fmt.Errorf("%s%s: %s", c.host, path, res.Status)
		}
		return res, nil
	}
}

// authorize answers a registry's challenge, with a token where it asks for one.
func (c *client) authorize(how string) error {
	scheme, params, _ := strings.Cut(how, " ")
	basic := "Basic " + base64.StdEncoding.EncodeToString([]byte(c.user+":"+c.pass))
	switch strings.ToLower(scheme) {
	case "basic":
		if c.user == "" {
			return fmt.Errorf("%s wants a login: cell login %s <user>", c.host, c.host)
		}
		c.header = basic
		return nil
	case "bearer":
	default:
		return fmt.Errorf("%s: cannot answer %q", c.host, how)
	}
	found := map[string]string{}
	for _, m := range challenge.FindAllStringSubmatch(params, -1) {
		found[strings.ToLower(m[1])] = m[2]
	}
	realm, err := url.Parse(found["realm"])
	if err != nil || realm.Host == "" {
		return fmt.Errorf("%s: no realm in %q", c.host, how)
	}
	if realm.Scheme != "https" && !loopback(realm.Host) {
		return fmt.Errorf("%s: its token server is not HTTPS: %s", c.host, realm)
	}
	query := realm.Query()
	if v := found["service"]; v != "" {
		query.Set("service", v)
	}
	scope := found["scope"]
	if scope == "" {
		scope = "repository:" + c.repo + ":pull"
	}
	query.Set("scope", scope)
	realm.RawQuery = query.Encode()
	q, err := http.NewRequest("GET", realm.String(), nil)
	if err != nil {
		return err
	}
	if c.user != "" {
		q.Header.Set("Authorization", basic)
	}
	res, err := c.http.Do(q)
	if err != nil {
		return err
	}
	defer res.Body.Close()
	if res.StatusCode != http.StatusOK {
		return fmt.Errorf("%s: token: %s", c.host, res.Status)
	}
	var token struct {
		Token       string `json:"token"`
		AccessToken string `json:"access_token"`
	}
	if err := json.NewDecoder(io.LimitReader(res.Body, maxMetadata)).Decode(&token); err != nil {
		return fmt.Errorf("%s: token: %w", c.host, err)
	}
	if token.Token == "" {
		token.Token = token.AccessToken
	}
	if token.Token == "" {
		return fmt.Errorf("%s: an empty token", c.host)
	}
	c.header = "Bearer " + token.Token
	return nil
}

func digestOf(data []byte) string {
	sum := sha256.Sum256(data)
	return "sha256:" + hex.EncodeToString(sum[:])
}

// metadata reads a small response whole, limited.
func metadata(res *http.Response) ([]byte, error) {
	defer res.Body.Close()
	data, err := io.ReadAll(io.LimitReader(res.Body, maxMetadata+1))
	if err != nil {
		return nil, err
	}
	if len(data) > maxMetadata {
		return nil, errors.New("metadata too large")
	}
	return data, nil
}

// manifest fetches one and returns its digest, checked if the reference is one.
func (c *client) manifest(reference string) ([]byte, string, error) {
	res, err := c.get("/v2/"+c.repo+"/manifests/"+reference, acceptManifest)
	if err != nil {
		return nil, "", err
	}
	data, err := metadata(res)
	if err != nil {
		return nil, "", err
	}
	digest := digestOf(data)
	if strings.HasPrefix(reference, "sha256:") && digest != reference {
		return nil, "", fmt.Errorf("manifest %s came back as %s", reference, digest)
	}
	return data, digest, nil
}

// blob saves a descriptor's bytes to a file, and refuses them if they are not its.
func (c *client) blob(d descriptor, file string) error {
	if !refDigest.MatchString(d.Digest) {
		return fmt.Errorf("not a sha256 digest: %q", d.Digest)
	}
	res, err := c.get("/v2/"+c.repo+"/blobs/"+d.Digest, "")
	if err != nil {
		return err
	}
	defer res.Body.Close()
	f, err := os.Create(file)
	if err != nil {
		return err
	}
	defer f.Close()
	h := sha256.New()
	if _, err := io.Copy(io.MultiWriter(f, h), res.Body); err != nil {
		return fmt.Errorf("%s: %w", d.Digest, err)
	}
	if got := "sha256:" + hex.EncodeToString(h.Sum(nil)); got != d.Digest {
		return fmt.Errorf("blob %s came back as %s", d.Digest, got)
	}
	return f.Close()
}

// pull fetches an image if it is not on disk, and points its name at it. The digest
// it returns is the one for this machine's platform, even from a multi-platform name.
func (s *store) pull(r ref) (string, error) {
	c, err := s.newClient(r)
	if err != nil {
		return "", err
	}
	data, digest, err := c.manifest(r.reference())
	if err != nil {
		return "", err
	}
	var m manifest
	if err := json.Unmarshal(data, &m); err != nil {
		return "", fmt.Errorf("%s: %w", r, err)
	}
	if len(m.Manifests) > 0 {
		var pick *descriptor
		for i, d := range m.Manifests {
			if d.Platform != nil && d.Platform.OS == "linux" && d.Platform.Architecture == "amd64" {
				pick = &m.Manifests[i]
				break
			}
		}
		if pick == nil {
			return "", fmt.Errorf("%s has no linux/amd64", r)
		}
		if !refDigest.MatchString(pick.Digest) {
			return "", fmt.Errorf("%s: not a sha256 digest: %q", r, pick.Digest)
		}
		if data, digest, err = c.manifest(pick.Digest); err != nil {
			return "", err
		}
		m = manifest{}
		if err := json.Unmarshal(data, &m); err != nil {
			return "", fmt.Errorf("%s: %w", r, err)
		}
	}
	if m.SchemaVersion != 2 || m.Config.Digest == "" || len(m.Layers) == 0 {
		return "", fmt.Errorf("%s: not an image manifest cell knows", r)
	}
	if _, err := os.Stat(s.image(digest)); err == nil {
		return digest, s.setRef(r.String(), digest)
	}
	tmp, err := os.MkdirTemp(s.images(), ".pull-")
	if err != nil {
		return "", err
	}
	defer forceRemove(tmp)
	configFile := filepath.Join(tmp, "config.json")
	if err := c.blob(m.Config, configFile); err != nil {
		return "", err
	}
	configData, err := os.ReadFile(configFile)
	if err != nil {
		return "", err
	}
	var config imageConfig
	if err := json.Unmarshal(configData, &config); err != nil {
		return "", fmt.Errorf("%s: config: %w", r, err)
	}
	if config.OS != "linux" || config.Architecture != "amd64" {
		return "", fmt.Errorf("%s is %s/%s", r, config.OS, config.Architecture)
	}
	if len(config.RootFS.DiffIDs) != len(m.Layers) {
		return "", fmt.Errorf("%s: %d layers, %d diff ids", r, len(m.Layers), len(config.RootFS.DiffIDs))
	}
	rootfs := filepath.Join(tmp, "rootfs")
	if err := os.Mkdir(rootfs, 0o755); err != nil {
		return "", err
	}
	root, err := os.OpenRoot(rootfs)
	if err != nil {
		return "", err
	}
	defer root.Close()
	layerFile := filepath.Join(tmp, "layer")
	for i, l := range m.Layers {
		fmt.Fprintf(os.Stderr, "cell: %s: layer %d of %d, %d MB\n", r, i+1, len(m.Layers), l.Size>>20)
		if err := c.blob(l, layerFile); err != nil {
			return "", err
		}
		if err := unpackFile(root, layerFile, l.MediaType, config.RootFS.DiffIDs[i]); err != nil {
			return "", fmt.Errorf("%s: %s: %w", r, l.Digest, err)
		}
	}
	if err := os.Remove(layerFile); err != nil {
		return "", err
	}
	if err := os.Rename(tmp, s.image(digest)); err != nil {
		return "", err
	}
	return digest, s.setRef(r.String(), digest)
}

// unpackFile unpacks a saved layer, and checks the digest of what it held.
func unpackFile(root *os.Root, file, mediaType, diffID string) error {
	f, err := os.Open(file)
	if err != nil {
		return err
	}
	defer f.Close()
	var layer io.Reader
	var wait func() error
	switch {
	case strings.HasSuffix(mediaType, "+gzip"), strings.HasSuffix(mediaType, ".tar.gzip"):
		z, err := gzip.NewReader(f)
		if err != nil {
			return err
		}
		defer z.Close()
		layer = z
	case strings.HasSuffix(mediaType, "+zstd"), strings.HasSuffix(mediaType, ".tar.zstd"):
		// The one thing the standard library cannot read.
		cmd := exec.Command("zstd", "-dcq")
		cmd.Stdin, cmd.Stderr = f, os.Stderr
		out, err := cmd.StdoutPipe()
		if err != nil {
			return err
		}
		if err := cmd.Start(); err != nil {
			return fmt.Errorf("a zstd layer: %w", err)
		}
		layer, wait = out, cmd.Wait
	case strings.HasSuffix(mediaType, ".tar"), mediaType == "application/vnd.docker.image.rootfs.diff.tar":
		layer = f
	default:
		return fmt.Errorf("a layer of %q", mediaType)
	}
	h := sha256.New()
	tee := io.TeeReader(layer, h)
	err = unpackLayer(root, tee)
	if err == nil {
		// What follows the tar's end is still in the digest.
		_, err = io.Copy(io.Discard, tee)
	}
	if wait != nil {
		if werr := wait(); err == nil {
			err = werr
		}
	}
	if err != nil {
		return err
	}
	if got := "sha256:" + hex.EncodeToString(h.Sum(nil)); got != diffID {
		return fmt.Errorf("its contents are %s, and its config says %s", got, diffID)
	}
	return nil
}

// credentials reads auth.json, in the format the other tools use.
func (s *store) credentials(registry string) (string, string, error) {
	auths, err := s.readAuth()
	if err != nil {
		return "", "", err
	}
	entry, ok := auths[registry]
	if !ok {
		return "", "", nil
	}
	decoded, err := base64.StdEncoding.DecodeString(entry.Auth)
	if err != nil {
		return "", "", fmt.Errorf("auth.json: %s: %w", registry, err)
	}
	user, pass, _ := strings.Cut(string(decoded), ":")
	return user, pass, nil
}

type authEntry struct {
	Auth string `json:"auth"`
}

func (s *store) readAuth() (map[string]authEntry, error) {
	var file struct {
		Auths map[string]authEntry `json:"auths"`
	}
	data, err := os.ReadFile(filepath.Join(s.root, "auth.json"))
	if errors.Is(err, os.ErrNotExist) {
		return map[string]authEntry{}, nil
	}
	if err != nil {
		return nil, err
	}
	if err := json.Unmarshal(data, &file); err != nil {
		return nil, fmt.Errorf("auth.json: %w", err)
	}
	if file.Auths == nil {
		file.Auths = map[string]authEntry{}
	}
	return file.Auths, nil
}

// login keeps a password, which comes on stdin so no process listing shows it.
func (s *store) login(registry, user string) error {
	if !refRegistry.MatchString(registry) {
		return fmt.Errorf("%s: not a registry", registry)
	}
	input, err := io.ReadAll(io.LimitReader(os.Stdin, 1<<16))
	if err != nil {
		return err
	}
	password := strings.TrimRight(string(input), "\r\n")
	if password == "" {
		return errors.New("no password on stdin")
	}
	auths, err := s.readAuth()
	if err != nil {
		return err
	}
	auths[registry] = authEntry{base64.StdEncoding.EncodeToString([]byte(user + ":" + password))}
	data, err := json.MarshalIndent(map[string]any{"auths": auths}, "", "  ")
	if err != nil {
		return err
	}
	tmp := filepath.Join(s.root, "auth.json.tmp")
	if err := os.WriteFile(tmp, append(data, '\n'), 0o600); err != nil {
		return err
	}
	return os.Rename(tmp, filepath.Join(s.root, "auth.json"))
}
