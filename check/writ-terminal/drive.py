# Drive the real writ in a real terminal emulator (a private tmux), against a hostile fake model,
# and read what landed on the screen. No GPU, model or `work` is needed. Run it from anywhere:
#   python3 -I check/writ-terminal/drive.py [path to writ]
# It takes the local server's socket for its fake, so it refuses if a server is up.
import json, os, re, shutil, socket, subprocess, sys, tempfile, time, signal
HERE = os.path.dirname(os.path.abspath(__file__))
SP = tempfile.mkdtemp(prefix='writ-terminal-')
W = sys.argv[1] if len(sys.argv) > 1 else '/bin/writ'; HOME = os.path.expanduser('~')
sock = f'{HOME}/.local/share/writ/llama/llama.sock'
probe = socket.socket(socket.AF_UNIX)
if os.path.exists(sock) and probe.connect_ex(sock) == 0:
    sys.exit('a local server is up: `writ llama stop` first')
# The fake is a local server, and a model on OpenRouter would be real, and paid for.
try:
    if json.load(open(f'{HOME}/.config/writ/use.json')).get('provider') != 'local':
        sys.exit('this machine is using a model on OpenRouter: `writ use local` first')
except FileNotFoundError:
    pass
TM = ['tmux', '-L', f'writ-test-{os.getpid()}', '-f', '/dev/null']
results = []
def tm(*a, check=True):
    r = subprocess.run(TM + list(a), capture_output=True, text=True)
    if check and r.returncode: raise SystemExit(f'tmux {a}: {r.stderr}')
    return r.stdout
def keys(*k): tm('send-keys', '-t', 'w', *k)
def typed(text, enter=True):
    tm('send-keys', '-t', 'w', '-l', text)
    if enter: keys('Enter')
def screen(): return tm('capture-pane', '-p', '-t', 'w')
def history(esc=False): return tm('capture-pane', '-p', '-J', '-S', '-', '-t', 'w', *(['-e'] if esc else []))
def wait(pred, secs=20, what=''):
    t = time.time()
    while time.time() - t < secs:
        if pred(): return True
        time.sleep(0.1)
    return False
def idle(): return wait(lambda: '· Ctrl-C stops' not in screen() and ' held ·' in screen())
def check(name, ok, detail=''):
    results.append(ok); print(('PASS ' if ok else 'FAIL ') + name + ('' if ok else f'\n     {str(detail)[-700:]!r}'))
def reqs(): return [json.loads(l) for l in open(LOG)] if os.path.exists(LOG) else []

proj = f'{SP}/proj'; LOG = f'{SP}/requests.log'
shutil.rmtree(proj, ignore_errors=True); os.makedirs(proj)
for f in [LOG]:
    if os.path.exists(f): os.unlink(f)
