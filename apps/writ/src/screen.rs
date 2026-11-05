//! The one place the interactive session writes to a terminal. Finished lines go up into the
//! terminal's own scrollback and are never touched again, so history, search and copying stay the
//! terminal's. Only the live rows at the bottom are redrawn, and only those that changed.
//!
//! Every byte of text passes `term::scrub` here, and the only escape sequences that leave are the
//! ones this file spells out. A model's words can look like anything, but they cannot have a colour,
//! so writ's own lines, which all do, cannot be forged. And this never asks the terminal anything,
//! so there is no reply for anything to imitate.

use crate::term;
use std::fmt::Write;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Style {
    /// A model's words, and nothing else of writ's: no colour to imitate.
    Plain,
    Dim,
    You,
    Code,
    Good,
    Bad,
    Note,
    Added,
    Removed,
    Hunk,
}

fn sgr(s: Style) -> &'static str {
    match s {
        Style::Plain => "",
        Style::Dim => "\x1b[2m",
        Style::You => "\x1b[1m",
        Style::Code | Style::Hunk => "\x1b[36m",
        Style::Good | Style::Added => "\x1b[32m",
        Style::Bad | Style::Removed => "\x1b[31m",
        Style::Note => "\x1b[33m",
    }
}

const RESET: &str = "\x1b[0m";
const HIDE_CURSOR: &str = "\x1b[?25l";
const SHOW_CURSOR: &str = "\x1b[?25h";
const WRAP_OFF: &str = "\x1b[?7l";
const WRAP_ON: &str = "\x1b[?7h";
const SYNC_ON: &str = "\x1b[?2026h";
const SYNC_OFF: &str = "\x1b[?2026l";
const PASTE_ON: &str = "\x1b[?2004h";
const PASTE_OFF: &str = "\x1b[?2004l";
const CLEAR_BELOW: &str = "\x1b[J";
const CLEAR_RIGHT: &str = "\x1b[K";
const CLEAR_ALL: &str = "\x1b[2J\x1b[H";

/// What to send on the way in.
pub const ENTER: &str = PASTE_ON;
/// What to send on the way out, whatever state it was left in. Safe from a signal handler.
pub const LEAVE: &str = "\x1b[?2026l\x1b[?2004l\x1b[?7h\x1b[?25h\x1b[0m";

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub style: Style,
    pub text: String,
}

impl Row {
    pub fn new(style: Style, text: impl Into<String>) -> Row {
        Row { style, text: text.into() }
    }
}

/// Columns a character takes. Close to wcwidth, and a wrong guess costs a clipped row, never a
/// misplaced one: live rows are drawn with the terminal's wrapping off.
pub fn width(c: char) -> usize {
    let u = c as u32;
    if matches!(u, 0x300..=0x36f | 0x483..=0x489 | 0x591..=0x5bd | 0x610..=0x61a | 0x64b..=0x65f | 0x1ab0..=0x1aff | 0x1dc0..=0x1dff | 0x20d0..=0x20ff | 0xfe00..=0xfe0f | 0xfe20..=0xfe2f) {
        return 0;
    }
    if matches!(u, 0x1100..=0x115f | 0x2e80..=0x303e | 0x3041..=0x33ff | 0x3400..=0x4dbf | 0x4e00..=0x9fff | 0xa000..=0xa4cf | 0xa960..=0xa97f | 0xac00..=0xd7a3 | 0xf900..=0xfaff | 0xfe30..=0xfe6f | 0xff00..=0xff60 | 0xffe0..=0xffe6 | 0x1f300..=0x1f64f | 0x1f680..=0x1f6ff | 0x1f900..=0x1f9ff | 0x20000..=0x3fffd) {
        return 2;
    }
    1
}

pub fn text_width(s: &str) -> usize {
    s.chars().map(width).sum()
}

/// As much of `s` as fits in `cols`.
pub fn clip(s: &str, cols: usize) -> String {
    let mut used = 0;
    let mut out = String::new();
    for c in s.chars() {
        used += width(c);
        if used > cols {
            break;
        }
        out.push(c);
    }
    out
}

