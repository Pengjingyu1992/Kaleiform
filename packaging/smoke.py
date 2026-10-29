#!/usr/bin/env python3
"""Exercise installed Kaleiform binaries on Windows and Linux."""
import argparse
import json
import os
from pathlib import Path
import socket
import struct
import subprocess
import sys
import tempfile
import time


def png(path, expected=None):
    data = path.read_bytes()
    assert data[:8] == b"\x89PNG\r\n\x1a\n" and len(data) > 100, path
    size = struct.unpack(">II", data[16:24])
    assert min(size) > 0 and (expected is None or size == expected), size


def run(bindir, desktop, work):
    suffix = ".exe" if os.name == "nt" else ""
    cli = bindir / f"vectorcraft-cli{suffix}"
    app = bindir / f"vectorcraft{suffix}"
    native = work / "smoke.vectorcraft"
    svg, raster, pdf = (work / f"smoke.{ext}" for ext in ("svg", "png", "pdf"))
    subprocess.run([
        str(cli), "run", "--cmd", "file.new", "--params", '{"width":240,"height":120}',
        "--cmd", "shape.rectangle", "--params", '{"x":10,"y":10,"width":100,"height":50}',
        "--cmd", "paint.setFill", "--params", '{"color":"#3366ff"}',
        "--export", str(native), "--export", str(svg), "--export", str(raster), "--export", str(pdf),
    ], check=True, stdout=subprocess.DEVNULL)
    assert "3366ff" in svg.read_text(), "SVG lost the rectangle's fill"
    assert pdf.read_bytes().startswith(b"%PDF-"), "Invalid PDF export"
    png(raster, (240, 120))
    reopened = subprocess.check_output([str(cli), "run", "--in", str(native), "--cmd", "document.inspect"], text=True)
    info = json.loads(reopened.splitlines()[-1])
    assert info["result"]["artboards"][0]["width"] == 240, info
    print("CLI: draw, SVG/PNG/PDF export and native document reopen passed", flush=True)
    if not desktop:
        return
    if sys.platform == "linux" and not (os.environ.get("DISPLAY") or os.environ.get("WAYLAND_DISPLAY")):
        raise RuntimeError("--desktop needs a display; use xvfb-run or a Wayland compositor")
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        port = reservation.getsockname()[1]
    env = dict(os.environ, VECTORCRAFT_NO_PREFS="1")
    with (work / "desktop.log").open("w+") as log:
        proc = subprocess.Popen([str(app), "--control", str(port), str(native)], env=env, stdout=log, stderr=log)
        try:
            deadline = time.monotonic() + 45
            while True:
                if proc.poll() is not None:
                    raise RuntimeError("Desktop exited before startup")
                try:
                    conn = socket.create_connection(("127.0.0.1", port), timeout=1)
                    break
                except OSError:
                    if time.monotonic() >= deadline:
                        raise RuntimeError("Desktop did not start within 45 seconds") from None
                    time.sleep(0.2)
            with conn:
                conn.settimeout(20)
                f = conn.makefile("rwb")

                def call(method, params=None):
                    f.write(json.dumps({"id": 1, "method": method, "params": params or {}}).encode() + b"\n")
                    f.flush()
                    reply = json.loads(f.readline())
                    assert reply.get("ok"), reply
                    return reply["result"]

                def command(name, params=None):
                    return call("engine.execute", {"command": name, "params": params or {}})

                def count(doc):
                    def nodes(node):
                        return int(node["kind"] not in ("Layer", "Group")) + sum(nodes(n) for n in node.get("children", []))
                    return sum(nodes(n) for n in doc["layers"])

                state = call("ui.inspect")
                assert state["graphicsAdapter"] and min(state["window"]) > 0, state
                assert len(state["documents"]) == 1, state["documents"]
                if sys.platform in ("win32", "linux"):
                    assert state["workspace"]["enabled"] and state["workspace"]["customTitlebar"], state["workspace"]
                    assert not state["nativeMenuBar"], "Desktop menus must remain accessible inside the window"
                assert command("app.language", {"lang": "zh-hans"}) == "zh-hans"
                before = count(call("document.inspect"))
                command("shape.ellipse", {"x": 140, "y": 20, "width": 60, "height": 60})
                assert count(call("document.inspect")) == before + 1
                command("edit.undo")
                assert count(call("document.inspect")) == before
                command("edit.redo")
                assert count(call("document.inspect")) == before + 1
                call("ui.render", {"path": str(work / "artboard.png")})
                png(work / "artboard.png", (240, 120))
                call("ui.screenshot", {"path": str(work / "desktop.png")})
                png(work / "desktop.png")
                def resize(width, height):
                    call("ui.resize", {"width": width, "height": height})
                    deadline = time.monotonic() + 10
                    while time.monotonic() < deadline:
                        state = call("ui.inspect")
                        if all(abs(a-b) <= 1 for a, b in zip(state["window"], (width, height))):
                            return state
                        time.sleep(.1)
                    raise AssertionError("Window resize did not reach its requested logical size")

                resize(1440, 900)
                for brightness in ("light", "dark"):
                    call("ui.set", {"brightness": brightness})
                    call("ui.screenshot", {"path": str(work / f"desktop-{brightness}.png")})
                    png(work / f"desktop-{brightness}.png")
                resize(800, 600)
                command("window.panel", {"panel": "history"})
                state = call("ui.inspect")
                assert state["canvasRect"][2] > 700, state["canvasRect"]
                call("ui.screenshot", {"path": str(work / "desktop-narrow.png")})
                png(work / "desktop-narrow.png")
                command("file.newDialog")
                call("ui.screenshot", {"path": str(work / "new-document.png")})
                png(work / "new-document.png")
                call("ui.dialog.cancel")
                state = call("ui.inspect")
                assert len(state["documents"]) == 1, "Cancel must keep the existing document"
                if state["workspace"]["customTitlebar"]:
                    call("ui.click", {"x": state["window"][0] - 23, "y": 26})
                    deadline = time.monotonic() + 5
                    while time.monotonic() < deadline:
                        state = call("ui.inspect")
                        if state["ui"].get("dialog"):
                            break
                        time.sleep(.1)
                    assert state["ui"]["dialog"]["kind"] == "saveChanges", state["ui"]["dialog"]
                    call("ui.dialog.cancel")
                print("Desktop: shared workspace, light/dark, narrow inspector, new-dialog cancel and unsaved caption close passed", flush=True)
                print("Desktop: file launch, Chinese UI, draw, undo/redo, render and screenshot passed; " + str(state["graphicsAdapter"]), flush=True)
        except Exception:
            log.flush()
            log.seek(0)
            print(log.read(), flush=True)
            raise
        finally:
            if proc.poll() is None:
                proc.terminate()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prefix", type=Path, default=Path("/usr"))
    parser.add_argument("--bin-dir", type=Path, help="Directory containing the app and CLI (Windows installer or portable folder)")
    parser.add_argument("--desktop", action="store_true")
    parser.add_argument("--output", type=Path, help="Retain generated evidence in this directory")
    args = parser.parse_args()
    bindir = args.bin_dir.resolve() if args.bin_dir else args.prefix.resolve() / "bin"
    if args.output:
        args.output.mkdir(parents=True, exist_ok=True)
        run(bindir, args.desktop, args.output.resolve())
    else:
        with tempfile.TemporaryDirectory(prefix="kaleiform-smoke-") as directory:
            run(bindir, args.desktop, Path(directory))
