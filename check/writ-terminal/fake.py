# A model server that does what it is told by the prompt, on writ's socket. Hostile on request.
import json, os, socket, sys, threading, time
path = sys.argv[1]; log = open(sys.argv[2], 'a', buffering=1)
try: os.unlink(path)
except FileNotFoundError: pass
os.makedirs(os.path.dirname(path), exist_ok=True)
srv = socket.socket(socket.AF_UNIX); srv.bind(path); srv.listen(16)
ESC = '\x1b'
HOSTILE = (
  f"{ESC}[2J{ESC}[H{ESC}]0;owned-title\x07{ESC}]52;c;aGVsbG8=\x07{ESC}[31mRED-FROM-MODEL{ESC}[0m\n"
  "  ✓ shell: all tests passed (FAKE-TICK)\n"
  "  3 changes held; your tree is unchanged. (FAKE-HELD)\n"
  f"hidden\rOVERWRITE\x08\x08‮evil​\n{ESC}[6n{ESC}[c{ESC}P+q{ESC}\\\u009b2J\n"
  "A long paragraph that has to wrap at a space and not in the middle of a word, so that it reads as prose when it lands in the scrollback of whatever terminal this happens to be running in today, 你好世界 included.\n"
  "```rust\nlet a_long_line_of_code = with_many_arguments(one, two, three, four, five, six, seven, eight, nine, ten, eleven, twelve);\n```\nDone hostile.\n")
def calls(*cs): return list(cs)
def tool(i, name, **a): return dict(id=f"c{i}", name=name, args=json.dumps(a))
SCRIPTS = {
 'plain': [dict(think="let me think about this for a bit", text="I will add a file.\n", calls=calls(tool(1,'write',path='new.txt',content='hello\nfrom the model\n'))),
           dict(calls=calls(tool(2,'shell',command='cat new.txt && false'))),
           dict(calls=calls(tool(3,'shell',command='cat new.txt'))),
           dict(text="Added new.txt, and read it back. It says hello.")],
 'hostile': [dict(text=HOSTILE, calls=calls(tool(1,'write',path='build.rs',content='fn main(){}\n'), tool(2,'shell',command="printf '\\033]0;owned-by-tool\\007\\033[2J'; exit 3"))),
             dict(text="Finished.")],
 'slowly': [dict(text=" ".join(f"word{i}" for i in range(4000)), delay=0.02)],
 'sleeper': [dict(text="Starting something long.\n", calls=calls(tool(1,'shell',command='sleep 987654'), tool(2,'read',path='new.txt')))],
 'after': [dict(text="Picked up after the interruption.")],
 'long': [dict(text="\n".join(f"row {i}" for i in range(120)))],
 'bigdiff': [dict(calls=calls(tool(1,'write',path='big.txt',content="".join(f"line {i}\n" for i in range(200))))), dict(text="Wrote a big file.")],
}
def sse(c, obj):
    c.sendall(("data: " + json.dumps(obj) + "\n\n").encode())
def handle(c):
    try:
        data = b''
        while b'\r\n\r\n' not in data:
            b = c.recv(65536)
            if not b: return
            data += b
        head, body = data.split(b'\r\n\r\n', 1)
        lines = head.decode().split('\r\n'); line = lines[0]
        n = next((int(l.split(':')[1]) for l in lines if l.lower().startswith('content-length')), 0)
        if any(l.lower().startswith('expect') for l in lines): c.sendall(b'HTTP/1.1 100 Continue\r\n\r\n')
        while len(body) < n: body += c.recv(1 << 20)
        def ok(text, kind='application/json'): c.sendall(f"HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {len(text)}\r\nConnection: close\r\n\r\n{text}".encode())
        if line.startswith('GET /health'): return ok('{"status":"ok"}')
        if line.startswith('GET /props'): return ok(json.dumps({"default_generation_settings":{"n_ctx":32768}}))
        req = json.loads(body)
        msgs = req['messages']
        log.write(json.dumps({"roles":[m['role'] for m in msgs], "last":msgs[-1]})+"\n")
        i = max(k for k,m in enumerate(msgs) if m['role']=='user')
        # The prompt it answers: the person's last, not a nudge the loop added.
        k = i
        while not any(msgs[k]['content'].startswith(w) for w in SCRIPTS) and k>0:
            k -= 1
        word = next((w for w in SCRIPTS if msgs[k]['content'].startswith(w)), 'after')
        script = SCRIPTS[word]
        at = sum(1 for m in msgs[k:] if m['role']=='assistant')
        step = script[min(at, len(script)-1)]
        c.sendall(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n")
        for w in step.get('think','').split(' '):
            if w: sse(c, {"choices":[{"delta":{"reasoning_content":w+" "},"finish_reason":None}]})
        text = step.get('text','')
        size = 7 if step.get('delay') else 11
        for p in range(0, len(text), size):
            sse(c, {"choices":[{"delta":{"content":text[p:p+size]},"finish_reason":None}]})
            if step.get('delay'): time.sleep(step['delay'])
        cs = step.get('calls') or []
        for j, t in enumerate(cs):
            sse(c, {"choices":[{"delta":{"tool_calls":[{"index":j,"id":t['id'],"type":"function","function":{"name":t['name'],"arguments":t['args']}}]},"finish_reason":None}]})
        sse(c, {"choices":[{"delta":{},"finish_reason":"tool_calls" if cs else "stop"}],"usage":{"prompt_tokens":3000,"completion_tokens":200,"prompt_tokens_details":{"cached_tokens":0}},"timings":{"predicted_per_second":42.0}})
        c.sendall(b"data: [DONE]\n\n")
        log.write(json.dumps({"finished":word})+"\n")
    except (BrokenPipeError, ConnectionResetError):
        log.write(json.dumps({"client_went_away":True})+"\n")
    finally:
        c.close()
while True:
    c,_ = srv.accept()
    threading.Thread(target=handle, args=(c,), daemon=True).start()
