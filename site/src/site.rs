//! Every file the site is: the pages from `pages/*.html`, the manual, and
//! the files in `static/`.

use crate::manual::{self, REPOSITORY};

pub struct Site {
    resources: Vec<(&'static str, Vec<u8>)>,
    missing: Vec<u8>,
}

const SHELL: &str = include_str!("../pages/page.html");
const HOME: &str = include_str!("../pages/home.html");
const MANUAL: &str = include_str!("../pages/manual.html");
const MISSING: &str = include_str!("../pages/missing.html");
const SEAL: &str = include_str!("../../brand/oath.svg");
const SESSION: &str = include_str!("../samples/session.txt");
const INDEX: &str = include_str!("../samples/index.md");

const DESCRIPTION: &str = "Oath is a Linux distribution with its own PID 1 and userspace. \
     The machine is a typed catalog that people and AI agents change with one command, and an apply can be undone.";

/// Files copied as they are. `_headers` is Cloudflare's, and sets the
/// security and caching headers.
const STATIC: &[(&str, &[u8])] = &[
    ("/_headers", include_bytes!("../static/_headers")),
    ("/style.css", include_bytes!("../static/style.css")),
    ("/seal.svg", SEAL.as_bytes()),
    ("/favicon.svg", include_bytes!("../static/favicon.svg")),
    ("/favicon.ico", include_bytes!("../static/favicon.ico")),
    ("/apple-touch-icon.png", include_bytes!("../static/apple-touch-icon.png")),
    ("/fonts/PublicSans.woff2", include_bytes!("../static/fonts/PublicSans.woff2")),
    ("/fonts/PublicSans-Italic.woff2", include_bytes!("../static/fonts/PublicSans-Italic.woff2")),
    ("/fonts/BarlowCondensed-600.woff2", include_bytes!("../static/fonts/BarlowCondensed-600.woff2")),
    ("/fonts/JetBrainsMono.woff2", include_bytes!("../static/fonts/JetBrainsMono.woff2")),
    ("/fonts/OFL-publicsans.txt", include_bytes!("../static/fonts/OFL-publicsans.txt")),
    ("/fonts/OFL-barlowcondensed.txt", include_bytes!("../static/fonts/OFL-barlowcondensed.txt")),
    ("/fonts/OFL-jetbrainsmono.txt", include_bytes!("../static/fonts/OFL-jetbrainsmono.txt")),
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
        for (path, body) in STATIC {
            resources.push((*path, body.to_vec()));
        }
        Site { resources, missing: page("Not found · Oath", "", MISSING) }
    }

    /// Each file by its place under the output directory. Cloudflare serves
    /// `manual.html` at `/manual`, and `404.html` for a path it does not have.
    pub fn files(&self) -> Vec<(&str, &[u8])> {
        let mut files: Vec<(&str, &[u8])> = self
            .resources
            .iter()
            .map(|(path, body)| {
                let file = match *path {
                    "/" => "index.html",
                    "/manual" => "manual.html",
                    other => other.trim_start_matches('/'),
                };
                (file, body.as_slice())
            })
            .collect();
        files.push(("404.html", self.missing.as_slice()));
        files
    }
}

fn page(title: &str, current: &str, body: &str) -> Vec<u8> {
    let manual_current = if current == "manual" { " aria-current=\"page\"" } else { "" };
    fill(
        SHELL,
        &[
            ("title", title),
            ("description", DESCRIPTION),
            ("manual_current", manual_current),
            ("body", body),
        ],
    )
    .into_bytes()
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
    lines(text, |line| match line.strip_prefix("$ ") {
        Some(cmd) => format!("<span class=\"prompt\">$ </span><span class=\"cmd\">{}</span>", escape(cmd)),
        None => escape(line),
    })
}

/// The INDEX as an agent sees it, its Markdown headings in bold.
fn index(text: &str) -> String {
    lines(text, |line| {
        if line.starts_with('#') {
            format!("<span class=\"cmd\">{}</span>", escape(line))
        } else {
            escape(line)
        }
    })
}

/// Each line of terminal output in its own block, so a narrow screen can
/// wrap a long one under a hanging indent and still show where it began.
/// An indented line hangs deeper (`h4`, `h6`, `h8`: two past its indent,
/// at most eight), so its continuation sits inside it, not left of it.
fn lines(text: &str, mark: impl Fn(&str) -> String) -> String {
    text.trim_end()
        .lines()
        .map(|line| {
            let class = match line.len() - line.trim_start().len() {
                0 => "line",
                1..=2 => "line h4",
                3..=4 => "line h6",
                _ => "line h8",
            };
            let inner = if line.is_empty() { " ".to_string() } else { mark(line) };
            format!("<span class=\"{class}\">{inner}</span>")
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_placeholder_is_filled() {
        let site = Site::build();
        for (path, body) in site.files() {
            if path.ends_with(".html") {
                let text = String::from_utf8_lossy(body);
                assert!(!text.contains("{{"), "{path} has an unfilled placeholder");
            }
        }
    }

    #[test]
    fn files_are_what_cloudflare_serves() {
        let site = Site::build();
        let names: Vec<&str> = site.files().into_iter().map(|(name, _)| name).collect();
        for wanted in ["index.html", "manual.html", "404.html", "_headers", "style.css"] {
            assert!(names.contains(&wanted), "no {wanted}");
        }
        assert!(names.iter().all(|name| !name.starts_with('/')));
    }

    #[test]
    fn a_session_marks_commands_and_escapes_output() {
        let s = session("$ oath get <id>\n\n  a < b\n");
        assert_eq!(
            s,
            "<span class=\"line\"><span class=\"prompt\">$ </span><span class=\"cmd\">oath get &lt;id&gt;</span></span>\
             <span class=\"line\"> </span><span class=\"line h4\">  a &lt; b</span>"
        );
    }
}
