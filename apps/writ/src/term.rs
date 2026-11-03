//! Everything from a box or a model reaches the terminal through here, so what it prints
//! cannot move the cursor, rewrite an approval, set the clipboard, or hide text from the
//! person reading. Control bytes and invisible characters come out as something you can see.

/// Cleans a byte stream that may split a UTF-8 character across reads.
#[derive(Default)]
pub struct Clean {
    carry: Vec<u8>,
}

impl Clean {
    pub fn push(&mut self, bytes: &[u8]) -> String {
        self.carry.extend_from_slice(bytes);
        let mut out = String::new();
        let mut rest: &[u8] = &self.carry;
        loop {
            match std::str::from_utf8(rest) {
                Ok(s) => {
                    scrub_into(s, &mut out);
                    rest = &[];
                    break;
                }
                Err(e) => {
                    let (good, after) = rest.split_at(e.valid_up_to());
                    scrub_into(std::str::from_utf8(good).unwrap_or(""), &mut out);
                    match e.error_len() {
                        Some(n) => {
                            for b in &after[..n] {
                                out.push_str(&format!("⟨0x{b:02x}⟩"));
                            }
                            rest = &after[n..];
                        }
                        // A character cut off by the read: the next one finishes it.
                        None => {
                            rest = after;
                            break;
                        }
                    }
                }
            }
        }
        self.carry = rest.to_vec();
        out
    }

    pub fn finish(&mut self) -> String {
        let out = self.carry.iter().map(|b| format!("⟨0x{b:02x}⟩")).collect();
        self.carry.clear();
        out
    }
}

fn scrub_into(s: &str, out: &mut String) {
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\n' | '\t' => out.push(c),
            '\r' if chars.peek() == Some(&'\n') => {}
            c if (c as u32) < 0x20 => {
                out.push('^');
                out.push(((c as u8) + 0x40) as char);
            }
            '\u{7f}' => out.push_str("^?"),
            c if invisible(c) => out.push_str(&format!("⟨U+{:04X}⟩", c as u32)),
            c => out.push(c),
        }
    }
}

/// C1 controls, and characters that change or hide what is read without showing:
/// bidirectional controls, zero-width characters, Unicode tags.
fn invisible(c: char) -> bool {
    matches!(c as u32,
        0x80..=0x9f
        | 0xad
        | 0x34f
        | 0x61c
        | 0x180e
        | 0x200b..=0x200f
        | 0x202a..=0x202e
        | 0x2060..=0x2064
        | 0x2066..=0x206f
        | 0xfeff
        | 0xfff9..=0xfffb
        | 0x1d173..=0x1d17a
        | 0xe0000..=0xe0fff)
}

/// One whole string, cleaned.
pub fn scrub(s: &str) -> String {
    let mut out = String::new();
    scrub_into(s, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clean(b: &[u8]) -> String {
        let mut c = Clean::default();
        let mut s = c.push(b);
        s.push_str(&c.finish());
        s
    }

    #[test]
    fn plain_text_passes() {
        assert_eq!(clean("héllo\tworld\n€ 你好 🙂\n".as_bytes()), "héllo\tworld\n€ 你好 🙂\n");
    }

    #[test]
    fn escape_sequences_lose_their_escape() {
        assert_eq!(clean(b"\x1b]52;c;aGk=\x07"), "^[]52;c;aGk=^G");
        assert_eq!(clean(b"\x1b[2J\x1b[H"), "^[[2J^[[H");
        assert!(!clean(b"\x1b\x07\x08\x00\x7f").contains(['\x1b', '\x07', '\x08', '\0', '\x7f']));
    }

    #[test]
    fn carriage_return_cannot_overwrite_a_line() {
        assert_eq!(clean(b"a\r\nb"), "a\nb");
        assert_eq!(clean(b"approve\rdeny"), "approve^Mdeny");
    }

    #[test]
    fn c1_and_invisibles_show() {
        // CSI as a C1 control: 8-bit escape sequences work on some terminals.
        assert_eq!(clean("\u{9b}2J".as_bytes()), "⟨U+009B⟩2J");
        assert_eq!(clean("a\u{202e}b".as_bytes()), "a⟨U+202E⟩b");
        assert_eq!(clean("a\u{200b}b".as_bytes()), "a⟨U+200B⟩b");
        assert_eq!(clean("\u{e0041}".as_bytes()), "⟨U+E0041⟩");
    }

    #[test]
    fn invalid_utf8_shows_as_bytes() {
        assert_eq!(clean(b"a\xffb"), "a⟨0xff⟩b");
        assert_eq!(clean(b"a\xe2\x82"), "a⟨0xe2⟩⟨0x82⟩");
    }

    #[test]
    fn a_character_split_across_reads_survives() {
        let mut c = Clean::default();
        let euro = "€".as_bytes();
        assert_eq!(c.push(&euro[..1]), "");
        assert_eq!(c.push(&euro[1..2]), "");
        assert_eq!(c.push(&euro[2..]), "€");
        assert_eq!(c.finish(), "");
    }
}
