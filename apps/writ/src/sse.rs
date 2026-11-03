//! Server-sent events, as a model's stream sends them. A comment line is what OpenRouter keeps
//! the connection open with. Text outside any event is kept, because an error before the stream
//! starts is a plain body.

/// The longest line: a large tool call in one chunk fits, and a stream with no newline stops.
const MAX_LINE: usize = 32 << 20;
/// How much of a body that is not a stream is kept, to show as the error.
const MAX_BODY: usize = 64 << 10;

#[derive(Debug, PartialEq)]
pub enum Event {
    Data(String),
    Done,
}

#[derive(Default)]
pub struct Sse {
    line: Vec<u8>,
    data: Vec<u8>,
    in_event: bool,
    /// What is not a field of any event: an error's body, or a stream that is not one.
    pub body: Vec<u8>,
}

impl Sse {
    /// Parse more of the stream, handing each finished event to `f`. A `Err` from `f` stops it.
    pub fn push<F>(&mut self, bytes: &[u8], f: &mut F) -> Result<(), String>
    where
        F: FnMut(Event) -> Result<(), String>,
    {
        for &b in bytes {
            if b == b'\n' {
                self.end_line(f)?;
            } else {
                if self.line.len() >= MAX_LINE {
                    return Err(format!("a line in the stream is over {MAX_LINE} bytes"));
                }
                self.line.push(b);
            }
        }
        Ok(())
    }

    /// The stream ended. An event with no blank line after it was cut off, so it does not count.
    pub fn finish(&mut self) {
        if !self.in_event && self.body.len() < MAX_BODY {
            let room = MAX_BODY - self.body.len();
            let take = self.line.len().min(room);
            self.body.extend_from_slice(&self.line[..take]);
        }
        self.line.clear();
        self.data.clear();
        self.in_event = false;
    }

    fn end_line<F>(&mut self, f: &mut F) -> Result<(), String>
    where
        F: FnMut(Event) -> Result<(), String>,
    {
        let mut line = std::mem::take(&mut self.line);
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        if line.is_empty() {
            if self.in_event {
                self.in_event = false;
                let data = std::mem::take(&mut self.data);
                let text = String::from_utf8(data).map_err(|_| "the stream is not UTF-8".to_string())?;
                f(if text == "[DONE]" { Event::Done } else { Event::Data(text) })?;
            }
            return Ok(());
        }
        if line[0] == b':' {
            return Ok(());
        }
        if let Some(rest) = line.strip_prefix(b"data:") {
            let rest = rest.strip_prefix(b" ").unwrap_or(rest);
            if self.in_event {
                self.data.push(b'\n');
            }
            self.data.extend_from_slice(rest);
            self.in_event = true;
            return Ok(());
        }
        // Another field of an event, which nothing here reads.
        if line.starts_with(b"event:") || line.starts_with(b"id:") || line.starts_with(b"retry:") {
            return Ok(());
        }
        if self.body.len() < MAX_BODY {
            let room = MAX_BODY - self.body.len();
            let take = line.len().min(room);
            self.body.extend_from_slice(&line[..take]);
            self.body.push(b'\n');
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn events(bytes: &[u8]) -> Vec<Event> {
        let mut out = Vec::new();
        let mut sse = Sse::default();
        sse.push(bytes, &mut |e| {
            out.push(e);
            Ok(())
        })
        .unwrap();
        sse.finish();
        out
    }

    fn data(s: &str) -> Event {
        Event::Data(s.to_string())
    }

    #[test]
    fn events_and_done() {
        assert_eq!(events(b"data: {\"a\":1}\n\ndata: [DONE]\n\n"), [data("{\"a\":1}"), Event::Done]);
    }

    #[test]
    fn comments_and_other_fields_are_not_events() {
        assert_eq!(events(b": OPENROUTER PROCESSING\n\nevent: x\nid: 1\ndata: y\n\n"), [data("y")]);
    }

    #[test]
    fn crlf_no_space_and_many_lines() {
        assert_eq!(events(b"data:a\r\ndata: b\r\n\r\n"), [data("a\nb")]);
    }

    #[test]
    fn an_event_with_no_blank_line_was_cut_off() {
        assert_eq!(events(b"data: {\"a\":1}\n"), []);
        assert_eq!(events(b"data: {\"a\":1}"), []);
    }

    #[test]
    fn every_split_gives_the_same_events() {
        let whole = "data: é€🙂\n\n: x\n\ndata: [DONE]\n\n".as_bytes();
        for at in 0..whole.len() {
            let mut out = Vec::new();
            let mut sse = Sse::default();
            let mut f = |e| {
                out.push(e);
                Ok(())
            };
            sse.push(&whole[..at], &mut f).unwrap();
            sse.push(&whole[at..], &mut f).unwrap();
            assert_eq!(out, [data("é€🙂"), Event::Done], "split at {at}");
        }
    }

    #[test]
    fn a_body_that_is_not_a_stream_is_kept_and_bounded() {
        let mut sse = Sse::default();
        sse.push(b"{\"error\":{\"code\":500}}\nmore", &mut |_| Ok(())).unwrap();
        sse.finish();
        assert_eq!(sse.body, b"{\"error\":{\"code\":500}}\nmore");
        let mut sse = Sse::default();
        let big = vec![b'x'; 200_000];
        sse.push(&big, &mut |_| Ok(())).unwrap();
        sse.push(b"\n", &mut |_| Ok(())).unwrap();
        sse.finish();
        assert!(sse.body.len() <= MAX_BODY + 1);
    }

    #[test]
    fn bad_utf8_in_an_event_stops_the_stream() {
        let mut sse = Sse::default();
        assert!(sse.push(b"data: \xff\n\n", &mut |_| Ok(())).is_err());
    }

    #[test]
    fn a_line_with_no_end_stops_the_stream() {
        let mut sse = Sse::default();
        let big = vec![b'x'; MAX_LINE + 1];
        assert!(sse.push(&big, &mut |_| Ok(())).is_err());
    }
}
