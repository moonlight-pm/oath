//! Keys and the line the person is typing. Nothing here runs anything: what comes out is text a
//! person sent. A paste is only ever inserted, so pasted text cannot submit itself, whatever it holds.

use crate::screen::{text_width, width};

const MAX_PASTE: usize = 1 << 20;
const PROMPT: &str = "> ";
const PROMPT_MORE: &str = "  ";
const PASTE_END: &[u8] = b"\x1b[201~";

#[derive(Clone, Debug, PartialEq)]
pub enum Key {
    Char(char),
    Paste(String),
    Enter,
    NewLine,
    Tab,
    Backspace,
    Delete,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    WordLeft,
    WordRight,
    DeleteWord,
    KillToEnd,
    KillToStart,
    Interrupt,
    Eof,
    Esc,
    Clear,
    Thinking,
}

/// Bytes from a terminal into keys. They arrive in any pieces, so what is unfinished waits.
#[derive(Default)]
pub struct Decoder {
    buf: Vec<u8>,
    paste: Option<Vec<u8>>,
}

enum Step {
    /// Took this many bytes, and maybe made a key.
    Took(usize, Option<Key>),
    Wait,
}

impl Decoder {
    pub fn push(&mut self, bytes: &[u8], out: &mut Vec<Key>) {
        self.buf.extend_from_slice(bytes);
        loop {
            if self.paste.is_some() && !self.take_paste(out) {
                return;
            }
            if self.buf.is_empty() {
                return;
            }
            match self.step() {
                Step::Took(n, key) => {
                    self.buf.drain(..n);
                    out.extend(key);
                }
                Step::Wait => return,
            }
        }
    }

    /// A lone escape only means Esc once nothing has followed it for a moment.
    pub fn waiting_on_escape(&self) -> bool {
        self.paste.is_none() && self.buf.first() == Some(&0x1b)
    }

    pub fn flush_escape(&mut self, out: &mut Vec<Key>) {
        if self.waiting_on_escape() {
            self.buf.clear();
            out.push(Key::Esc);
        }
    }

    /// Whether the paste finished. Whatever it holds goes in as text, an end marker in it included.
    fn take_paste(&mut self, out: &mut Vec<Key>) -> bool {
        let Some(so_far) = self.paste.as_mut() else { return true };
        let end = self.buf.windows(PASTE_END.len()).position(|w| w == PASTE_END);
        let upto = end.unwrap_or_else(|| self.buf.len().saturating_sub(PASTE_END.len() - 1));
        let room = MAX_PASTE.saturating_sub(so_far.len());
        so_far.extend_from_slice(&self.buf[..upto.min(room)]);
        self.buf.drain(..upto);
        if end.is_none() {
            return false;
        }
        self.buf.drain(..PASTE_END.len());
        if let Some(p) = self.paste.take() {
            out.push(Key::Paste(clean_paste(&p)));
        }
        true
    }

    fn step(&mut self) -> Step {
        let b = self.buf[0];
        match b {
            0x1b => self.escape(),
            b'\r' => Step::Took(1, Some(Key::Enter)),
            b'\n' => Step::Took(1, Some(Key::NewLine)),
            b'\t' => Step::Took(1, Some(Key::Tab)),
            0x7f | 0x08 => Step::Took(1, Some(Key::Backspace)),
            0x01 => Step::Took(1, Some(Key::Home)),
            0x02 => Step::Took(1, Some(Key::Left)),
            0x03 => Step::Took(1, Some(Key::Interrupt)),
            0x04 => Step::Took(1, Some(Key::Eof)),
            0x05 => Step::Took(1, Some(Key::End)),
            0x06 => Step::Took(1, Some(Key::Right)),
            0x0b => Step::Took(1, Some(Key::KillToEnd)),
            0x0c => Step::Took(1, Some(Key::Clear)),
            0x0e => Step::Took(1, Some(Key::Down)),
            0x10 => Step::Took(1, Some(Key::Up)),
            0x14 => Step::Took(1, Some(Key::Thinking)),
            0x15 => Step::Took(1, Some(Key::KillToStart)),
            0x17 => Step::Took(1, Some(Key::DeleteWord)),
            b if b < 0x20 => Step::Took(1, None),
            b if b < 0x80 => Step::Took(1, Some(Key::Char(b as char))),
            _ => self.utf8(),
        }
    }

