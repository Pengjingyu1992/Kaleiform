"""Probe an owned synthetic test window before/after making it visible; no release approval."""

import argparse
import ctypes
from ctypes import wintypes
import json
import os
from pathlib import Path
import runpy
import re
import subprocess
import time


def notice(label, value):
    message = label + ": " + json.dumps(value, ensure_ascii=False)
    print("::notice::" + message.replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A"), flush=True)


def window(pid, pin=False):
    api = ctypes.WinDLL("user32", use_last_error=True)
    callback_type = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
    api.EnumWindows.argtypes = [callback_type, wintypes.LPARAM]
    api.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
    api.GetWindow.argtypes = [wintypes.HWND, wintypes.UINT]
    api.GetWindow.restype = wintypes.HWND
    api.GetForegroundWindow.restype = wintypes.HWND
    api.IsIconic.argtypes = [wintypes.HWND]
    api.IsWindowVisible.argtypes = [wintypes.HWND]
    api.GetWindowTextLengthW.argtypes = [wintypes.HWND]
    api.ShowWindow.argtypes = [wintypes.HWND, ctypes.c_int]
    api.SetForegroundWindow.argtypes = [wintypes.HWND]
    api.SetWindowPos.argtypes = [
        wintypes.HWND,
        wintypes.HWND,
        ctypes.c_int,
        ctypes.c_int,
        ctypes.c_int,
        ctypes.c_int,
        wintypes.UINT,
    ]
    candidates = []

    @callback_type
    def collect(handle, _):
        owner = wintypes.DWORD()
        api.GetWindowThreadProcessId(handle, ctypes.byref(owner))
        if owner.value == pid and not api.GetWindow(handle, 4) and api.GetWindowTextLengthW(handle):
            candidates.append(handle)
        return True

    api.EnumWindows(collect, 0)
    assert len(candidates) == 1, f"Expected one owned root window, found {len(candidates)}"
    handle = candidates[0]
    if pin:
        if api.IsIconic(handle):
            api.ShowWindow(handle, 9)
        assert api.SetWindowPos(handle, -1, 0, 0, 0, 0, 0x43), ctypes.get_last_error()
        api.SetForegroundWindow(handle)
        time.sleep(0.5)
    return {
        "visible": bool(api.IsWindowVisible(handle)),
        "minimized": bool(api.IsIconic(handle)),
        "foreground": api.GetForegroundWindow() == handle,
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--mode", choices=["basic", "visual"], default="visual")
    args = parser.parse_args()
    assert os.name == "nt"
    root = Path(__file__).resolve().parents[2]
    original_run_path = runpy.run_path
    smoke = original_run_path(str(root / "packaging" / "smoke.py"))
    original_popen = subprocess.Popen
    owned = []

    def capture_process(*values, **kwargs):
        process = original_popen(*values, **kwargs)
        if values and Path(str(values[0][0])).name.lower() == "vectorcraft.exe":
            owned.append(process)
        return process

    def probe(call, work):
        assert len(owned) == 1
        results = []

        def capture(phase, pin=False):
            try:
                if pin:
                    notice("Owned window pinned", window(owned[0].pid, pin=True))
                    call("ui.focus")
                state = call("ui.inspect")
                notice(
                    "Frame state " + phase,
                    {k: state.get(k) for k in ("window", "windowState", "perf", "canvasRaster")},
                )
                call("ui.screenshot", {"path": str(work / (phase + ".png"))})
                result = {"phase": phase, "ok": True}
            except Exception as error:
                result = {
                    "phase": phase,
                    "ok": False,
                    "error": type(error).__name__ + ": " + str(error)[:600],
                }
            results.append(result)
            notice("Capture probe", result)
            (work / "probe-results.json").write_text(json.dumps(results, indent=2), encoding="utf-8")

        capture("before-fixture")

        def command(name, params=None):
            return call("engine.execute", {"command": name, "params": params or {}})

        command("file.new", {"width": 320, "height": 240})
        capture("new-document")
        a = command("shape.rectangle", {"x": 20, "y": 20, "width": 130, "height": 100})["id"]
        command("paint.setFill", {"color": "#3366ff"})
        b = command("shape.ellipse", {"x": 90, "y": 65, "width": 110, "height": 95})["id"]
        command("paint.setFill", {"color": "#ff8833"})
        capture("shapes")
        command(
            "text.create",
            {
                "x": 25,
                "y": 200,
                "text": "虹构 Desktop QA — 中文 ABC 123",
                "size": 16,
                "font": "Noto Sans CJK SC",
            },
        )
        capture("text")
        command("select.set", {"ids": [a, b]})
        command("prefs.set", {"values": {"backgroundSave": False, "backgroundExport": False}})
        command("app.language", {"lang": "zh-hans"})
        command("view.fitArtboard")
        for method in ["engine.commands", "ui.tool.list", "ui.menu.list"]:
            call(method)
        command("effect.list")
        command("prefs.list")
        command("document.formats")
        call("ui.resize", {"width": 1440, "height": 900})
        time.sleep(0.5)
        command("window.panel", {"panel": "actions"})
        (work / "probe-state.json").write_text(
            json.dumps(call("ui.inspect"), ensure_ascii=False, indent=2), encoding="utf-8"
        )
        notice("Owned window before", window(owned[0].pid))
        capture("before")
        capture("after", pin=True)
        assert results[-1]["ok"], "Owned-window visibility did not restore capture"

    def route(path, *values, **kwargs):
        if Path(path).name == "desktop_functional.py":
            return {"run": probe}
        if Path(path).name == "desktop_visual.py" and args.mode == "basic":
            return {"run": lambda call, work: notice("Diagnostic mode", "basic without visual zoom sequence")}
        return original_run_path(path, *values, **kwargs)

    args.output.mkdir(parents=True, exist_ok=True)
    subprocess.Popen = capture_process
    runpy.run_path = route
    try:
        smoke["run"](args.bin_dir.resolve(), True, args.output.resolve(), True)
    except Exception:
        log = args.output / "desktop.log"
        if log.exists():
            tail = log.read_text(encoding="utf-8", errors="replace").splitlines()[-100:]
            for line in [s for s in tail if re.search(r"panic|error|warn|device|DXGI|screenshot", s, re.I)][
                -12:
            ]:
                notice("New probe app diagnostic", line[:600])
        raise
    finally:
        runpy.run_path = original_run_path
        subprocess.Popen = original_popen


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        notice("Diagnostic exception", type(error).__name__ + ": " + str(error)[:800])
        raise