/// Break text at spaces to fit `cols`, keeping the first line's indent on the rest. Greedily, so a
/// row once full does not change when more text arrives: that is what lets it go up as it fills.
pub fn wrap(text: &str, cols: usize) -> Vec<String> {
    let cols = cols.max(8);
    let indent: String = text.chars().take_while(|c| *c == ' ').collect();
    let indent = if indent.len() * 2 > cols { String::new() } else { indent };
    let mut rows = Vec::new();
    let mut row = String::new();
    let mut used = 0;
    let mut last_space: Option<(usize, usize)> = None;
    for c in text.chars() {
        let w = width(c);
        if used + w > cols && used > 0 {
            match last_space.take().filter(|(at, _)| row[..*at].trim_start().chars().next().is_some()) {
                Some((at, _)) => {
                    let rest = row[at + 1..].to_string();
                    row.truncate(at);
                    rows.push(std::mem::take(&mut row));
                    row = format!("{indent}{rest}");
                }
                None => {
                    rows.push(std::mem::take(&mut row));
                    row = indent.clone();
                }
            }
            used = text_width(&row);
            last_space = None;
        }
        if c == ' ' {
            last_space = Some((row.len(), used));
        }
        row.push(c);
        used += w;
    }
    rows.push(row);
    rows
}

/// What is on the terminal, as far as this knows: the live rows drawn last, and the row the cursor
/// was left on.
pub struct Screen {
    cols: usize,
    rows: usize,
    drawn: Vec<String>,
    cursor_row: usize,
    clear_first: bool,
}

/// A live row, safe to print and as wide as it may be: it has no tabs, since their width is the terminal's.
fn live_row(r: &Row, cols: usize) -> String {
    let text = term::scrub(&r.text).replace(['\n', '\t'], " ");
    let text = clip(&text, cols);
    if r.style == Style::Plain {
        text
    } else {
        format!("{}{text}{RESET}", sgr(r.style))
    }
}

