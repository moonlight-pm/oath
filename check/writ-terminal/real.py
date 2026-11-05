# The real writ, a real model and a real server as work, in a private tmux. It needs a model, and
# it leaves the server stopped. `gpu` also checks the model is on the card:
#   python3 -I check/writ-terminal/real.py /bin/writ cpu|gpu
# The model decides how the last task goes, so a failure there is about the model as much as writ.
import json, os, re, subprocess, sys, tempfile, time, shutil
SP = tempfile.mkdtemp(prefix='writ-terminal-')
W = sys.argv[1]; tag = sys.argv[2]
TM = ['tmux','-L',f'writ-real-{os.getpid()}','-f','/dev/null']
results=[]
def tm(*a, check=True):
    r=subprocess.run(TM+list(a),capture_output=True,text=True)
    if check and r.returncode: raise SystemExit(f'tmux {a}: {r.stderr}')
    return r.stdout
def keys(*k): tm('send-keys','-t','w',*k)
def typed(t,enter=True):
    tm('send-keys','-t','w','-l',t)
    if enter: keys('Enter')
def screen(): return tm('capture-pane','-p','-t','w')
def history(): return tm('capture-pane','-p','-J','-S','-','-t','w')
def wait(pred,secs=300):
    t=time.time()
    while time.time()-t<secs:
        if pred(): return True
        time.sleep(0.25)
    return False
def idle(secs=300): return wait(lambda: '· Ctrl-C stops' not in screen() and ' held ·' in screen(), secs)
def check(n,ok,d=''):
    results.append(ok); print(('PASS ' if ok else 'FAIL ')+n+('' if ok else f'\n     {str(d)[-900:]!r}'),flush=True)
