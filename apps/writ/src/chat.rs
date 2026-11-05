//! Chat Completions: the request, and the stream back into one turn. Local models and OpenRouter
//! speak it alike. What matters most here is what is refused: a call the model did not finish
//! never runs, however much of it arrived. Only a stream that ran to its `[DONE]`, after a
//! reason to stop, with arguments that parse, gives calls to run.

use crate::json::{self, n, obj, s, Value};
use crate::sse::{Event, Sse};

const MAX_CALLS: usize = 64;
const MAX_ARGUMENTS: usize = 8 << 20;
const MAX_NAME: usize = 64;

#[derive(Clone, Debug, PartialEq)]
pub struct Call {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Finish {
    Stop,
    ToolCalls,
    Length,
    ContentFilter,
    Error(String),
    /// The stream ended with no reason given: the connection dropped or a process died.
    CutOff,
    Other(String),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Usage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub cached_tokens: u64,
    /// In dollars, where a provider says.
    pub cost: Option<f64>,
    /// Local servers report their speed.
    pub tokens_per_second: Option<f64>,
    /// Who ran it, where a router says: who was sent what the model read.
    pub provider: Option<String>,
}

/// What the model sent in one turn.
#[derive(Clone, Debug, PartialEq)]
pub struct Turn {
    pub content: String,
    pub reasoning: String,
    /// Kept whole: some models need it sent back unchanged with the next request.
    pub reasoning_details: Option<Value>,
    pub calls: Vec<Call>,
    pub finish: Finish,
    pub usage: Option<Usage>,
    /// The error as it came, when one did: what to do next is in its fields, not its words.
    pub error: Option<Value>,
}

impl Turn {
    /// The calls that may run, or why none may. All of them or none: a turn that is wrong in
    /// one call is not a turn to act on.
    pub fn runnable(&self) -> Result<Vec<(Call, Value)>, String> {
        match &self.finish {
            Finish::ToolCalls | Finish::Stop => {}
            Finish::Length => return Err("the model ran out of tokens in this turn".into()),
            Finish::ContentFilter => return Err("a content filter stopped this turn".into()),
            Finish::Error(e) => return Err(format!("the stream failed: {e}")),
            Finish::CutOff => return Err("the stream was cut off".into()),
            Finish::Other(r) => return Err(format!("the turn ended with {r:?}")),
        }
        let mut out = Vec::new();
        for c in &self.calls {
            if !good_name(&c.name) {
                return Err(format!("a tool call has the name {:?}", c.name));
            }
            if c.id.is_empty() {
                return Err(format!("the call to {} has no id", c.name));
            }
            if out.iter().any(|(have, _): &(Call, Value)| have.id == c.id) {
                return Err(format!("two calls have the id {:?}", c.id));
            }
            let args = json::parse(&c.arguments).map_err(|e| format!("the call to {}: {e}", c.name))?;
            if !matches!(args, Value::Obj(_)) {
                return Err(format!("the call to {} has arguments that are not an object", c.name));
            }
            out.push((c.clone(), args));
        }
        Ok(out)
    }
}

fn good_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}

