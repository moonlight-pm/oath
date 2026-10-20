//! docs/manual as one page: each chapter numbered, every heading an
//! anchor, links between chapters turned into anchors on the page and
//! any other relative link into the file on GitHub. Raw HTML in the
//! Markdown is shown as text.

use pulldown_cmark::{html, CowStr, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

/// Chapter files in page order: the order of docs/manual/README.md's
/// table, which a test holds this list to.
pub const CHAPTERS: &[(&str, &str)] = &[
    ("using", include_str!("../../docs/manual/using.md")),
    ("catalog", include_str!("../../docs/manual/catalog.md")),
    ("services", include_str!("../../docs/manual/services.md")),
    ("packages", include_str!("../../docs/manual/packages.md")),
    ("network", include_str!("../../docs/manual/network.md")),
    ("generations", include_str!("../../docs/manual/generations.md")),
    ("build", include_str!("../../docs/manual/build.md")),
    ("install", include_str!("../../docs/manual/install.md")),
];

pub const REPOSITORY: &str = "https://github.com/moonlight-pm/oath";

pub struct Manual {
    /// Chapters and their sections, as nested lists.
    pub contents: String,
    pub body: String,
}

struct Heading {
    level: HeadingLevel,
    id: String,
    number: String,
    /// The heading's inline HTML (code spans kept).
    title: String,
}

pub fn render() -> Manual {
    let mut contents = String::from("<ol class=\"toc\">");
    let mut body = String::new();
    for (i, (name, text)) in CHAPTERS.iter().enumerate() {
        let events: Vec<Event> = Parser::new_ext(text, Options::ENABLE_TABLES).collect();
        let headings = headings(name, i + 1, &events);

        contents.push_str("<li>");
        let mut open = false;
        for h in &headings {
            let link = format!(
                "<a href=\"#{}\"><span class=\"number\">{}</span><span>{}</span></a>",
                h.id, h.number, h.title
            );
            match h.level {
                HeadingLevel::H1 => contents.push_str(&link),
                HeadingLevel::H2 => {
                    if !open {
                        contents.push_str("<ol>");
                        open = true;
                    }
                    contents.push_str(&format!("<li>{link}</li>"));
                }
                _ => {}
            }
        }
        if open {
            contents.push_str("</ol>");
        }
        contents.push_str("</li>");

        body.push_str("<section class=\"chapter\">");
        html::push_html(&mut body, rewrite(name, events, &headings).into_iter());
        body.push_str("</section>\n");
    }
    contents.push_str("</ol>");
    Manual { contents, body }
}

/// Every heading in a chapter: its id (`<chapter>` for the title,
/// `<chapter>-<slug>` below it) and its number (`3`, `3.2`, `3.2.1`).
fn headings(chapter: &str, n: usize, events: &[Event]) -> Vec<Heading> {
    let mut out: Vec<Heading> = Vec::new();
    let mut counts = [0usize; 2];
    let mut i = 0;
    while i < events.len() {
        if let Event::Start(Tag::Heading { level, .. }) = &events[i] {
            let start = i + 1;
            while !matches!(events[i], Event::End(TagEnd::Heading(_))) {
                i += 1;
            }
            let inner = &events[start..i];
            let number = match level {
                HeadingLevel::H1 => n.to_string(),
                HeadingLevel::H2 => {
                    counts = [counts[0] + 1, 0];
                    format!("{n}.{}", counts[0])
                }
                HeadingLevel::H3 => {
                    counts[1] += 1;
                    format!("{n}.{}.{}", counts[0], counts[1])
                }
                _ => String::new(),
            };
            let mut id = if *level == HeadingLevel::H1 {
                chapter.to_string()
            } else {
                format!("{chapter}-{}", slug(&plain(inner)))
            };
            let base = id.clone();
            let mut k = 2;
            while out.iter().any(|h| h.id == id) {
                id = format!("{base}-{k}");
                k += 1;
            }
            let mut title = String::new();
            html::push_html(&mut title, inner.iter().cloned().map(escape_raw));
            out.push(Heading { level: *level, id, number, title });
        }
        i += 1;
    }
    out
}

/// The chapter's events as they go on the page: headings one level down
/// (the page's h1 is the manual's title) with their number and anchor,
/// tables in a scrolling wrapper, links rewritten, raw HTML as text.
fn rewrite<'a>(chapter: &str, events: Vec<Event<'a>>, headings: &[Heading]) -> Vec<Event<'a>> {
    let mut out = Vec::with_capacity(events.len() + headings.len() * 3);
    let mut next = headings.iter();
    let mut current: Option<&Heading> = None;
    for event in events {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                let h = next.next().expect("headings() saw this heading");
                current = Some(h);
                out.push(Event::Start(Tag::Heading {
                    level: down(level),
                    id: Some(CowStr::from(h.id.clone())),
                    classes: vec![],
                    attrs: vec![],
                }));
                if !h.number.is_empty() {
                    out.push(Event::InlineHtml(format!("<span class=\"number\">{}</span>", h.number).into()));
                }
            }
            Event::End(TagEnd::Heading(level)) => {
                if let Some(h) = current.take() {
                    out.push(Event::InlineHtml(
                        format!("<a class=\"anchor\" href=\"#{}\" aria-label=\"Link to this section\">§</a>", h.id).into(),
                    ));
                }
                out.push(Event::End(TagEnd::Heading(down(level))));
            }
            Event::Start(Tag::Table(align)) => {
                out.push(Event::Html("<div class=\"table-wrap\">".into()));
                out.push(Event::Start(Tag::Table(align)));
            }
            Event::End(TagEnd::Table) => {
                out.push(Event::End(TagEnd::Table));
                out.push(Event::Html("</div>".into()));
            }
            Event::Start(Tag::Link { link_type, dest_url, title, id }) => {
                let dest_url = CowStr::from(link(chapter, &dest_url));
                out.push(Event::Start(Tag::Link { link_type, dest_url, title, id }));
            }
            other => out.push(escape_raw(other)),
        }
    }
    out
}

