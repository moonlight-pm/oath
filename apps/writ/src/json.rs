//! JSON, ours: writ reads a model's stream and writes its requests, and nothing else here needs
//! a crate. Objects keep their order and numbers keep their text, so what a provider sent can go
//! back unchanged, and a request's prefix stays the same from one turn to the next. A duplicate
//! key is an error: two readers of the same tool call must not see different arguments.

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Num(String),
    Str(String),
    Arr(Vec<Value>),
    Obj(Vec<(String, Value)>),
}

const MAX_DEPTH: usize = 64;
const MAX_KEYS: usize = 4096;

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Obj(o) => o.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn arr(&self) -> Option<&[Value]> {
        match self {
            Value::Arr(a) => Some(a),
            _ => None,
        }
    }

    pub fn u64(&self) -> Option<u64> {
        match self {
            Value::Num(n) => n.parse().ok(),
            _ => None,
        }
    }

    pub fn f64(&self) -> Option<f64> {
        match self {
            Value::Num(n) => n.parse().ok(),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    pub fn write(&self, out: &mut String) {
        match self {
            Value::Null => out.push_str("null"),
            Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Value::Num(n) => out.push_str(n),
            Value::Str(s) => write_str(s, out),
            Value::Arr(a) => {
                out.push('[');
                for (i, v) in a.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    v.write(out);
                }
                out.push(']');
            }
            Value::Obj(o) => {
                out.push('{');
                for (i, (k, v)) in o.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write_str(k, out);
                    out.push(':');
                    v.write(out);
                }
                out.push('}');
            }
        }
    }

    pub fn text(&self) -> String {
        let mut s = String::new();
        self.write(&mut s);
        s
    }
}

pub fn s(v: &str) -> Value {
    Value::Str(v.to_string())
}

pub fn n(v: u64) -> Value {
    Value::Num(v.to_string())
}