    fn utf8(&self) -> Step {
        let need = match self.buf[0] {
            0xc2..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf4 => 4,
            _ => return Step::Took(1, None),
        };
        if self.buf.len() < need {
            return Step::Wait;
        }
        match std::str::from_utf8(&self.buf[..need]).ok().and_then(|s| s.chars().next()) {
            Some(c) => Step::Took(need, Some(Key::Char(c))),
            None => Step::Took(1, None),
        }
    }

    fn escape(&mut self) -> Step {
        match self.buf.get(1) {
            None => Step::Wait,
            Some(b'[') => self.csi(),
            Some(b'O') => match self.buf.get(2) {
                None => Step::Wait,
                Some(b) => Step::Took(3, arrow(*b)),
            },
            Some(b'\r') | Some(b'\n') => Step::Took(2, Some(Key::NewLine)),
            Some(b'b') => Step::Took(2, Some(Key::WordLeft)),
            Some(b'f') => Step::Took(2, Some(Key::WordRight)),
            Some(0x7f) | Some(0x08) => Step::Took(2, Some(Key::DeleteWord)),
            // Escape, then a key: both, since nothing else reads as this.
            Some(0x1b) => Step::Took(1, Some(Key::Esc)),
            Some(_) => Step::Took(2, None),
        }
    }

    fn csi(&mut self) -> Step {
        let Some(len) = self.buf[2..].iter().position(|b| (0x40..=0x7e).contains(b)) else {
            // Anything this long is not a key, and must not swallow what is typed after it.
            return if self.buf.len() > 64 { Step::Took(self.buf.len(), None) } else { Step::Wait };
        };
        let end = self.buf[2 + len];
        let total = 2 + len + 1;
        let params = String::from_utf8_lossy(&self.buf[2..2 + len]).into_owned();
        let mut parts = params.split(';');
        let first = parts.next().unwrap_or("");
        let modifier = parts.next().unwrap_or("");
        let moved = modifier.starts_with('3') || modifier.starts_with('5');
        let key = match end {
            b'A' => Some(Key::Up),
            b'B' => Some(Key::Down),
            b'C' => Some(if moved { Key::WordRight } else { Key::Right }),
            b'D' => Some(if moved { Key::WordLeft } else { Key::Left }),
            b'H' => Some(Key::Home),
            b'F' => Some(Key::End),
            // Shift-Enter, where the terminal has a way to say so.
            b'u' if first == "13" => Some(Key::NewLine),
            b'~' => match first {
                "1" | "7" => Some(Key::Home),
                "4" | "8" => Some(Key::End),
                "3" => Some(Key::Delete),
                "27" if params.ends_with(";13") => Some(Key::NewLine),
                "200" => {
                    self.paste = Some(Vec::new());
                    None
                }
                _ => None,
            },
            _ => None,
        };
        Step::Took(total, key)
    }
}

fn arrow(b: u8) -> Option<Key> {
    match b {
        b'A' => Some(Key::Up),
        b'B' => Some(Key::Down),
        b'C' => Some(Key::Right),
        b'D' => Some(Key::Left),
        b'H' => Some(Key::Home),
        b'F' => Some(Key::End),
        _ => None,
    }
}

/// Text only: line ends made one kind, and no control bytes, which would end up in a prompt.
fn clean_paste(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes).replace("\r\n", "\n").replace('\r', "\n");
    text.chars().filter(|c| *c == '\n' || *c == '\t' || !c.is_control()).collect()
}

/// What a key did to the line.
#[derive(Debug, PartialEq)]
pub enum Edit {
    Nothing,
    Submit(String),
}

/// The line being typed, which can be several, and what was sent before.
#[derive(Default)]
pub struct Editor {
    text: Vec<char>,
    at: usize,
    history: Vec<String>,
    /// Where in the history Up has got to, and what was being typed before it started.
    back: Option<usize>,
    draft: Vec<char>,
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

impl Editor {
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.at = 0;
        self.back = None;
    }

    /// What was typed goes back, as it was: a command that failed, to be fixed.
    pub fn set(&mut self, text: &str) {
        self.text = text.chars().collect();
        self.at = self.text.len();
    }

    pub fn remember(&mut self, text: &str) {
        if self.history.last().map(String::as_str) != Some(text) {
            self.history.push(text.to_string());
        }
    }

    fn insert(&mut self, s: &str) {
        for c in s.chars() {
            self.text.insert(self.at, c);
            self.at += 1;
        }
    }