def work(): return subprocess.run('pgrep -u 2 -a | cut -c1-70',shell=True,capture_output=True,text=True).stdout.strip()
proj=f'{SP}/rproj'; shutil.rmtree(proj,ignore_errors=True); os.makedirs(f'{proj}/src')
open(f'{proj}/Cargo.toml','w').write('[package]\nname = "demo"\nversion = "0.3.1"\nedition = "2021"\n')
open(f'{proj}/src/lib.rs','w').write('pub fn add(a: i32, b: i32) -> i32 {\n    a - b\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn adds() {\n        assert_eq!(super::add(2, 3), 5);\n    }\n}\n')
open(f'{proj}/.gitignore','w').write('target/\nCargo.lock\n')
subprocess.run('git init -q . && git add -A && git -c user.email=t@t -c user.name=t commit -qm first',shell=True,cwd=proj)
tm('new-session','-d','-s','w','-x','110','-y','32','-c',proj,'sh')
tm('set','-g','history-limit','50000')
typed('PS1="SHELL> "; clear'); time.sleep(0.3)
try:
    typed(W)
    check('it starts with the model not loaded', wait(lambda: 'not loaded' in screen(),20), screen())
    check('nothing runs as work yet', work()=='', work())
    typed('Reply with the single word: ready')
    check('the status row says it is loading', wait(lambda: 'loading the model' in screen() or 'thinking' in screen(),30), screen())
    check('the model loaded and answered', wait(lambda: re.search(r'(?i)^ready', history(), re.M) is not None) and idle(), history()[-600:])
    h=history()
    check('it said what it loaded', 'loading ' in h and 'loaded in ' in h, h[-600:])
    check('the server is work', '/bin/llama-server' in work() or 'llama-server' in work(), work())
    check('the status row shows it loaded', 'loaded:' in screen(), screen())

    typed('Write a very long and detailed history of the Unix operating system, at least 2000 words.')
    wait(lambda: '· Ctrl-C stops' in screen(),10)
    wait(lambda: 'thinking' in screen() and ' tokens' in screen(),60)
    time.sleep(3)
    keys('C-c')
    check('Ctrl-C stops it in the middle', wait(lambda: 'interrupted' in history(),30) and idle(30), history()[-500:])
    p=subprocess.run(['pgrep','-x','llama-server'],capture_output=True,text=True).stdout.strip()
    typed('Reply with the single word: again')
    # What matters here is that its template takes the conversation. A small model may say anything.
    time.sleep(1)
    check('the model takes a prompt after being cut off', idle(180), history()[-700:])
    check('with no error from its template', 'stopped:' not in history(), history()[-700:])

    typed('Use the shell tool to run exactly this command and nothing else: sleep 987653')
    check('it runs the command', wait(lambda: 'shell: sleep 987653' in screen(),300), history()[-500:])
    keys('Escape')
    check('Esc stops the command', wait(lambda: history().count('interrupted')>=2,20) and idle(30), history()[-500:])
    check('it is gone', wait(lambda: subprocess.run('pgrep -a -x sleep | grep 987653',shell=True,capture_output=True).returncode!=0,5))
    typed('That command was stopped on purpose, and that is fine. Reply with the single word: understood')
    time.sleep(1)
    check('the model takes a prompt after a stopped tool', idle(180) and 'stopped:' not in history(), history()[-700:])

    typed('/new')
    wait(lambda: '> /new' in history(),5); idle(20)
    typed('cargo test fails in this project. Find out why, fix the bug, run the tests again, and tell me in one sentence what you changed.')
    time.sleep(1)
    limit = 240 if tag=='cpu' else 600
    ended = idle(limit)
    print(f'     the task {"ended" if ended else "did not end in time"}: {len(history().split("cargo test fails")[-1].splitlines())} rows', flush=True)
    if not ended:
        # The model's failure, not writ's: stop it, and go on to what writ does.
        keys('C-c'); idle(30)
    elif tag=='cpu':
        print('     (a model this small may not fix it: what it did is not checked here)', flush=True)
    if ended and tag!='cpu':
        typed('/diff')
        wait(lambda: 'src/lib.rs' in history().split('> /diff')[-1],10); time.sleep(0.5)
        d=history().split('> /diff')[-1]
        check('the review shows the fix', ('a + b' in d) and 'a - b' in d, d[-500:])
        check('the real tree is still as it was', 'a - b' in open(f'{proj}/src/lib.rs').read())
        idle(20)
        typed('/apply')
        check('/apply takes it', wait(lambda: 'applied' in history().split('> /apply')[-1],15), history()[-300:])
        check('the real tree has the fix', 'a + b' in open(f'{proj}/src/lib.rs').read())
        idle(20)
    if tag!='cpu':
        o=subprocess.run('nvidia-smi --query-compute-apps=pid,used_memory --format=csv,noheader',shell=True,capture_output=True,text=True).stdout
        pid=subprocess.run(['pgrep','-x','llama-server'],capture_output=True,text=True).stdout.strip()
        check('the model is on the GPU', pid and pid+',' in o, o)
    typed('/llama stop')
    check('/llama stop stops it', wait(lambda: 'stopped' in history().split('> /llama')[-1],20) and wait(lambda: work()=='',10), work())
    idle(20)
    check('the status row knows', 'not loaded' in screen(), screen())
    typed('/use local')
    check('/use local loads it again', wait(lambda: 'local: loaded' in history().split('> /use')[-1]),history()[-400:])
    idle(20)
    typed('/quit')
    check('it leaves', wait(lambda: screen().rstrip().endswith('SHELL>'),15), screen())
    check('it says the model stays loaded', 'stays loaded' in history(), history()[-300:])
    check('and it does', 'llama-server' in work(), work())
    typed(f'{W} llama stop')
    check('and it stops', wait(lambda: work()=='',20), work())
finally:
    subprocess.run([W,'llama','stop'],capture_output=True)
    keep = os.environ.get('WRIT_KEEP_SCREEN')
    if keep:
        open(keep,'w').write(history())
    tm('kill-server',check=False)
shutil.rmtree(SP, ignore_errors=True)
print(f'{sum(results)} of {len(results)} passed',flush=True)
sys.exit(0 if all(results) else 1)