pub fn obj<const N: usize>(pairs: [(&str, Value); N]) -> Value {
    Value::Obj(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

fn write_str(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

pub fn parse(text: &str) -> Result<Value, String> {
    let mut p = Parser { b: text.as_bytes(), text, at: 0 };
    let v = p.value(0)?;
    p.space();
    if p.at != p.b.len() {
        return Err(p.bad("text after the value"));
    }
    Ok(v)
}

struct Parser<'a> {
    b: &'a [u8],
    text: &'a str,
    at: usize,
}

impl Parser<'_> {
    fn bad(&self, why: &str) -> String {
        format!("JSON at byte {}: {why}", self.at)
    }

    fn space(&mut self) {
        while matches!(self.b.get(self.at), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.at += 1;
        }
    }

    fn literal(&mut self, word: &str, v: Value) -> Result<Value, String> {
        if self.b[self.at..].starts_with(word.as_bytes()) {
            self.at += word.len();
            Ok(v)
        } else {
            Err(self.bad("unknown word"))
        }
    }

    fn value(&mut self, depth: usize) -> Result<Value, String> {
        if depth > MAX_DEPTH {
            return Err(self.bad("nested too deep"));
        }
        self.space();
        match self.b.get(self.at) {
            None => Err(self.bad("ended early")),
            Some(b'n') => self.literal("null", Value::Null),
            Some(b't') => self.literal("true", Value::Bool(true)),
            Some(b'f') => self.literal("false", Value::Bool(false)),
            Some(b'"') => Ok(Value::Str(self.string()?)),
            Some(b'[') => self.array(depth),
            Some(b'{') => self.object(depth),
            Some(_) => self.number(),
        }
    }

    fn array(&mut self, depth: usize) -> Result<Value, String> {
        self.at += 1;
        let mut items = Vec::new();
        self.space();
        if self.b.get(self.at) == Some(&b']') {
            self.at += 1;
            return Ok(Value::Arr(items));
        }
        loop {
            items.push(self.value(depth + 1)?);
            self.space();
            match self.b.get(self.at) {
                Some(b',') => self.at += 1,
                Some(b']') => {
                    self.at += 1;
                    return Ok(Value::Arr(items));
                }
                _ => return Err(self.bad("expected , or ]")),
            }
        }
    }

    fn object(&mut self, depth: usize) -> Result<Value, String> {
        self.at += 1;
        let mut items: Vec<(String, Value)> = Vec::new();
        self.space();
        if self.b.get(self.at) == Some(&b'}') {
            self.at += 1;
            return Ok(Value::Obj(items));
        }
        loop {
            self.space();
            if self.b.get(self.at) != Some(&b'"') {
                return Err(self.bad("expected a key"));
            }
            let k = self.string()?;
            if items.len() >= MAX_KEYS {
                return Err(self.bad("too many keys"));
            }
            if items.iter().any(|(have, _)| *have == k) {
                return Err(self.bad("duplicate key"));
            }
            self.space();
            if self.b.get(self.at) != Some(&b':') {
                return Err(self.bad("expected :"));
            }
            self.at += 1;
            items.push((k, self.value(depth + 1)?));
            self.space();
            match self.b.get(self.at) {
                Some(b',') => self.at += 1,
                Some(b'}') => {
                    self.at += 1;
                    return Ok(Value::Obj(items));
                }
                _ => return Err(self.bad("expected , or }")),
            }
        }
    }

    fn digits(&mut self) -> bool {
        let start = self.at;
        while matches!(self.b.get(self.at), Some(b'0'..=b'9')) {
            self.at += 1;
        }
        self.at > start
    }

    fn number(&mut self) -> Result<Value, String> {
        let start = self.at;
        if self.b.get(self.at) == Some(&b'-') {
            self.at += 1;
        }
        if self.b.get(self.at) == Some(&b'0') {
            self.at += 1;
        } else if !self.digits() {
            return Err(self.bad("not a value"));
        }
        if self.b.get(self.at) == Some(&b'.') {
            self.at += 1;
            if !self.digits() {
                return Err(self.bad("bad fraction"));
            }
        }
        if matches!(self.b.get(self.at), Some(b'e' | b'E')) {
            self.at += 1;
            if matches!(self.b.get(self.at), Some(b'+' | b'-')) {
                self.at += 1;
            }
            if !self.digits() {
                return Err(self.bad("bad exponent"));
            }
        }
        Ok(Value::Num(self.text[start..self.at].to_string()))
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let h = self.b.get(self.at..self.at + 4).ok_or_else(|| self.bad("cut-off \\u"))?;
        if !h.iter().all(u8::is_ascii_hexdigit) {
            return Err(self.bad("bad \\u"));
        }
        let v = u32::from_str_radix(&self.text[self.at..self.at + 4], 16).map_err(|_| self.bad("bad \\u"))?;
        self.at += 4;
        Ok(v)
    }

    fn string(&mut self) -> Result<String, String> {
        self.at += 1;
        let mut out = String::new();
        loop {
            let start = self.at;
            while !matches!(self.b.get(self.at), None | Some(b'"' | b'\\' | 0..=0x1f)) {
                self.at += 1;
            }
            out.push_str(&self.text[start..self.at]);
            match self.b.get(self.at) {
                None => return Err(self.bad("cut-off string")),
                Some(b'"') => {
                    self.at += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.at += 1;
                    let e = *self.b.get(self.at).ok_or_else(|| self.bad("cut-off escape"))?;
                    self.at += 1;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => out.push(self.unicode()?),
                        _ => return Err(self.bad("bad escape")),
                    }
                }
                Some(_) => return Err(self.bad("control character in a string")),
            }
        }
    }

    /// A \u escape, past the `u`. A surrogate alone has no character, so it shows as U+FFFD.
    fn unicode(&mut self) -> Result<char, String> {
        let first = self.hex4()?;
        if (0xd800..0xdc00).contains(&first) && self.b[self.at..].starts_with(b"\\u") {
            let save = self.at;
            self.at += 2;
            let low = self.hex4()?;
            if (0xdc00..0xe000).contains(&low) {
                let c = 0x10000 + ((first - 0xd800) << 10) + (low - 0xdc00);
                return Ok(char::from_u32(c).unwrap_or('\u{fffd}'));
            }
            self.at = save;
        }
        Ok(char::from_u32(first).unwrap_or('\u{fffd}'))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_goes_in_comes_out_in_order() {
        for t in [
            r#"{"b":1,"a":[true,false,null,"x"],"c":{"d":-1.5e+3}}"#,
            r#"[]"#,
            r#"{}"#,
            r#""\"\\\n\r\t\u0001""#,
            "\"héllo 你好 🙂\"",
            "0.10",
            "1E5",
        ] {
            assert_eq!(parse(t).unwrap().text(), t);
        }
    }

    #[test]
    fn escapes_and_surrogates() {
        assert_eq!(parse(r#""\ud83d\ude00\u00e9\/""#).unwrap(), s("😀é/"));
        assert_eq!(parse(r#""\ud83dx""#).unwrap(), s("\u{fffd}x"));
        assert_eq!(parse(r#""\ud83d\u0041""#).unwrap(), s("\u{fffd}A"));
        assert_eq!(parse(r#""\ude00""#).unwrap(), s("\u{fffd}"));
    }

    #[test]
    fn bad_json_is_refused() {
        for t in [
            "", " ", "{", "}", "[", "]", "[1,]", "{\"a\":1,}", "{\"a\"}", "{a:1}", "01", "-", "1.", "1e",
            ".5", "+1", "nul", "truex", "\"abc", "\"\\x\"", "\"\\u12\"", "\"\\u12G4\"", "[1 2]", "1 2",
            "\"a\nb\"", "\"a\u{1}b\"", "NaN", "Infinity",
        ] {
            assert!(parse(t).is_err(), "{t:?}");
        }
    }

    #[test]
    fn a_duplicate_key_is_refused() {
        assert!(parse(r#"{"path":"a","path":"b"}"#).is_err());
        assert!(parse(r#"{"x":{"path":"a","path":"b"}}"#).is_err());
        assert!(parse(r#"{"path":"a","pat\u0068":"b"}"#).is_err());
    }

    #[test]
    fn deep_nesting_is_refused_and_does_not_overflow() {
        let deep = "[".repeat(1_000_000);
        assert!(parse(&deep).is_err());
        let ok = format!("{}{}", "[".repeat(MAX_DEPTH), "]".repeat(MAX_DEPTH));
        assert!(parse(&ok).is_ok());
    }

    #[test]
    fn numbers_keep_their_text_and_read_as_numbers() {
        assert_eq!(parse("123").unwrap().u64(), Some(123));
        assert_eq!(parse("1.5").unwrap().u64(), None);
        assert_eq!(parse("1.5").unwrap().f64(), Some(1.5));
        assert_eq!(parse("18446744073709551616").unwrap().text(), "18446744073709551616");
    }

    #[test]
    fn a_control_character_is_escaped_so_a_line_stays_one_line() {
        let t = s("a\nb\u{1}\u{1f}").text();
        assert!(!t.chars().any(|c| (c as u32) < 0x20), "{t}");
        assert_eq!(parse(&t).unwrap(), s("a\nb\u{1}\u{1f}"));
    }
}