    fn line_start(&self, at: usize) -> usize {
        self.text[..at].iter().rposition(|c| *c == '\n').map_or(0, |i| i + 1)
    }

    fn line_end(&self, at: usize) -> usize {
        self.text[at..].iter().position(|c| *c == '\n').map_or(self.text.len(), |i| at + i)
    }

    fn word_left(&self) -> usize {
        let mut i = self.at;
        while i > 0 && !is_word(self.text[i - 1]) {
            i -= 1;
        }
        while i > 0 && is_word(self.text[i - 1]) {
            i -= 1;
        }
        i
    }

    fn word_right(&self) -> usize {
        let mut i = self.at;
        while i < self.text.len() && !is_word(self.text[i]) {
            i += 1;
        }
        while i < self.text.len() && is_word(self.text[i]) {
            i += 1;
        }
        i
    }

    fn up(&mut self) {
        let start = self.line_start(self.at);
        if start == 0 {
            self.history_back();
            return;
        }
        let column = self.at - start;
        let above = self.line_start(start - 1);
        self.at = (above + column).min(start - 1);
    }

    fn down(&mut self) {
        let end = self.line_end(self.at);
        if end == self.text.len() {
            self.history_forward();
            return;
        }
        let column = self.at - self.line_start(self.at);
        let below = end + 1;
        self.at = (below + column).min(self.line_end(below));
    }

    fn history_back(&mut self) {
        let next = match self.back {
            None if self.history.is_empty() => return,
            None => {
                self.draft = self.text.clone();
                self.history.len() - 1
            }
            Some(0) => return,
            Some(i) => i - 1,
        };
        self.back = Some(next);
        self.text = self.history[next].chars().collect();
        self.at = self.text.len();
    }

    fn history_forward(&mut self) {
        let Some(i) = self.back else { return };
        if i + 1 < self.history.len() {
            self.back = Some(i + 1);
            self.text = self.history[i + 1].chars().collect();
        } else {
            self.back = None;
            self.text = std::mem::take(&mut self.draft);
        }
        self.at = self.text.len();
    }

    pub fn key(&mut self, key: &Key) -> Edit {
        match key {
            Key::Char(c) => self.insert(&c.to_string()),
            Key::Paste(s) => self.insert(s),
            Key::Tab => self.insert("    "),
            Key::NewLine => self.insert("\n"),
            Key::Enter => {
                // Terminals that cannot say Shift-Enter can still say this.
                if self.at > 0 && self.text[self.at - 1] == '\\' {
                    self.text[self.at - 1] = '\n';
                    return Edit::Nothing;
                }
                if self.text.iter().all(|c| c.is_whitespace()) {
                    self.clear();
                    return Edit::Nothing;
                }
                let sent: String = self.text.iter().collect();
                self.clear();
                return Edit::Submit(sent);
            }
            Key::Backspace => {
                if self.at > 0 {
                    self.at -= 1;
                    self.text.remove(self.at);
                }
            }
            Key::Delete | Key::Eof => {
                if self.at < self.text.len() {
                    self.text.remove(self.at);
                }
            }
            Key::Left => self.at = self.at.saturating_sub(1),
            Key::Right => self.at = (self.at + 1).min(self.text.len()),
            Key::Up => self.up(),
            Key::Down => self.down(),
            Key::Home => self.at = self.line_start(self.at),
            Key::End => self.at = self.line_end(self.at),
            Key::WordLeft => self.at = self.word_left(),
            Key::WordRight => self.at = self.word_right(),
            Key::DeleteWord => {
                let from = self.word_left();
                self.text.drain(from..self.at);
                self.at = from;
            }
            Key::KillToEnd => {
                let end = self.line_end(self.at);
                self.text.drain(self.at..end);
            }
            Key::KillToStart => {
                let start = self.line_start(self.at);
                self.text.drain(start..self.at);
                self.at = start;
            }
            Key::Interrupt | Key::Esc | Key::Clear | Key::Thinking => {}
        }
        Edit::Nothing
    }

