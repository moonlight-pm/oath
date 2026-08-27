#!/usr/bin/env python3
"""Boot the QEMU appliance, drive the courage test, write a run report.

Creates build/runs/<id>/ with serial.log, events.jsonl, probe.json, REPORT.md.
Uses a qcow overlay so the golden image is not mutated.
"""
from __future__ import annotations

import hashlib
import json
import os
import re
import select
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = Path(os.environ.get("OATH_BUILD", ROOT / "build"))


def sha256(p: Path) -> str | None:
    try:
        h = hashlib.sha256()
        with p.open("rb") as f:
            for chunk in iter(lambda: f.read(1 << 20), b""):
                h.update(chunk)
        return h.hexdigest()
    except OSError:
        return None


def which_qemu() -> str:
    q = os.environ.get("QEMU", "qemu-system-x86_64")
    if shutil.which(q):
        return q
    tools = subprocess.check_output(
        ["nix-build", str(ROOT / "image/tools.nix"), "--no-out-link"], text=True
    ).strip()
    os.environ["PATH"] = str(Path(tools) / "bin") + ":" + os.environ.get("PATH", "")
    if shutil.which("qemu-system-x86_64"):
        return "qemu-system-x86_64"
    sys.exit("qemu-system-x86_64 not on PATH")


class SerialVM:
    def __init__(self, run: Path, kernel: Path, initrd: Path, disk: Path, qemu: str, label: str):
        self.run = run
        self.buf = ""
        accel = ["-enable-kvm"] if os.access("/dev/kvm", os.R_OK) else []
        serial = run / f"serial-{label}.log"
        qlog = run / f"qemu-{label}.log"
        cmd = [
            qemu,
            "-machine",
            "q35",
            *accel,
            "-m",
            "512",
            "-display",
            "none",
            "-monitor",
            "none",
            "-chardev",
            f"stdio,id=cons,logfile={serial},signal=off",
            "-serial",
            "chardev:cons",
            "-kernel",
            str(kernel),
            "-initrd",
            str(initrd),
            "-append",
            "console=ttyS0 panic=10",
            "-drive",
            f"file={disk},if=virtio,format=qcow2,cache=writeback",
            "-d",
            "guest_errors",
            "-D",
            str(qlog),
            "-no-reboot",
        ]
        (run / "qemu.cmd").write_text(" ".join(cmd) + "\n")
        self.proc = subprocess.Popen(
            cmd,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            bufsize=0,
        )

    def _read_some(self, timeout: float) -> str:
        if self.proc.stdout is None:
            return ""
        r, _, _ = select.select([self.proc.stdout], [], [], timeout)
        if not r:
            return ""
        data = self.proc.stdout.read(4096)
        if not data:
            return ""
        text = data.decode("utf-8", "replace")
        self.buf += text
        return text

    def wait_for(self, pattern: str, timeout: float) -> bool:
        deadline = time.time() + timeout
        cre = re.compile(pattern)
        while time.time() < deadline:
            if cre.search(self.buf):
                return True
            if self.proc.poll() is not None:
                return cre.search(self.buf) is not None
            self._read_some(min(0.2, deadline - time.time()))
        return cre.search(self.buf) is not None

    def drain(self, seconds: float) -> None:
        deadline = time.time() + seconds
        while time.time() < deadline:
            if not self._read_some(0.05):
                if self.proc.poll() is not None:
                    return

    def send(self, line: str) -> None:
        if self.proc.stdin is None:
            return
        self.proc.stdin.write((line + "\n").encode())
        self.proc.stdin.flush()

    def snapshot_tail(self) -> str:
        return self.buf[-4000:]

    def close(self, kill_after: float = 2.0) -> int | None:
        if self.proc.poll() is None:
            try:
                self.proc.send_signal(signal.SIGTERM)
            except OSError:
                pass
            try:
                self.proc.wait(timeout=kill_after)
            except subprocess.TimeoutExpired:
                self.proc.kill()
        return self.proc.returncode

    def wait_exit(self, timeout: float) -> int | None:
        try:
            return self.proc.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            return None


def extract_events(serial: str) -> list[dict]:
    out = []
    for line in serial.splitlines():
        if "oath-tel " in line:
            payload = line.split("oath-tel ", 1)[1].strip()
            try:
                out.append(json.loads(payload))
            except json.JSONDecodeError:
                out.append({"raw": payload})
    return out