impl Screen {
    pub fn new(cols: usize, rows: usize) -> Screen {
        Screen { cols: cols.max(1), rows: rows.max(1), drawn: Vec::new(), cursor_row: 0, clear_first: false }
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    /// The terminal changed size. It may have moved what was drawn, so draw it all again.
    pub fn resize(&mut self, cols: usize, rows: usize) {
        self.cols = cols.max(1);
        self.rows = rows.max(1);
        self.drawn.clear();
        self.clear_first = true;
    }

    /// One frame: `up` goes into the scrollback for good, and `live` is what the bottom now shows,
    /// with the cursor at `cursor`, a row of `live` and a column. Live rows past the screen's height
    /// lose their first ones, since the cursor cannot reach what has scrolled away.
    pub fn frame(&mut self, up: &[Row], live: &[Row], cursor: (usize, usize)) -> Vec<u8> {
        let room = self.rows.saturating_sub(1).max(1);
        let cut = live.len().saturating_sub(room);
        let live = &live[cut..];
        let cursor = (cursor.0.saturating_sub(cut).min(live.len().saturating_sub(1)), cursor.1.min(self.cols - 1));

        let mut o = String::new();
        o.push_str(SYNC_ON);
        o.push_str(HIDE_CURSOR);
        if self.cursor_row > 0 {
            let _ = write!(o, "\x1b[{}A", self.cursor_row);
        }
        o.push('\r');
        if self.clear_first {
            o.push_str(CLEAR_BELOW);
            self.clear_first = false;
        }
        if !up.is_empty() {
            o.push_str(CLEAR_BELOW);
            self.drawn.clear();
            // The terminal wraps these itself, so it can reflow them and copy them whole.
            for r in up {
                for line in term::scrub(&r.text).split('\n') {
                    if r.style == Style::Plain || line.is_empty() {
                        o.push_str(line);
                    } else {
                        o.push_str(sgr(r.style));
                        o.push_str(line);
                        o.push_str(RESET);
                    }
                    o.push_str("\r\n");
                }
            }
        }
        o.push_str(WRAP_OFF);
        let now: Vec<String> = live.iter().map(|r| live_row(r, self.cols)).collect();
        for (i, row) in now.iter().enumerate() {
            if i > 0 {
                o.push_str("\r\n");
            }
            if self.drawn.get(i) != Some(row) {
                o.push_str(row);
                o.push_str(CLEAR_RIGHT);
            }
        }
        if now.len() < self.drawn.len() {
            o.push_str("\x1b[1B\r");
            o.push_str(CLEAR_BELOW);
            o.push_str("\x1b[1A");
        }
        let up_to_cursor = now.len().saturating_sub(1) - cursor.0;
        if up_to_cursor > 0 {
            let _ = write!(o, "\x1b[{up_to_cursor}A");
        }
        o.push('\r');
        if cursor.1 > 0 {
            let _ = write!(o, "\x1b[{}C", cursor.1);
        }
        o.push_str(WRAP_ON);
        o.push_str(SHOW_CURSOR);
        o.push_str(SYNC_OFF);
        self.drawn = now;
        self.cursor_row = cursor.0;
        o.into_bytes()
    }

    /// Ctrl-L: an empty screen, with the scrollback left as it was.
    pub fn clear(&mut self) -> Vec<u8> {
        self.drawn.clear();
        self.cursor_row = 0;
        CLEAR_ALL.as_bytes().to_vec()
    }

    /// Take the live rows away, leaving the cursor where the shell's next line belongs.
    pub fn leave(&mut self) -> Vec<u8> {
        let mut o = String::new();
        if self.cursor_row > 0 {
            let _ = write!(o, "\x1b[{}A", self.cursor_row);
        }
        o.push('\r');
        o.push_str(CLEAR_BELOW);
        o.push_str(PASTE_OFF);
        self.drawn.clear();
        self.cursor_row = 0;
        o.into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A terminal small enough to read: the sequences above and nothing else, and it fails on any
    /// it does not know, so a stray one cannot pass unnoticed.
    struct Vt {
        cols: usize,
        rows: usize,
        lines: Vec<Vec<char>>,
        top: usize,
        at: (usize, usize),
        wrap: bool,
    }

    impl Vt {
        fn new(cols: usize, rows: usize) -> Vt {
            Vt { cols, rows, lines: vec![Vec::new()], top: 0, at: (0, 0), wrap: true }
        }
        fn line_mut(&mut self, row: usize) -> &mut Vec<char> {
            while self.lines.len() <= self.top + row {
                self.lines.push(Vec::new());
            }
            let t = self.top;
            &mut self.lines[t + row]
        }
        fn down(&mut self) {
            if self.at.0 + 1 < self.rows {
                self.at.0 += 1;
            } else {
                self.top += 1;
            }
            self.line_mut(self.at.0);
        }
        fn feed(&mut self, bytes: &[u8]) {
            let s = String::from_utf8(bytes.to_vec()).unwrap();
            let mut it = s.chars().peekable();
            while let Some(c) = it.next() {
                match c {
                    '\r' => self.at.1 = 0,
                    '\n' => self.down(),
                    '\x1b' => {
                        assert_eq!(it.next(), Some('['), "only CSI sequences leave");
                        let mut body = String::new();
                        let end = loop {
                            let c = it.next().expect("a whole sequence");
                            if ('\x40'..='\x7e').contains(&c) {
                                break c;
                            }
                            body.push(c);
                        };
                        let n: usize = body.parse().unwrap_or(1);
                        match (end, body.as_str()) {
                            ('A', _) => self.at.0 = self.at.0.saturating_sub(n),
                            ('B', _) => self.at.0 = (self.at.0 + n).min(self.rows - 1),
                            ('C', _) => self.at.1 = (self.at.1 + n).min(self.cols - 1),
                            ('K', "") => {
                                let (r, col) = self.at;
                                self.line_mut(r).truncate(col);
                            }
                            ('J', "") => {
                                let (r, col) = self.at;
                                self.line_mut(r).truncate(col);
                                self.lines.truncate(self.top + r + 1);
                            }
                            ('h', "?7") => self.wrap = true,
                            ('l', "?7") => self.wrap = false,
                            ('h' | 'l', "?25" | "?2026" | "?2004") => {}
                            ('m', "0" | "1" | "2" | "31" | "32" | "33" | "36") => {}
                            other => panic!("a sequence this file does not send: {other:?}"),
                        }
                    }
                    c => {
                        assert!(!c.is_control(), "a control character reached the terminal: {c:?}");
                        if self.at.1 >= self.cols {
                            if self.wrap {
                                self.down();
                                self.at.1 = 0;
                            } else {
                                self.at.1 = self.cols - 1;
                            }
                        }
                        let (r, col) = self.at;
                        let l = self.line_mut(r);
                        while l.len() <= col {
                            l.push(' ');
                        }
                        l[col] = c;
                        self.at.1 += 1;
                    }
                }
            }
        }
        fn screen(&self) -> Vec<String> {
            self.text(self.top)
        }
        fn all(&self) -> Vec<String> {
            self.text(0)
        }
        /// An empty line under the last one is not something a person can see.
        fn text(&self, from: usize) -> Vec<String> {
            let mut t: Vec<String> = self.lines[from..].iter().map(|l| l.iter().collect()).collect();
            while t.last().is_some_and(|l| l.is_empty()) {
                t.pop();
            }
            t
        }
    }

    fn plain(rows: &[&str]) -> Vec<Row> {
        rows.iter().map(|t| Row::new(Style::Plain, *t)).collect()
    }

    #[test]
    fn a_row_is_only_written_when_it_changed() {
        let mut s = Screen::new(40, 10);
        let mut t = Vt::new(40, 10);
        t.feed(&s.frame(&[], &plain(&["status", "> hello"]), (1, 7)));
        assert_eq!(t.screen(), ["status", "> hello"]);
        assert_eq!(t.at, (1, 7));
        let second = s.frame(&[], &plain(&["status", "> hello!"]), (1, 8));
        assert!(!String::from_utf8_lossy(&second).contains("status"), "unchanged rows stay as they are");
        t.feed(&second);
        assert_eq!(t.screen(), ["status", "> hello!"]);
        assert_eq!(t.at, (1, 8));
    }

    #[test]
    fn what_goes_up_stays_and_the_live_rows_follow_it() {
        let mut s = Screen::new(40, 10);
        let mut t = Vt::new(40, 10);
        t.feed(&s.frame(&[], &plain(&["status", "> "]), (1, 2)));
        t.feed(&s.frame(&plain(&["first answer"]), &plain(&["status", "> "]), (1, 2)));
        t.feed(&s.frame(&plain(&["second", "third"]), &plain(&["busy", "> x"]), (1, 3)));
        assert_eq!(t.screen(), ["first answer", "second", "third", "busy", "> x"]);
        assert_eq!(t.at, (4, 3));
    }

    #[test]
    fn fewer_rows_leave_nothing_behind() {
        let mut s = Screen::new(40, 10);
        let mut t = Vt::new(40, 10);
        t.feed(&s.frame(&[], &plain(&["one", "two", "three", "four"]), (3, 0)));
        t.feed(&s.frame(&[], &plain(&["one", "two"]), (1, 1)));
        assert_eq!(t.screen(), ["one", "two"]);
        assert_eq!(t.at, (1, 1));
        // And the last row, when it did not change, is not what gets cleared.
        t.feed(&s.frame(&[], &plain(&["one"]), (0, 0)));
        assert_eq!(t.screen(), ["one"]);
    }

    #[test]
    fn it_holds_together_at_the_bottom_of_the_screen() {
        let mut s = Screen::new(30, 4);
        let mut t = Vt::new(30, 4);
        t.feed(&s.frame(&[], &plain(&["status", "> "]), (1, 2)));
        for i in 0..20 {
            let line = format!("line {i}");
            t.feed(&s.frame(&plain(&[line.as_str()]), &plain(&["status", "> "]), (1, 2)));
        }
        let all = t.all();
        for i in 0..20 {
            assert_eq!(all[i], format!("line {i}"), "{all:?}");
        }
        assert_eq!(all[20..], ["status", "> "]);
        // Growing past the screen's height keeps the rows the cursor is on.
        t.feed(&s.frame(&[], &plain(&["a", "b", "c", "d", "e", "f"]), (5, 0)));
        assert_eq!(t.screen().last().unwrap(), "f");
        t.feed(&s.frame(&[], &plain(&["status", "> again"]), (1, 7)));
        assert_eq!(t.screen().last().unwrap(), "> again");
    }

    #[test]
    fn a_live_row_never_wraps_and_a_finished_line_does() {
        let mut s = Screen::new(10, 6);
        let mut t = Vt::new(10, 6);
        t.feed(&s.frame(&[], &plain(&["0123456789ABCDEF", "> "]), (1, 2)));
        assert_eq!(t.screen(), ["0123456789", "> "]);
        t.feed(&s.frame(&plain(&["0123456789ABCDEF"]), &plain(&["s", "> "]), (1, 2)));
        assert_eq!(t.screen(), ["0123456789", "ABCDEF", "s", "> "]);
    }

    #[test]
    fn a_resize_draws_it_all_again() {
        let mut s = Screen::new(40, 10);
        let mut t = Vt::new(40, 10);
        t.feed(&s.frame(&[], &plain(&["status", "> abc"]), (1, 5)));
        s.resize(20, 10);
        t.cols = 20;
        t.feed(&s.frame(&[], &plain(&["status", "> abc"]), (1, 5)));
        assert_eq!(t.screen(), ["status", "> abc"]);
    }

    #[test]
    fn what_a_model_writes_cannot_steer_the_terminal() {
        let hostile = "\x1b[2J\x1b[H\x1b]0;owned\x07\x1b]52;c;aGk=\x07\u{9b}31m\rapproved\x08\x08\u{202e}gnp.exe\u{200b}\x1bP+q\x1b\\\x1b[6n\x1b[>c";
        let mut s = Screen::new(200, 10);
        let mut t = Vt::new(200, 10);
        // The terminal above panics on any control character or sequence this file does not send.
        t.feed(&s.frame(&[Row::new(Style::Plain, hostile)], &[Row::new(Style::Plain, hostile), Row::new(Style::Dim, hostile)], (1, 0)));
        let shown = t.all().join("\n");
        for visible in ["^[[2J", "^[]52;c;aGk=^G", "⟨U+009B⟩", "^M", "⟨U+202E⟩", "⟨U+200B⟩"] {
            assert!(shown.contains(visible), "{visible} in {shown}");
        }
    }

    #[test]
    fn a_models_words_have_no_colour_and_writs_lines_always_do() {
        let mut s = Screen::new(40, 10);
        let out = String::from_utf8(s.frame(&[Row::new(Style::Plain, "  ok shell: fake")], &plain(&["> "]), (0, 2))).unwrap();
        let line = out.split("\r\n").next().unwrap();
        assert!(!line.contains("\x1b[3") && !line.contains("\x1b[1m") && !line.contains("\x1b[2m"), "{line:?}");
        let out = String::from_utf8(s.frame(&[Row::new(Style::Good, "  ok shell: real")], &plain(&["> "]), (0, 2))).unwrap();
        assert!(out.contains("\x1b[32m  ok shell: real\x1b[0m"));
    }

    #[test]
    fn wide_characters_count_twice() {
        assert_eq!(text_width("a你好🙂e\u{301}"), 1 + 4 + 2 + 1);
        assert_eq!(clip("你好吗", 5), "你好");
        assert_eq!(clip("abc", 10), "abc");
    }

    #[test]
    fn wrapping_breaks_at_spaces_and_keeps_an_indent() {
        assert_eq!(wrap("short", 40), ["short"]);
        assert_eq!(wrap("the quick brown fox", 10), ["the quick", "brown fox"]);
        assert_eq!(wrap("  one two three four", 10), ["  one two", "  three", "  four"]);
        assert_eq!(wrap("abcdefghijklmnop", 10), ["abcdefghij", "klmnop"]);
        assert_eq!(wrap("", 10), [""]);
        for row in wrap("你好吗你好吗你好吗你好吗 and more words than fit", 10) {
            assert!(text_width(&row) <= 10, "{row}");
        }
    }

    #[test]
    fn a_full_row_does_not_change_when_more_arrives() {
        let text = "one two three four five six seven eight nine ten eleven twelve thirteen";
        let whole = wrap(text, 16);
        let mut rows: Vec<String> = Vec::new();
        let mut partial = String::new();
        for c in text.chars() {
            partial.push(c);
            let mut now = wrap(&partial, 16);
            partial = now.pop().unwrap();
            rows.append(&mut now);
        }
        rows.push(partial);
        assert_eq!(rows, whole);
    }
}