    /// The rows it takes at this width, and the row and column the cursor is on.
    pub fn layout(&self, cols: usize) -> (Vec<String>, (usize, usize)) {
        let mut rows = Vec::new();
        let mut row = PROMPT.to_string();
        let mut used = text_width(PROMPT);
        let mut cursor = (0, used);
        for (i, c) in self.text.iter().enumerate() {
            if i == self.at {
                cursor = (rows.len(), used);
            }
            if *c == '\n' {
                rows.push(std::mem::replace(&mut row, PROMPT_MORE.to_string()));
                used = text_width(PROMPT_MORE);
                continue;
            }
            let w = width(*c);
            if used + w > cols.saturating_sub(1).max(4) {
                rows.push(std::mem::replace(&mut row, PROMPT_MORE.to_string()));
                used = text_width(PROMPT_MORE);
                if i == self.at {
                    cursor = (rows.len(), used);
                }
            }
            row.push(*c);
            used += w;
        }
        if self.at == self.text.len() {
            cursor = (rows.len(), used);
        }
        rows.push(row);
        (rows, cursor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(bytes: &[u8]) -> Vec<Key> {
        let mut d = Decoder::default();
        let mut out = Vec::new();
        d.push(bytes, &mut out);
        out
    }

    #[test]
    fn plain_keys_and_text() {
        assert_eq!(keys(b"hi\r"), [Key::Char('h'), Key::Char('i'), Key::Enter]);
        assert_eq!(keys("é你🙂".as_bytes()), [Key::Char('é'), Key::Char('你'), Key::Char('🙂')]);
        assert_eq!(keys(&[0x03, 0x04, 0x7f, 0x17, 0x15, 0x0b, 0x0c, 0x14]), [Key::Interrupt, Key::Eof, Key::Backspace, Key::DeleteWord, Key::KillToStart, Key::KillToEnd, Key::Clear, Key::Thinking]);
    }

    #[test]
    fn arrows_however_the_terminal_says_them() {
        assert_eq!(keys(b"\x1b[A\x1b[B\x1b[C\x1b[D"), [Key::Up, Key::Down, Key::Right, Key::Left]);
        assert_eq!(keys(b"\x1bOA\x1bOD"), [Key::Up, Key::Left]);
        assert_eq!(keys(b"\x1b[1;5C\x1b[1;3D\x1bb\x1bf"), [Key::WordRight, Key::WordLeft, Key::WordLeft, Key::WordRight]);
        assert_eq!(keys(b"\x1b[H\x1b[F\x1b[1~\x1b[4~\x1b[3~"), [Key::Home, Key::End, Key::Home, Key::End, Key::Delete]);
    }

    #[test]
    fn a_new_line_every_way_a_terminal_can_say_it() {
        assert_eq!(keys(b"\n\x1b\r\x1b[13;2u\x1b[27;2;13~"), [Key::NewLine, Key::NewLine, Key::NewLine, Key::NewLine]);
    }

    #[test]
    fn a_sequence_nobody_knows_is_swallowed_whole() {
        assert_eq!(keys(b"\x1b[99;99zok"), [Key::Char('o'), Key::Char('k')]);
        assert_eq!(keys(b"\x1b[15~x"), [Key::Char('x')]);
    }

    #[test]
    fn pieces_wait_for_the_rest() {
        let mut d = Decoder::default();
        let mut out = Vec::new();
        d.push(b"\x1b", &mut out);
        assert!(out.is_empty() && d.waiting_on_escape());
        d.push(b"[", &mut out);
        assert!(out.is_empty());
        d.push(b"A", &mut out);
        assert_eq!(out, [Key::Up]);
        out.clear();
        let euro = "€".as_bytes();
        d.push(&euro[..2], &mut out);
        assert!(out.is_empty());
        d.push(&euro[2..], &mut out);
        assert_eq!(out, [Key::Char('€')]);
    }

    #[test]
    fn a_lone_escape_is_esc_only_when_it_is_alone() {
        let mut d = Decoder::default();
        let mut out = Vec::new();
        d.push(b"\x1b", &mut out);
        d.flush_escape(&mut out);
        assert_eq!(out, [Key::Esc]);
        assert!(!d.waiting_on_escape());
    }

    #[test]
    fn a_paste_is_text_and_cannot_submit_itself() {
        assert_eq!(keys(b"\x1b[200~one\r\ntwo\rthree\x1b[201~"), [Key::Paste("one\ntwo\nthree".into())]);
        // Its control bytes do not survive as keys or as text.
        assert_eq!(keys(b"\x1b[200~a\x03b\x1b[Ac\x04\x1b[201~d"), [Key::Paste("ab[Ac".into()), Key::Char('d')]);
        let mut e = Editor::default();
        for k in keys(b"\x1b[200~rm -rf /\r\x1b[201~") {
            assert_eq!(e.key(&k), Edit::Nothing);
        }
        assert_eq!(e.layout(80).0, ["> rm -rf /", "  "]);
    }

    #[test]
    fn a_paste_can_arrive_in_pieces_and_is_bounded() {
        let mut d = Decoder::default();
        let mut out = Vec::new();
        d.push(b"\x1b[200~ab", &mut out);
        d.push(b"cd\x1b[20", &mut out);
        assert!(out.is_empty());
        d.push(b"1~e", &mut out);
        assert_eq!(out, [Key::Paste("abcd".into()), Key::Char('e')]);
        let mut d = Decoder::default();
        let mut out = Vec::new();
        d.push(b"\x1b[200~", &mut out);
        let big = vec![b'x'; 2 << 20];
        d.push(&big, &mut out);
        d.push(b"\x1b[201~", &mut out);
        let [Key::Paste(p)] = out.as_slice() else { panic!() };
        assert_eq!(p.len(), MAX_PASTE);
    }

    fn typed(e: &mut Editor, s: &str) {
        e.key(&Key::Paste(s.into()));
    }

    #[test]
    fn enter_sends_and_empty_does_not() {
        let mut e = Editor::default();
        assert_eq!(e.key(&Key::Enter), Edit::Nothing);
        typed(&mut e, "   ");
        assert_eq!(e.key(&Key::Enter), Edit::Nothing);
        assert!(e.is_empty());
        typed(&mut e, "go");
        assert_eq!(e.key(&Key::Enter), Edit::Submit("go".into()));
        assert!(e.is_empty());
    }

    #[test]
    fn a_backslash_before_enter_makes_a_new_line() {
        let mut e = Editor::default();
        typed(&mut e, "one\\");
        assert_eq!(e.key(&Key::Enter), Edit::Nothing);
        typed(&mut e, "two");
        assert_eq!(e.key(&Key::Enter), Edit::Submit("one\ntwo".into()));
    }

    #[test]
    fn moving_and_deleting() {
        let mut e = Editor::default();
        typed(&mut e, "hello big world");
        e.key(&Key::DeleteWord);
        assert_eq!(e.layout(80).0, ["> hello big "]);
        e.key(&Key::WordLeft);
        assert_eq!(e.layout(80).1, (0, 2 + 6));
        e.key(&Key::KillToEnd);
        assert_eq!(e.layout(80).0, ["> hello "]);
        e.key(&Key::Home);
        e.key(&Key::Delete);
        e.key(&Key::Right);
        assert_eq!(e.layout(80), (vec!["> ello ".to_string()], (0, 3)));
        e.key(&Key::KillToStart);
        assert_eq!(e.layout(80).0, ["> llo "]);
    }

    #[test]
    fn up_and_down_move_through_lines_then_history() {
        let mut e = Editor::default();
        e.remember("first");
        e.remember("second");
        e.remember("second");
        typed(&mut e, "draft");
        e.key(&Key::Up);
        assert_eq!(e.layout(80).0, ["> second"]);
        e.key(&Key::Up);
        assert_eq!(e.layout(80).0, ["> first"]);
        e.key(&Key::Up);
        assert_eq!(e.layout(80).0, ["> first"]);
        e.key(&Key::Down);
        e.key(&Key::Down);
        assert_eq!(e.layout(80).0, ["> draft"]);

        e.clear();
        typed(&mut e, "ab\nlonger");
        assert_eq!(e.layout(80).1, (1, 2 + 6));
        e.key(&Key::Up);
        assert_eq!(e.layout(80).1, (0, 2 + 2));
        e.key(&Key::Down);
        assert_eq!(e.layout(80).1, (1, 2 + 2));
    }

    #[test]
    fn long_lines_wrap_and_the_cursor_follows() {
        let mut e = Editor::default();
        typed(&mut e, "abcdefghij");
        let (rows, cursor) = e.layout(8);
        assert_eq!(rows, ["> abcde", "  fghij"]);
        assert_eq!(cursor, (1, 7));
        for _ in 0..5 {
            e.key(&Key::Left);
        }
        assert_eq!(e.layout(8).1, (1, 2));
        typed(&mut e, "你");
        let (rows, _) = e.layout(8);
        assert!(rows.iter().all(|r| text_width(r) <= 7), "{rows:?}");
    }
}