def main() -> int:
    kernel = Path(os.environ.get("OATH_KERNEL", OUT / "bzImage"))
    backing = Path(os.environ.get("OATH_IMAGE", OUT / "oath.qcow2"))
    initrd = OUT / "initrd.gz"
    for p in (kernel, backing, initrd):
        if not p.is_file():
            print(f"missing {p} — run image/build.sh first", file=sys.stderr)
            return 2

    qemu = which_qemu()
    qemu_img = shutil.which("qemu-img") or "qemu-img"
    rid = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime()) + "-probe"
    run = OUT / "runs" / rid
    run.mkdir(parents=True, exist_ok=True)
    overlay = run / "disk.qcow2"
    subprocess.check_call(
        [qemu_img, "create", "-f", "qcow2", "-F", "qcow2", "-b", str(backing.resolve()), str(overlay)],
        stdout=subprocess.DEVNULL,
    )
    meta = {
        "started": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "kind": "probe",
        "qemu": qemu,
        "kvm": os.access("/dev/kvm", os.R_OK),
        "kernel": str(kernel),
        "initrd": str(initrd),
        "backing": str(backing),
        "overlay": str(overlay),
        "sha256": {
            "kernel": sha256(kernel),
            "initrd": sha256(initrd),
            "backing": sha256(backing),
        },
    }
    (run / "meta.json").write_text(json.dumps(meta, indent=2) + "\n")
    print(f"run: {run}", file=sys.stderr)

    steps: list[dict] = []

    def step(name: str, ok: bool, detail: str = "") -> None:
        rec = {"name": name, "ok": bool(ok), "detail": detail[:2000]}
        steps.append(rec)
        mark = "ok" if ok else "FAIL"
        print(f"  [{mark}] {name}" + (f" — {detail[:120]}" if detail and not ok else ""), file=sys.stderr)

    def boot(label: str) -> SerialVM:
        vm = SerialVM(run, kernel, initrd, overlay, qemu, label)
        # First boot also writes to serial.log via qemu logfile; process stdout is the same stream.
        ready = vm.wait_for(r"oath-init: ready|oath-tel .*ready", timeout=45)
        step(f"{label}.ready", ready, "did not see init ready" if not ready else "")
        prompt = vm.wait_for(r"~ #", timeout=15)
        step(f"{label}.prompt", prompt, "no serial prompt" if not prompt else "")
        return vm

    def cmd(vm: SerialVM, line: str, expect: str | None, name: str, timeout: float = 8.0) -> str:
        before = len(vm.buf)
        vm.send(line)
        deadline = time.time() + timeout
        while time.time() < deadline:
            vm._read_some(0.15)
            chunk = vm.buf[before:]
            if "~ #" in chunk[len(line) :]:
                break
            if vm.proc.poll() is not None:
                break
        chunk = vm.buf[before:]
        ok = True
        detail = ""
        if expect is not None and expect not in chunk:
            ok = False
            detail = f"expected {expect!r} in:\n{chunk[-800:]}"
        step(name, ok, detail)
        return chunk

    # --- boot 1 ---
    vm = boot("boot1")
    cmd(vm, "oath ls", "host:local", "ls.host")
    cmd(vm, "oath get host:local", 'hostname: "oath"', "get.initial")
    cmd(vm, "oath set host:local hostname=atlas", None, "set.atlas")
    cmd(vm, "oath apply", "applied generation", "apply.atlas")
    cmd(vm, "hostname", "atlas", "hostname.atlas")
    cmd(vm, "oath undo", "undid to generation", "undo")
    cmd(vm, "hostname", "oath", "hostname.undone")
    cmd(vm, "oath set host:local hostname=atlas", None, "set.atlas2")
    cmd(vm, "oath apply", "applied generation", "apply.atlas2")
    cmd(vm, "oath set host:local power=reboot", None, "set.reboot")
    cmd(vm, "oath apply", "confirm required", "apply.reboot.noconfirm")
    # confirmed reboot should make qemu exit (-no-reboot)
    before = len(vm.buf)
    vm.send("oath apply --confirm")
    rc = vm.wait_exit(timeout=20)
    vm._read_some(0.2)
    chunk = vm.buf[before:]
    step("apply.reboot.confirm_exits", rc is not None, f"qemu still running rc={rc} tail={chunk[-400:]}")
    if rc is None:
        vm.close()

    # --- boot 2: same overlay, hostname should still be atlas ---
    vm2 = boot("boot2")
    cmd(vm2, "hostname", "atlas", "reboot.hostname")
    cmd(vm2, "oath get host:local", 'hostname: "atlas"', "reboot.get")
    vm2.close()

    serial_parts = []
    for p in sorted(run.glob("serial-*.log")):
        serial_parts.append(p.read_text(errors="replace"))
    serial = "\n".join(serial_parts) if serial_parts else vm2.buf
    events = extract_events(serial)
    (run / "serial.log").write_text(serial)
    (run / "events.jsonl").write_text("".join(json.dumps(e) + "\n" for e in events))

    failed = [s for s in steps if not s["ok"]]
    probe = {
        "run": str(run),
        "ok": not failed,
        "failed": [s["name"] for s in failed],
        "steps": steps,
        "events": len(events),
    }
    (run / "probe.json").write_text(json.dumps(probe, indent=2) + "\n")

    lines = [
        f"# Probe {rid}",
        "",
        f"**ok:** {probe['ok']}",
        f"**failed:** {', '.join(probe['failed']) or 'none'}",
        f"**events:** {len(events)} `oath-tel` lines",
        "",
        "| step | ok |",
        "|------|----|",
    ]
    for s in steps:
        lines.append(f"| `{s['name']}` | {'yes' if s['ok'] else 'NO'} |")
    lines.append("")
    if failed:
        lines.append("## Failures")
        for s in failed:
            lines.append(f"### {s['name']}")
            lines.append("```")
            lines.append(s["detail"] or "(no detail)")
            lines.append("```")
            lines.append("")
    (run / "REPORT.md").write_text("\n".join(lines))
    print(f"report: {run / 'REPORT.md'}", file=sys.stderr)
    print(json.dumps({"ok": probe["ok"], "failed": probe["failed"], "run": str(run)}))
    return 0 if probe["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