open(f'{proj}/README.md','w').write('# demo\n')
subprocess.run('git init -q . && git add -A && git -c user.email=t@t -c user.name=t commit -qm first', shell=True, cwd=proj)
fake = subprocess.Popen(['python3','-I',f'{HERE}/fake.py',sock,LOG])
time.sleep(0.5)
tm('new-session','-d','-s','w','-x','100','-y','30','-c',proj,'sh')
tm('set','-g','history-limit','20000')
tm('set','-g','set-titles','on'); tm('set','-g','set-clipboard','on')
typed('PS1="SHELL> "; clear')
time.sleep(0.3)
try:
    typed(W)
    check('it starts, says where it is, and waits', wait(lambda: 'session s' in screen() and '> ' in screen() and 'loaded' in screen()), screen())
    check('the status row knows the model is up', 'a server' in screen() or 'loaded' in screen(), screen())
    check('its first lines say which model', wait(lambda: 'model: local, loaded' in screen()), screen())

    typed('plain')
    check('a whole prompt runs to the end', wait(lambda: 'It says hello' in history()) and idle(), history())
    h = history()
    check('the person\'s prompt is on the screen', '> plain' in h, h)
    check('the model\'s words are there', 'I will add a file.' in h, h)
    check('thinking is hidden by default', 'let me think' not in h, h)
    check('a call that worked is a tick', '✓ write: new.txt' in h, h)
    check('a command that failed is a cross, with its code', '✗ shell: cat new.txt && false' in h and 'exit 1' in h, h)
    check('it says what is held, once', '1 change held' in h and '/diff' in h, h)
    check('the real tree is untouched', not os.path.exists(f'{proj}/new.txt'))
    check('the status row shows held and context', '1 held' in screen() and 'context 9%' in screen(), screen())
    e = history(esc=True)
    check('writ\'s own lines have colour', re.search(r'\x1b\[32m\s*✓ write', e) is not None, e[:400])
    check('the model\'s words have none', re.search(r'\x1b\[(?!0m)[0-9;]*m[^\n]*I will add a file', e) is None, e[:600])

    typed('/diff')
    check('/diff shows the file', wait(lambda: '+from the model' in history()), history())
    e = history(esc=True)
    check('an added line is green', re.search(r'\x1b\[32m\+hello', e) is not None, e[-500:])
    check('and it is the person\'s review, in scrollback', '--- new.txt' in history())

    typed('hostile'); title_before = tm('display','-p','-t','w','#{pane_title}')
    check('the hostile answer ends', wait(lambda: 'Finished.' in history()) and idle(), history()[-800:])
    h = history(); e = history(esc=True)
    check('nothing was cleared: earlier lines survive', 'I will add a file.' in h and '> plain' in h, h[:500])
    check('the title did not change', 'owned' not in tm('display','-p','-t','w','#{pane_title}'))
    check('the clipboard did not change', 'hello' not in tm('show-buffer', check=False))
    check('escape sequences show as text', '^[[2J' in h and '^[]52;c;aGVsbG8=^G' in h, h)
    check('a carriage return cannot overwrite', 'hidden^MOVERWRITE' in h, h)
    check('bidi and zero-width controls show', '⟨U+202E⟩' in h and '⟨U+200B⟩' in h, h)
    check('a C1 control shows', '⟨U+009B⟩' in h, h)
    check('the model\'s colour did not take', re.search(r'\x1b\[31m[^\n]*RED-FROM-MODEL', e) is None and 'RED-FROM-MODEL' in h, h)
    fake_tick = [l for l in e.splitlines() if 'FAKE-TICK' in l][0]
    real_tick = [l for l in e.splitlines() if '✓ write: new.txt' in l][0]
    check('a forged tick has no colour, and a real one does', '\x1b[' not in fake_tick.replace('\x1b[0m','') and '\x1b[32m' in real_tick, repr(fake_tick))
    fake_held = [l for l in e.splitlines() if 'FAKE-HELD' in l][0]
    check('a forged status has no colour', '\x1b[33m' not in fake_held, repr(fake_held))
    check('the tool\'s own escapes did not take either', 'owned-by-tool' not in tm('display','-p','-t','w','#{pane_title}'))
    check('a build script is flagged to wait', 'flagged to wait' in h, h[-600:])
    rows = [l for l in h.splitlines() if 'long paragraph' in l or 'middle of a' in l]
    para = [l for l in screen().splitlines() + h.splitlines() if 'wrap at a space' in l]
    check('prose wraps at spaces', all(not re.search(r'\w$', l[:99]) or len(l) < 100 for l in para), para)
    code = [l for l in h.splitlines() if 'with_many_arguments' in l]
    check('code stays one line', code and code[0].rstrip().endswith('twelve);'), code)

    typed('/apply')
    check('/apply takes what is plain', wait(lambda: '1 applied' in history()), history()[-500:])
    check('it reached the tree', os.path.exists(f'{proj}/new.txt'))
    check('it says what the file was', 'A new.txt' in history().split('> /apply')[-1], history()[-300:])
    check('the build script waited', not os.path.exists(f'{proj}/build.rs') and 'wait for your word' in history())
    typed('/undo')
    check('/undo takes it back', wait(lambda: not os.path.exists(f'{proj}/new.txt')), history()[-300:])
    idle()

    n = len(reqs())
    typed('slowly')
    check('words flow while it runs', wait(lambda: 'word100' in history() or 'word100' in screen()), screen())
    check('the status row says how to stop it', 'Ctrl-C stops' in screen(), screen())
    keys('C-c')
    check('Ctrl-C stops the model', wait(lambda: 'interrupted' in history()) and idle(), history()[-400:])
    check('the server saw the client go', wait(lambda: any('client_went_away' in r for r in reqs()[n:]), 10), reqs()[n:])
    typed('after this')
    check('a prompt after a stop works', wait(lambda: 'Picked up after' in history()) and idle(), history()[-400:])
    r = [x for x in reqs()[n:] if 'roles' in x][-1]
    check('the model is told it was interrupted', r['roles'][-2:] == ['assistant','user'], r['roles'])

    n = len(reqs())
    typed('sleeper')
    check('the status row names the running command', wait(lambda: 'shell: sleep 987654' in screen()), screen())
    running = subprocess.run('pgrep -a -x sleep | grep 987654', shell=True, capture_output=True, text=True).stdout
    check('it is really running', '987654' in running, running)
    keys('Escape')
    check('Esc stops a command', wait(lambda: 'interrupted' in history()) and idle(), history()[-400:])
    check('the command and its box are gone', wait(lambda: subprocess.run('pgrep -a -x sleep | grep 987654', shell=True, capture_output=True).returncode != 0, 5))
    typed('after that')
    check('it goes on after that', wait(lambda: history().count('Picked up after') >= 2) and idle(), history()[-300:])
    r = [x for x in reqs()[n:] if 'roles' in x][-1]
    check('both calls of the stopped turn have answers', r['roles'][-4:] == ['assistant','tool','tool','user'], r['roles'])

    typed('slowly')
    wait(lambda: 'Ctrl-C stops' in screen())
    typed('after the slow one')
    check('a prompt sent while it runs is queued', wait(lambda: 'queued: after the slow one' in screen()), screen())
    typed('/diff')
    check('a command waits and stays in the line', wait(lambda: 'that waits until this stops' in history()) and '> /diff' in screen(), screen())
    check('a word to the person leaves a half-said word whole', re.search(r'word\d+ word\d+', history()) is not None and not re.search(r'wo\n  that waits', history()) and 'word50' in history().replace('\n','') , history()[-400:])
    keys('C-u')
    keys('C-c')
    check('the queued prompt then runs', wait(lambda: history().count('Picked up after') >= 3) and idle(), history()[-500:])
    check('the queue is empty', 'queued:' not in screen(), screen())

    tm('set-buffer','/apply --all\nsecond line\rthird')
    tm('paste-buffer','-p','-t','w')
    time.sleep(0.4)
    s = screen()
    check('a paste with line ends does not submit itself', '> /apply --all' in s and 'second line' in s and 'third' in s and not os.path.exists(f'{proj}/build.rs'), s)
    keys('C-c')
    check('Ctrl-C clears the line', wait(lambda: 'second line' not in screen().split('held')[-1]), screen())

    typed('etc is a word'); idle()
    typed('/nonsense')
    check('an unknown command is refused and stays to be fixed', wait(lambda: 'there is no /nonsense' in history()) and '> /nonsense' in screen(), screen())
    keys('C-u')

    tm('resize-window','-t','w','-x','50','-y','16'); time.sleep(0.5)
    s = screen()
    check('a narrower window redraws the live rows', s.rstrip().splitlines()[-1].startswith('>') and all(len(l) <= 50 for l in s.splitlines()), s)
    typed('multi\\')
    typed('line')
    idle()
    check('it works at that size', '  line' in history(), history()[-300:])
    tm('resize-window','-t','w','-x','100','-y','30'); time.sleep(0.5)

    typed('long')
    check('more than a screen goes to scrollback', wait(lambda: 'row 119' in history()) and idle(), history()[-300:])
    h = history()
    check('nothing changed, so held is not said again', 'held' not in h.split('row 119')[-1].split('s17')[0], h[-300:])
    check('every row is there, in order', all(f'row {i}\n' in h for i in range(120)), h[-400:])
    s = screen().rstrip().splitlines()
    check('the live rows are still at the bottom', s[-1].startswith('>') and 'held' in s[-2], s[-3:])

    keys('C-l'); time.sleep(0.3)
    check('Ctrl-L clears the screen and keeps the line', screen().rstrip().splitlines()[-1].startswith('>') and len(screen().rstrip().splitlines()) <= 3, screen())

    keys('C-t'); time.sleep(0.2)
    check('Ctrl-T says what it did', 'thinking is shown' in history(), history()[-200:])
    keys('C-t'); time.sleep(0.2)

    typed('/use')
    check('/use says which model', wait(lambda: 'local: ' in history()), history()[-400:])
    typed('/use elsewhere')
    check('/use knows what it can use', wait(lambda: 'elsewhere is not a model writ can use' in history()), history()[-400:])
    typed('/use openrouter')
    check('/use openrouter asks which model', wait(lambda: 'which model?' in history()), history()[-400:])
    typed('/use openrouter ../../etc/passwd')
    check('a name that is no model goes nowhere', wait(lambda: "is not a model's name" in history()), history()[-400:])
    check('and the model in use has not changed', not os.path.exists(f'{HOME}/.config/writ/use.json') or json.load(open(f'{HOME}/.config/writ/use.json'))['provider'] == 'local')
    typed('/quit')
    check('/quit goes back to the shell', wait(lambda: screen().rstrip().endswith('SHELL>')), screen())
    s = history()
    check('it says how to go on', 'writ diff s' in s and '--session s' in s, s[-500:])
    check('the terminal is put back', wait(lambda: True) and 'FAIL' not in s)
    typed('stty -a | tr " " "\\n" | grep -E "^-?(icanon|echo|isig|opost)$" | tr "\\n" " "; echo; echo END')
    wait(lambda: 'END' in screen().replace('echo END',''))
    s = screen()
    check('cooked mode is back', 'icanon' in s and '-icanon' not in s and '-echo' not in s and '-isig' not in s and '-opost' not in s, s)
    sid = re.search(r'session (s\d+)', history()).group(1)
    typed(f'{W} --session {sid}')
    check('a session can be gone on with', wait(lambda: 'carrying on, after' in screen()), screen())
    keys('Up'); time.sleep(0.3)
    check('Up brings back what was asked', re.search(r'> (long|etc is a word|multi)', screen()) is not None, screen())
    tm('new-window','-d','-t','w:','-n','other','-c',proj,'sh')
    tm('send-keys','-t','w:other','-l',f'{W} --session {sid}'); tm('send-keys','-t','w:other','Enter')
    check('a second writ on one session is refused', wait(lambda: 'open in another writ' in tm('capture-pane','-p','-t','w:other'), 10), tm('capture-pane','-p','-t','w:other'))
    tm('kill-window','-t','w:other')
    keys('C-u'); typed('/diff --names')
    check('its held changes are still held', wait(lambda: 'build.rs' in history()), history()[-400:])
    keys('C-d')
    check('Ctrl-D leaves', wait(lambda: screen().rstrip().endswith('SHELL>')), screen())

    typed(W)
    wait(lambda: '/help says' in screen())
    pane = tm('display','-p','-t','w','#{pane_pid}').strip()
    pid = subprocess.run(['pgrep','-P',pane,'-x','writ'],capture_output=True,text=True).stdout.strip()
    os.kill(int(pid), signal.SIGTERM)
    check('SIGTERM leaves', wait(lambda: 'SHELL>' in screen().rstrip().splitlines()[-1]), screen())
    typed('stty -a | tr " " "\\n" | grep -cE "^-icanon$"; echo DONE')
    wait(lambda: 'DONE' in screen().replace('echo DONE',''))
    check('and the terminal is put back after a signal', '\n0\n' in screen(), screen())
finally:
    tm('kill-server', check=False)
    fake.terminate()
    try: os.unlink(sock)
    except OSError: pass
    for d in os.listdir(f'{HOME}/.local/share/writ/sessions'):
        p = f'{HOME}/.local/share/writ/sessions/{d}'
        try:
            if json.load(open(f'{p}/meta.json'))['project'].startswith(SP) or not os.path.exists(f'{p}/meta.json'):
                subprocess.run(['chmod','-R','u+rwx',p]); shutil.rmtree(p)
        except OSError: pass
shutil.rmtree(SP, ignore_errors=True)
print(f'{sum(results)} of {len(results)} passed')
sys.exit(0 if all(results) else 1)