fn escape_raw(event: Event) -> Event {
    match event {
        Event::Html(s) | Event::InlineHtml(s) => Event::Text(s),
        other => other,
    }
}

fn down(level: HeadingLevel) -> HeadingLevel {
    match level {
        HeadingLevel::H1 => HeadingLevel::H2,
        HeadingLevel::H2 => HeadingLevel::H3,
        HeadingLevel::H3 => HeadingLevel::H4,
        HeadingLevel::H4 => HeadingLevel::H5,
        _ => HeadingLevel::H6,
    }
}

/// Where a link in `docs/manual/<chapter>.md` goes on the page.
fn link(chapter: &str, dest: &str) -> String {
    if dest.contains("://") || dest.starts_with("mailto:") {
        return dest.to_string();
    }
    if let Some(fragment) = dest.strip_prefix('#') {
        return format!("#{chapter}-{fragment}");
    }
    let (file, fragment) = dest.split_once('#').unwrap_or((dest, ""));
    if let Some(name) = file.strip_suffix(".md") {
        if CHAPTERS.iter().any(|(c, _)| *c == name) {
            return if fragment.is_empty() { format!("#{name}") } else { format!("#{name}-{fragment}") };
        }
        if name == "README" {
            return "#top".to_string();
        }
    }
    let mut path: Vec<&str> = vec!["docs", "manual"];
    for part in file.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                path.pop();
            }
            p => path.push(p),
        }
    }
    let kind = if file.ends_with('/') || file.is_empty() { "tree" } else { "blob" };
    let hash = if fragment.is_empty() { String::new() } else { format!("#{fragment}") };
    format!("{REPOSITORY}/{kind}/master/{}{hash}", path.join("/"))
}

/// A heading's text without markup.
fn plain(events: &[Event]) -> String {
    events
        .iter()
        .filter_map(|e| match e {
            Event::Text(t) | Event::Code(t) => Some(t.as_ref()),
            _ => None,
        })
        .collect()
}

/// GitHub's anchor for a heading: lower case, punctuation dropped,
/// spaces to hyphens, so `using.md#agents` lands where it does there.
fn slug(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter_map(|c| match c {
            ' ' => Some('-'),
            c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chapters_follow_the_readme() {
        let readme = include_str!("../../docs/manual/README.md");
        let listed: Vec<&str> = readme
            .lines()
            .filter(|l| l.starts_with("| ["))
            .filter_map(|l| l.split("](").nth(1)?.split(".md)").next())
            .collect();
        let ours: Vec<&str> = CHAPTERS.iter().map(|(n, _)| *n).collect();
        assert_eq!(listed, ours, "docs/manual/README.md lists the chapters in this order");
    }

    #[test]
    fn links_land_on_the_page_or_github() {
        assert_eq!(link("using", "#agents"), "#using-agents");
        assert_eq!(link("using", "network.md#names"), "#network-names");
        assert_eq!(link("using", "catalog.md"), "#catalog");
        assert_eq!(link("using", "../specs/README.md"), format!("{REPOSITORY}/blob/master/docs/specs/README.md"));
        assert_eq!(link("using", "https://curl.se/"), "https://curl.se/");
    }

    #[test]
    fn every_anchor_on_the_page_exists() {
        let m = render();
        let page = format!("{}{}", m.contents, m.body);
        for href in page.split("href=\"#").skip(1) {
            let id = href.split('"').next().unwrap();
            if id == "top" {
                continue;
            }
            assert!(page.contains(&format!("id=\"{id}\"")), "#{id} has no target");
        }
    }

    #[test]
    fn slugs_match_github() {
        assert_eq!(slug("Using oath"), "using-oath");
        assert_eq!(slug("Network and SSH"), "network-and-ssh");
        assert_eq!(slug("Apply and undo"), "apply-and-undo");
    }
}
