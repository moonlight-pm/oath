//! Every response the site gives, built once at start: the pages from
//! `pages/*.html`, the manual, and the files in `static/`.

use crate::manual::{self, REPOSITORY};

pub struct Resource {
    pub body: Vec<u8>,
    pub content_type: &'static str,
    pub etag: String,
    pub cache: &'static str,
}

pub struct Site {
    resources: Vec<(&'static str, Resource)>,
    pub missing: Resource,
}

const SHELL: &str = include_str!("../pages/page.html");
const HOME: &str = include_str!("../pages/home.html");
const MANUAL: &str = include_str!("../pages/manual.html");
const MISSING: &str = include_str!("../pages/missing.html");
const SEAL: &str = include_str!("../../brand/oath.svg");
const SESSION: &str = include_str!("../samples/session.txt");
const INDEX: &str = include_str!("../samples/index.md");

const DESCRIPTION: &str = "Oath is a Linux distribution with its own PID 1 and userspace. \
     The machine is a typed catalog that people and AI agents change with one command, and every change can be undone.";

const HTML: &str = "text/html; charset=utf-8";
/// A page or the stylesheet is checked on every load, so a deploy shows
/// at once; a font or an image is reused for a day.
const CHECK: &str = "no-cache";
const DAY: &str = "public, max-age=86400";

/// Files served as they are.
const STATIC: &[(&str, &[u8], &str, &str)] = &[
    ("/style.css", include_bytes!("../static/style.css"), "text/css; charset=utf-8", CHECK),
    ("/seal.svg", SEAL.as_bytes(), "image/svg+xml", DAY),
    ("/favicon.svg", include_bytes!("../static/favicon.svg"), "image/svg+xml", DAY),
    ("/fonts/PublicSans.woff2", include_bytes!("../static/fonts/PublicSans.woff2"), "font/woff2", DAY),
    ("/fonts/PublicSans-Italic.woff2", include_bytes!("../static/fonts/PublicSans-Italic.woff2"), "font/woff2", DAY),
    ("/fonts/BarlowCondensed-600.woff2", include_bytes!("../static/fonts/BarlowCondensed-600.woff2"), "font/woff2", DAY),
    ("/fonts/JetBrainsMono.woff2", include_bytes!("../static/fonts/JetBrainsMono.woff2"), "font/woff2", DAY),
    ("/fonts/OFL-publicsans.txt", include_bytes!("../static/fonts/OFL-publicsans.txt"), "text/plain; charset=utf-8", DAY),
    ("/fonts/OFL-barlowcondensed.txt", include_bytes!("../static/fonts/OFL-barlowcondensed.txt"), "text/plain; charset=utf-8", DAY),
    ("/fonts/OFL-jetbrainsmono.txt", include_bytes!("../static/fonts/OFL-jetbrainsmono.txt"), "text/plain; charset=utf-8", DAY),
];

impl Site {
    pub fn build() -> Site {
        let home = fill(HOME, &[("session", &session(SESSION)), ("index", &index(INDEX))]);
        let m = manual::render();
        let manual = fill(MANUAL, &[("contents", &m.contents), ("body", &m.body)]);

        let mut resources = vec![
            ("/", page("Oath", "", &home)),
            ("/manual", page("Manual · Oath", "manual", &manual)),
        ];
        for (path, body, content_type, cache) in STATIC {
            resources.push((path, resource(body.to_vec(), content_type, cache)));
        }
        Site { resources, missing: page("Not found · Oath", "", MISSING) }
    }

    pub fn get(&self, path: &str) -> Option<&Resource> {
        self.resources.iter().find(|(p, _)| *p == path).map(|(_, r)| r)
    }
}

fn page(title: &str, current: &str, body: &str) -> Resource {
    let manual_current = if current == "manual" { " aria-current=\"page\"" } else { "" };
    let html = fill(
        SHELL,
        &[
            ("title", title),
            ("description", DESCRIPTION),
            ("manual_current", manual_current),
            ("body", body),
        ],
    );
    resource(html.into_bytes(), HTML, CHECK)
}

fn resource(body: Vec<u8>, content_type: &'static str, cache: &'static str) -> Resource {
    let etag = format!("\"{:016x}\"", fnv1a(&body));
    Resource { body, content_type, etag, cache }
}

/// Put each `{{name}}` in a template. `seal` and `repository` are known
/// to every template. A name left over is a bug in the template.
fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let mut out = template.to_string();
    for (name, value) in values.iter().chain([("seal", seal()), ("repository", REPOSITORY)].iter()) {
        out = out.replace(&format!("{{{{{name}}}}}"), value);
    }
    out
}

/// The seal, inline so it takes the text colour in light and dark.
fn seal() -> &'static str {
    use std::sync::OnceLock;
    static INLINE: OnceLock<String> = OnceLock::new();
    INLINE.get_or_init(|| {
        let start = SEAL.find('>').expect("brand/oath.svg opens with <svg …>") + 1;
        let end = SEAL.rfind("</svg>").expect("brand/oath.svg closes");
        format!(
            "<svg class=\"seal\" viewBox=\"0 0 640 640\" aria-hidden=\"true\" focusable=\"false\">{}</svg>",
            SEAL[start..end].trim()
        )
    })
}

/// A recorded terminal session: each `$ ` line is the command typed.
fn session(text: &str) -> String {
    text.trim_end()
        .lines()
        .map(|line| match line.strip_prefix("$ ") {
            Some(cmd) => format!("<span class=\"prompt\">$ </span><span class=\"cmd\">{}</span>", escape(cmd)),
            None => escape(line),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The INDEX as an agent sees it, its Markdown headings in bold.
fn index(text: &str) -> String {
    text.trim_end()
        .lines()
        .map(|line| {
            if line.starts_with('#') {
                format!("<span class=\"cmd\">{}</span>", escape(line))
            } else {
                escape(line)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |h, b| (h ^ *b as u64).wrapping_mul(0x100000001b3))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_placeholder_is_filled() {
        let site = Site::build();
        let pages = site.resources.iter().map(|(p, r)| (*p, r)).chain([("404", &site.missing)]);
        for (path, r) in pages {
            if r.content_type == HTML {
                let text = String::from_utf8_lossy(&r.body);
                assert!(!text.contains("{{"), "{path} has an unfilled placeholder");
            }
        }
    }

    #[test]
    fn a_session_marks_commands_and_escapes_output() {
        let s = session("$ oath get <id>\na < b\n");
        assert_eq!(s, "<span class=\"prompt\">$ </span><span class=\"cmd\">oath get &lt;id&gt;</span>\na &lt; b");
    }
}