/// A piece of text as it arrives, for showing.
#[derive(Debug, PartialEq)]
pub enum Delta<'a> {
    Content(&'a str),
    Reasoning(&'a str),
    /// writ's own word on how the turn is going, and no part of what the model said.
    Note(&'a str),
}

/// Turns the stream's bytes into a `Turn`.
#[derive(Default)]
pub struct Assembler {
    sse: Sse,
    content: String,
    reasoning: String,
    reasoning_details: Vec<Value>,
    calls: Vec<Partial>,
    finish: Option<Finish>,
    usage: Option<Usage>,
    provider: Option<String>,
    error: Option<Value>,
    failed: Option<String>,
    done: bool,
}

#[derive(Default)]
struct Partial {
    index: u64,
    id: String,
    name: String,
    arguments: String,
}

impl Assembler {
    /// More of the stream. `on` sees text as it arrives. Nothing that arrives is fatal to the
    /// caller; it ends the turn as an error instead.
    pub fn push<F: FnMut(Delta)>(&mut self, bytes: &[u8], on: &mut F) {
        if self.failed.is_some() {
            return;
        }
        let mut sse = std::mem::take(&mut self.sse);
        let r = sse.push(bytes, &mut |e| {
            match e {
                Event::Done => self.done = true,
                Event::Data(d) => self.event(&d, on)?,
            }
            Ok(())
        });
        self.sse = sse;
        if let Err(e) = r {
            self.failed = Some(e);
        }
    }

    fn event<F: FnMut(Delta)>(&mut self, data: &str, on: &mut F) -> Result<(), String> {
        let v = json::parse(data)?;
        if let Some(e) = v.get("error") {
            self.error = Some(e.clone());
            return Err(error_text(e));
        }
        if let Some(p) = v.get("provider").and_then(|p| p.str()) {
            self.provider = Some(p.to_string());
        }
        if let Some(u) = v.get("usage").filter(|u| !u.is_null()) {
            self.usage = Some(usage(u, v.get("timings")));
        }
        let Some(choice) = v.get("choices").and_then(|c| c.arr()).and_then(|c| c.first()) else {
            return Ok(());
        };
        // An error after the stream began: the status was already 200.
        if let Some(e) = choice.get("error") {
            self.error = Some(e.clone());
            return Err(error_text(e));
        }
        if let Some(delta) = choice.get("delta") {
            if let Some(t) = delta.get("content").and_then(|t| t.str()) {
                if !t.is_empty() {
                    self.content.push_str(t);
                    on(Delta::Content(t));
                }
            }
            for key in ["reasoning_content", "reasoning"] {
                if let Some(t) = delta.get(key).and_then(|t| t.str()) {
                    if !t.is_empty() {
                        self.reasoning.push_str(t);
                        on(Delta::Reasoning(t));
                    }
                    break;
                }
            }
            if let Some(details) = delta.get("reasoning_details").and_then(|d| d.arr()) {
                for d in details {
                    add_detail(&mut self.reasoning_details, d);
                }
            }
            if let Some(calls) = delta.get("tool_calls").and_then(|c| c.arr()) {
                for c in calls {
                    self.call_delta(c)?;
                }
            }
        }
        if let Some(r) = choice.get("finish_reason").and_then(|r| r.str()) {
            self.finish = Some(match r {
                "stop" => Finish::Stop,
                "tool_calls" => Finish::ToolCalls,
                "length" => Finish::Length,
                "content_filter" => Finish::ContentFilter,
                "error" => Finish::Error("the provider ended the stream with an error".into()),
                other => Finish::Other(other.to_string()),
            });
        }
        Ok(())
    }

    /// A piece of a call. Pieces of one call share an `index`, and may arrive between pieces
    /// of another.
    fn call_delta(&mut self, c: &Value) -> Result<(), String> {
        let index = c.get("index").and_then(|i| i.u64()).ok_or("a tool call with no index")?;
        let at = match self.calls.iter().position(|p| p.index == index) {
            Some(at) => at,
            None => {
                if self.calls.len() >= MAX_CALLS {
                    return Err(format!("over {MAX_CALLS} tool calls in one turn"));
                }
                self.calls.push(Partial { index, ..Partial::default() });
                self.calls.len() - 1
            }
        };
        let p = &mut self.calls[at];
        // The first of each stands: the format does not say they come once, and some hosts send them
        // again with every piece.
        if let Some(id) = c.get("id").and_then(|i| i.str()).filter(|_| p.id.is_empty()) {
            p.id = id.to_string();
        }
        if let Some(f) = c.get("function") {
            if let Some(name) = f.get("name").and_then(|n| n.str()).filter(|_| p.name.is_empty()) {
                p.name = name.to_string();
            }
            if let Some(args) = f.get("arguments").and_then(|a| a.str()) {
                if p.arguments.len() + args.len() > MAX_ARGUMENTS {
                    return Err(format!("a tool call's arguments are over {MAX_ARGUMENTS} bytes"));
                }
                p.arguments.push_str(args);
            }
        }
        Ok(())
    }

    /// The stream is over: `transport` says how its process ended, if badly.
    pub fn finish(mut self, transport: Option<String>) -> Turn {
        self.sse.finish();
        let body = String::from_utf8_lossy(&self.sse.body).trim().to_string();
        // What failed before any event: the body is the reason, and may name it better.
        let from_body = (!body.is_empty()).then(|| match json::parse(&body) {
            Ok(v) => {
                if let Some(e) = v.get("error") {
                    self.error = Some(e.clone());
                }
                v.get("error").map(error_text).unwrap_or(body.clone())
            }
            Err(_) => body.clone(),
        });
        let finish = if let Some(e) = self.failed {
            Finish::Error(e)
        } else if let Some(e) = from_body {
            Finish::Error(e)
        } else if let Some(e) = transport {
            Finish::Error(e)
        } else {
            match self.finish {
                // A reason to run calls counts only once the stream has said it is over.
                Some(Finish::Stop | Finish::ToolCalls) if !self.done => Finish::CutOff,
                Some(f) => f,
                None => Finish::CutOff,
            }
        };
        let mut calls: Vec<Partial> = self.calls;
        calls.sort_by_key(|p| p.index);
        Turn {
            content: self.content,
            reasoning: self.reasoning,
            reasoning_details: (!self.reasoning_details.is_empty()).then_some(Value::Arr(self.reasoning_details)),
            calls: calls
                .into_iter()
                .map(|p| Call { id: p.id, name: p.name, arguments: p.arguments })
                .collect(),
            finish,
            usage: self.usage.map(|u| Usage { provider: self.provider, ..u }),
            error: self.error,
        }
    }
}

/// One more piece of a turn's reasoning. A stream sends it a word at a time; what goes back is what
/// a whole answer would have held, or a long thought would cost its words again in `{"type":...}`.
/// Pieces of one block have the same type, index and format. Encrypted blocks stand alone.
fn add_detail(list: &mut Vec<Value>, piece: &Value) {
    let field = match piece.get("type").and_then(|t| t.str()) {
        Some("reasoning.text") => "text",
        Some("reasoning.summary") => "summary",
        _ => {
            list.push(piece.clone());
            return;
        }
    };
    let said = |v: &Value, key: &str| v.get(key).filter(|x| !x.is_null()).cloned();
    let same = list.last().is_some_and(|last| {
        ["type", "index", "format"].iter().all(|k| said(last, k) == said(piece, k))
            && (said(last, "id").is_none() || said(piece, "id").is_none() || said(last, "id") == said(piece, "id"))
    });
    let (true, Some(Value::Obj(last)), Value::Obj(pairs)) = (same, list.last_mut(), piece) else {
        list.push(piece.clone());
        return;
    };
    for (k, v) in pairs {
        match last.iter_mut().find(|(have, _)| have == k) {
            Some((_, Value::Str(have))) if k == field => have.push_str(v.str().unwrap_or("")),
            // A signature and an id arrive with one piece, and are null or absent in the others.
            Some((_, have)) if have.is_null() => *have = v.clone(),
            Some(_) => {}
            None => last.push((k.clone(), v.clone())),
        }
    }
}

fn error_text(e: &Value) -> String {
    let message = match e.get("message").and_then(|m| m.str()) {
        Some(m) => m.to_string(),
        None => e.str().map(str::to_string).unwrap_or_else(|| e.text()),
    };
    // A router's word is often only "Provider returned error", and the provider's own is here.
    match e.get("metadata").and_then(|m| m.get("raw")).filter(|r| !r.is_null()) {
        Some(raw) => {
            let raw = raw.str().map(str::to_string).unwrap_or_else(|| raw.text());
            format!("{message}: {}", raw.chars().take(300).collect::<String>())
        }
        None => message,
    }
}

fn usage(u: &Value, timings: Option<&Value>) -> Usage {
    let count = |k: &str| u.get(k).and_then(|v| v.u64()).unwrap_or(0);
    Usage {
        prompt_tokens: count("prompt_tokens"),
        completion_tokens: count("completion_tokens"),
        cached_tokens: u
            .get("prompt_tokens_details")
            .and_then(|d| d.get("cached_tokens"))
            .and_then(|v| v.u64())
            .unwrap_or(0),
        cost: u.get("cost").and_then(|v| v.f64()),
        tokens_per_second: timings.and_then(|t| t.get("predicted_per_second")).and_then(|v| v.f64()),
        provider: None,
    }
}

pub struct Tool {
    pub name: &'static str,
    pub description: &'static str,
    /// A JSON schema for its arguments.
    pub parameters: &'static str,
}

/// The fields of one whole request. The tools go in every time: a model has no memory of them.
pub fn request_fields(model: &str, messages: &[Value], tools: &[Tool], max_tokens: u64) -> Result<Vec<(String, Value)>, String> {
    let mut pairs = vec![
        ("model", s(model)),
        ("messages", Value::Arr(messages.to_vec())),
        ("stream", Value::Bool(true)),
        ("stream_options", obj([("include_usage", Value::Bool(true))])),
        ("max_tokens", n(max_tokens)),
    ];
    if !tools.is_empty() {
        let mut list = Vec::new();
        for t in tools {
            let parameters = json::parse(t.parameters).map_err(|e| format!("tool {}: {e}", t.name))?;
            list.push(obj([
                ("type", s("function")),
                (
                    "function",
                    obj([("name", s(t.name)), ("description", s(t.description)), ("parameters", parameters)]),
                ),
            ]));
        }
        pairs.push(("tools", Value::Arr(list)));
    }
    Ok(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

pub fn request(model: &str, messages: &[Value], tools: &[Tool], max_tokens: u64) -> Result<String, String> {
    Ok(Value::Obj(request_fields(model, messages, tools, max_tokens)?).text())
}

pub fn message(role: &str, content: &str) -> Value {
    obj([("role", s(role)), ("content", s(content))])
}

/// An assistant's message says which model wrote it when it holds reasoning, and that goes no further
/// than the session.
pub const WRITTEN_BY: &str = "written_by";

/// A conversation as one model is sent it: `Some` a hosted one by name, `None` a local one. Reasoning
/// goes back to the model that wrote it, unchanged, as some hosts demand, and to no other, since a
/// signed block means nothing to it. A hosted API wants null for no words beside a call, and takes no
/// empty message at all.
pub fn for_model(messages: &[Value], model: Option<&str>) -> Vec<Value> {
    let mut out = Vec::with_capacity(messages.len());
    for m in messages {
        let Value::Obj(pairs) = m else {
            out.push(m.clone());
            continue;
        };
        if m.get("role").and_then(|r| r.str()) != Some("assistant") {
            out.push(m.clone());
            continue;
        }
        let hosted = model.is_some();
        let empty = hosted && m.get("content").and_then(|c| c.str()).is_some_and(str::is_empty);
        if empty && m.get("tool_calls").is_none() {
            continue;
        }
        let mine = hosted && m.get(WRITTEN_BY).and_then(|w| w.str()) == model;
        let kept = pairs.iter().filter(|(k, _)| match k.as_str() {
            WRITTEN_BY => false,
            "reasoning" | "reasoning_details" => mine,
            _ => true,
        });
        out.push(Value::Obj(kept.map(|(k, v)| (k.clone(), if empty && k == "content" { Value::Null } else { v.clone() })).collect()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stream cut at `at`, as a dropped connection leaves it.
    fn turn_of(stream: &[u8], transport: Option<&str>) -> Turn {
        let mut a = Assembler::default();
        a.push(stream, &mut |_| {});
        a.finish(transport.map(str::to_string))
    }

    const TOOL_STREAM: &[u8] = include_bytes!("../testdata/llama-tool-call.sse");
    const TEXT_STREAM: &[u8] = include_bytes!("../testdata/llama-text.sse");

    #[test]
    fn a_real_tool_call_parses() {
        let t = turn_of(TOOL_STREAM, None);
        assert_eq!(t.finish, Finish::ToolCalls);
        let r = t.runnable().unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].0.name, "read_file");
        assert_eq!(r[0].1.get("path").and_then(|p| p.str()), Some("README.md"));
        assert!(t.usage.unwrap().completion_tokens > 0);
    }

    #[test]
    fn real_text_parses() {
        let t = turn_of(TEXT_STREAM, None);
        assert_eq!(t.finish, Finish::Stop);
        assert!(!t.content.is_empty());
        assert_eq!(t.runnable().unwrap(), []);
    }

    /// The property that matters: wherever a stream is cut, nothing runs, and no cut panics.
    #[test]
    fn a_stream_cut_anywhere_runs_nothing() {
        for stream in [TOOL_STREAM, TEXT_STREAM] {
            assert!(turn_of(stream, None).runnable().is_ok());
            for cut in 0..stream.len() {
                let t = turn_of(&stream[..cut], None);
                assert!(t.runnable().is_err(), "cut at {cut} of {}: {:?}", stream.len(), t.finish);
            }
        }
    }

    /// Every byte of a real stream, spoiled in turn. It may fail or not; it must not panic, and
    /// no call may come out of a stream whose calls did not parse.
    #[test]
    fn a_spoiled_stream_does_not_panic() {
        for at in (0..TOOL_STREAM.len()).step_by(7) {
            for b in [0u8, b'"', b'}', 0xff, b'\n'] {
                let mut s = TOOL_STREAM.to_vec();
                s[at] = b;
                let t = turn_of(&s, None);
                if let Ok(calls) = t.runnable() {
                    assert!(calls.iter().all(|(_, a)| matches!(a, Value::Obj(_))));
                }
            }
        }
    }

    #[test]
    fn a_process_that_died_runs_nothing_even_after_a_finish() {
        let t = turn_of(TOOL_STREAM, Some("curl exited with 18"));
        assert!(matches!(t.finish, Finish::Error(_)));
        assert!(t.runnable().is_err());
    }

    fn chunk(json: &str) -> Vec<u8> {
        format!("data: {json}\n\n").into_bytes()
    }

    fn ended(mut stream: Vec<u8>) -> Vec<u8> {
        stream.extend_from_slice(b"data: [DONE]\n\n");
        stream
    }

    #[test]
    fn a_reason_to_stop_is_not_enough_without_the_end() {
        let s = call_chunk(0, "a", "read_file", "{\"path\":\"x\"}", Some("tool_calls"));
        assert_eq!(turn_of(&s, None).finish, Finish::CutOff);
        assert!(turn_of(&s, None).runnable().is_err());
        assert!(turn_of(&ended(s), None).runnable().is_ok());
    }

    fn call_chunk(index: u64, id: &str, name: &str, args: &str, finish: Option<&str>) -> Vec<u8> {
        let call = obj([
            ("index", n(index)),
            ("id", s(id)),
            ("function", obj([("name", s(name)), ("arguments", s(args))])),
        ]);
        let finish = finish.map(s).unwrap_or(Value::Null);
        chunk(
            &obj([(
                "choices",
                Value::Arr(vec![obj([
                    ("delta", obj([("tool_calls", Value::Arr(vec![call]))])),
                    ("finish_reason", finish),
                ])]),
            )])
            .text(),
        )
    }

    #[test]
    fn calls_from_a_turn_that_did_not_end_well_never_run() {
        for reason in ["length", "content_filter", "error", "something_new"] {
            let t = turn_of(&ended(call_chunk(0, "a", "read_file", "{\"path\":\"x\"}", Some(reason))), None);
            assert_eq!(t.calls.len(), 1);
            assert!(t.runnable().is_err(), "{reason}");
        }
    }

    #[test]
    fn an_error_after_the_stream_began_ends_the_turn() {
        // OpenRouter: the status was already 200, so this is how a failure arrives.
        let mut s = call_chunk(0, "a", "read_file", "{\"path\":\"x\"}", None);
        s.extend(chunk(r#"{"error":{"code":502,"message":"provider down"},"choices":[{"delta":{"content":""},"finish_reason":"error"}]}"#));
        let t = turn_of(&s, None);
        assert_eq!(t.finish, Finish::Error("provider down".into()));
        assert!(t.runnable().is_err());
    }

    #[test]
    fn an_error_inside_a_choice_ends_the_turn() {
        let t = turn_of(&chunk(r#"{"choices":[{"delta":{},"error":{"message":"boom"},"finish_reason":"error"}]}"#), None);
        assert_eq!(t.finish, Finish::Error("boom".into()));
    }

    #[test]
    fn an_error_before_the_stream_shows_its_message() {
        let t = turn_of(br#"{"error":{"code":500,"message":"model not loaded"}}"#, Some("curl exited with 22"));
        assert_eq!(t.finish, Finish::Error("model not loaded".into()));
        let t = turn_of(b"Bad Gateway\n", Some("curl exited with 22"));
        assert_eq!(t.finish, Finish::Error("Bad Gateway".into()));
        let t = turn_of(b"", Some("curl exited with 7"));
        assert_eq!(t.finish, Finish::Error("curl exited with 7".into()));
    }

    #[test]
    fn pieces_of_two_calls_may_interleave_and_arrive_out_of_order() {
        let mut s = call_chunk(1, "b", "two", "{\"x\"", None);
        s.extend(call_chunk(0, "a", "one", "{\"y\"", None));
        s.extend(call_chunk(1, "", "", ":1}", None));
        s.extend(call_chunk(0, "", "", ":2}", Some("tool_calls")));
        let r = turn_of(&ended(s), None).runnable().unwrap();
        assert_eq!(r[0].0.name, "one");
        assert_eq!(r[0].1.get("y").and_then(|v| v.u64()), Some(2));
        assert_eq!(r[1].0.name, "two");
        assert_eq!(r[1].1.get("x").and_then(|v| v.u64()), Some(1));
    }

    #[test]
    fn one_bad_call_stops_them_all() {
        let good = call_chunk(0, "a", "read_file", "{}", None);
        for bad in [
            call_chunk(1, "b", "read_file", "{\"path\":", Some("tool_calls")),
            call_chunk(1, "b", "read_file", "[1]", Some("tool_calls")),
            call_chunk(1, "b", "read_file", "\"x\"", Some("tool_calls")),
            call_chunk(1, "b", "read_file", "{\"a\":1,\"a\":2}", Some("tool_calls")),
            call_chunk(1, "b", "../../bin/sh", "{}", Some("tool_calls")),
            call_chunk(1, "b", "", "{}", Some("tool_calls")),
            call_chunk(1, "", "read_file", "{}", Some("tool_calls")),
            call_chunk(1, "a", "read_file", "{}", Some("tool_calls")),
        ] {
            let mut s = good.clone();
            s.extend(bad);
            assert!(turn_of(&ended(s), None).runnable().is_err());
        }
    }

    #[test]
    fn limits_end_the_turn_with_an_error() {
        let mut s = Vec::new();
        for i in 0..=MAX_CALLS as u64 {
            s.extend(call_chunk(i, "x", "y", "{}", None));
        }
        assert!(matches!(turn_of(&s, None).finish, Finish::Error(_)));
        let piece = "x".repeat(1 << 20);
        let mut s = Vec::new();
        for _ in 0..=8 {
            s.extend(call_chunk(0, "x", "y", &piece, None));
        }
        assert!(matches!(turn_of(&s, None).finish, Finish::Error(_)));
    }

    #[test]
    fn garbage_in_the_stream_ends_the_turn() {
        for g in ["not json", "{", "{\"choices\":", "null", "[1,2,3]", "{\"choices\":[null]}", "{\"choices\":[{\"delta\":{\"tool_calls\":[{}]}}]}"] {
            let t = turn_of(&chunk(g), None);
            assert!(t.runnable().is_err() || t.calls.is_empty(), "{g}");
            assert!(!matches!(t.finish, Finish::ToolCalls), "{g}");
        }
    }

    #[test]
    fn reasoning_is_kept_for_the_next_request() {
        let s = chunk(r#"{"choices":[{"delta":{"reasoning_content":"hm","reasoning_details":[{"type":"x","data":"é"}]},"finish_reason":"stop"}]}"#);
        let t = turn_of(&ended(s), None);
        assert_eq!(t.reasoning, "hm");
        assert_eq!(t.reasoning_details.unwrap().text(), r#"[{"type":"x","data":"é"}]"#);
    }

    fn details_chunk(json: &str) -> Vec<u8> {
        chunk(&format!(r#"{{"choices":[{{"delta":{{"reasoning_details":[{json}]}}}}]}}"#))
    }

    #[test]
    fn reasoning_a_word_at_a_time_goes_back_as_one_block() {
        let mut s = details_chunk(r#"{"type":"reasoning.text","text":"need ","format":"anthropic-claude-v1","index":0,"signature":null}"#);
        s.extend(details_chunk(r#"{"type":"reasoning.text","text":"to read","format":"anthropic-claude-v1","index":0,"signature":null}"#));
        s.extend(details_chunk(r#"{"type":"reasoning.text","text":"","format":"anthropic-claude-v1","index":0,"signature":"sig"}"#));
        // Another block, and one that is not text at all, stay apart.
        s.extend(details_chunk(r#"{"type":"reasoning.text","text":"more","format":"anthropic-claude-v1","index":1}"#));
        s.extend(details_chunk(r#"{"type":"reasoning.encrypted","data":"AAA","index":2}"#));
        s.extend(details_chunk(r#"{"type":"reasoning.encrypted","data":"BBB","index":2}"#));
        s.extend(chunk(r#"{"choices":[{"delta":{},"finish_reason":"stop"}]}"#));
        let t = turn_of(&ended(s), None);
        assert_eq!(
            t.reasoning_details.unwrap().text(),
            concat!(
                r#"[{"type":"reasoning.text","text":"need to read","format":"anthropic-claude-v1","index":0,"signature":"sig"},"#,
                r#"{"type":"reasoning.text","text":"more","format":"anthropic-claude-v1","index":1},"#,
                r#"{"type":"reasoning.encrypted","data":"AAA","index":2},"#,
                r#"{"type":"reasoning.encrypted","data":"BBB","index":2}]"#
            )
        );
    }

    /// The shapes OpenRouter documents, a comment first and the usage chunk repeating the reason.
    #[test]
    fn a_routers_stream_gives_the_turn_who_ran_it_and_what_it_cost() {
        let stream = concat!(
            ": OPENROUTER PROCESSING\n\n",
            r#"data: {"id":"gen-1","provider":"DeepInfra","choices":[{"index":0,"delta":{"role":"assistant","reasoning":"need to read"},"finish_reason":null}]}"#, "\n\n",
            ": OPENROUTER PROCESSING\n\n",
            r#"data: {"id":"gen-1","provider":"DeepInfra","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"read","arguments":""}}]},"finish_reason":null}]}"#, "\n\n",
            r#"data: {"id":"gen-1","provider":"DeepInfra","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"path\":\"a.rs\"}"}}]},"finish_reason":null}]}"#, "\n\n",
            r#"data: {"id":"gen-1","provider":"DeepInfra","choices":[{"index":0,"delta":{"content":""},"finish_reason":"tool_calls","native_finish_reason":"tool_calls"}]}"#, "\n\n",
            r#"data: {"id":"gen-1","provider":"DeepInfra","choices":[{"index":0,"delta":{"content":"","role":"assistant"},"finish_reason":"tool_calls","native_finish_reason":"tool_calls"}],"usage":{"prompt_tokens":812,"completion_tokens":31,"prompt_tokens_details":{"cached_tokens":800,"cache_write_tokens":0},"cost":6e-05,"is_byok":false}}"#, "\n\n",
            "data: [DONE]\n\n"
        );
        let t = turn_of(stream.as_bytes(), None);
        assert_eq!(t.finish, Finish::ToolCalls);
        let calls = t.runnable().unwrap();
        assert_eq!(calls[0].0.name, "read");
        assert_eq!(calls[0].1.get("path").and_then(|p| p.str()), Some("a.rs"));
        assert_eq!(t.reasoning, "need to read");
        let u = t.usage.unwrap();
        assert_eq!((u.prompt_tokens, u.completion_tokens, u.cached_tokens), (812, 31, 800));
        assert_eq!(u.cost, Some(0.00006));
        assert_eq!(u.provider.as_deref(), Some("DeepInfra"));
    }

    #[test]
    fn an_id_and_a_name_sent_with_every_piece_are_not_doubled() {
        let mut s = call_chunk(0, "call_1", "read", "{\"path\"", None);
        s.extend(call_chunk(0, "call_1", "read", ":\"a\"}", Some("tool_calls")));
        let r = turn_of(&ended(s), None).runnable().unwrap();
        assert_eq!(r[0].0.id, "call_1");
        assert_eq!(r[0].0.name, "read");
        assert_eq!(r[0].1.get("path").and_then(|p| p.str()), Some("a"));
    }

    #[test]
    fn an_error_keeps_its_fields_and_the_providers_own_words() {
        let t = turn_of(&chunk(r#"{"error":{"code":429,"message":"Provider returned error","metadata":{"error_type":"rate_limit_exceeded","raw":"slow down"}},"choices":[{"delta":{"content":""},"finish_reason":"error"}]}"#), None);
        assert_eq!(t.finish, Finish::Error("Provider returned error: slow down".into()));
        let e = t.error.unwrap();
        assert_eq!(e.get("code").and_then(|c| c.u64()), Some(429));
        assert_eq!(e.get("metadata").unwrap().get("error_type").and_then(|k| k.str()), Some("rate_limit_exceeded"));
        // Before the stream: a body, with the same fields.
        let t = turn_of(br#"{"error":{"code":404,"message":"No allowed providers"}}"#, Some("curl exited with 22"));
        assert_eq!(t.error.unwrap().get("code").and_then(|c| c.u64()), Some(404));
    }

    fn assistant(pairs: &str) -> Value {
        json::parse(&format!(r#"{{"role":"assistant",{pairs}}}"#)).unwrap()
    }

    #[test]
    fn reasoning_goes_back_only_to_the_model_that_wrote_it() {
        let m = [assistant(r#""content":"a","reasoning_details":[{"type":"x"}],"written_by":"one""#), assistant(r#""content":"b","reasoning":"hm","written_by":"two""#)];
        let to_one = for_model(&m, Some("one"));
        assert_eq!(to_one[0].text(), r#"{"role":"assistant","content":"a","reasoning_details":[{"type":"x"}]}"#);
        assert_eq!(to_one[1].text(), r#"{"role":"assistant","content":"b"}"#);
        // A local model gets none, and nothing else about the message changes.
        for m in for_model(&m, None) {
            assert!(m.get("reasoning").is_none() && m.get("reasoning_details").is_none() && m.get(WRITTEN_BY).is_none());
        }
    }

    #[test]
    fn a_hosted_model_gets_null_beside_a_call_and_no_empty_message() {
        let calls = r#""content":"","tool_calls":[{"id":"1"}]"#;
        let m = [message("user", ""), assistant(calls), assistant(r#""content":"""#), message("tool", "")];
        let hosted = for_model(&m, Some("x"));
        assert_eq!(hosted.len(), 3);
        assert_eq!(hosted[1].get("content"), Some(&Value::Null));
        assert_eq!(hosted[2], message("tool", ""));
        assert_eq!(for_model(&m, None), m);
    }

    #[test]
    fn text_comes_out_as_it_arrives() {
        let mut seen = String::new();
        let mut a = Assembler::default();
        a.push(&chunk(r#"{"choices":[{"delta":{"content":"he"}}]}"#), &mut |d| {
            if let Delta::Content(t) = d {
                seen.push_str(t)
            }
        });
        assert_eq!(seen, "he");
    }

    #[test]
    fn the_request_sends_the_tools_and_asks_for_usage() {
        let t = [Tool { name: "t", description: "d", parameters: r#"{"type":"object"}"# }];
        let r = request("m", &[message("user", "hi\n\"there\"")], &t, 9).unwrap();
        let v = json::parse(&r).unwrap();
        assert_eq!(v.get("stream"), Some(&Value::Bool(true)));
        assert!(v.get("stream_options").unwrap().get("include_usage").is_some());
        assert_eq!(v.get("tools").unwrap().arr().unwrap().len(), 1);
        assert!(!r.contains('\n'), "one line: {r}");
        assert!(request("m", &[], &[Tool { name: "t", description: "d", parameters: "{" }], 9).is_err());
    }
}
