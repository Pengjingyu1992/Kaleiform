"""Run unchanged synthetic checks, then diagnose only the first failed native capture."""

import argparse
import ctypes
from ctypes import wintypes
import json
import os
from pathlib import Path
import re
import runpy
import socket
import time


class CapturedFailure(BaseException):
    pass


def power_request(flags):
    api = ctypes.WinDLL("kernel32", use_last_error=True)
    api.SetThreadExecutionState.argtypes = [wintypes.DWORD]
    api.SetThreadExecutionState.restype = wintypes.DWORD
    previous = api.SetThreadExecutionState(flags)
    if not previous:
        raise OSError(ctypes.get_last_error(), "SetThreadExecutionState failed")
    return previous


def owned_window(pid):
    api = ctypes.WinDLL("user32", use_last_error=True)
    callback_type = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
    api.EnumWindows.argtypes = [callback_type, wintypes.LPARAM]
    api.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
    api.GetWindow.argtypes = [wintypes.HWND, wintypes.UINT]
    api.GetWindow.restype = wintypes.HWND
    api.GetWindowTextLengthW.argtypes = [wintypes.HWND]
    api.GetForegroundWindow.restype = wintypes.HWND
    api.IsIconic.argtypes = [wintypes.HWND]
    api.IsWindowVisible.argtypes = [wintypes.HWND]
    api.GetClientRect.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.RECT)]
    handles = []

    @callback_type
    def collect(handle, _):
        owner = wintypes.DWORD()
        api.GetWindowThreadProcessId(handle, ctypes.byref(owner))
        if owner.value == pid and not api.GetWindow(handle, 4) and api.GetWindowTextLengthW(handle):
            handles.append(handle)
        return True

    api.EnumWindows(collect, 0)
    assert len(handles) == 1, f"Expected one owned root window, found {len(handles)}"
    handle = handles[0]
    rect = wintypes.RECT()
    assert api.GetClientRect(handle, ctypes.byref(rect))
    cloaked = wintypes.DWORD()
    dwm = ctypes.WinDLL("dwmapi")
    dwm.DwmGetWindowAttribute.argtypes = [wintypes.HWND, wintypes.DWORD, ctypes.c_void_p, wintypes.DWORD]
    result = dwm.DwmGetWindowAttribute(handle, 14, ctypes.byref(cloaked), ctypes.sizeof(cloaked))

    class LastInput(ctypes.Structure):
        _fields_ = [("cbSize", wintypes.UINT), ("dwTime", wintypes.DWORD)]

    last_input = LastInput()
    last_input.cbSize = ctypes.sizeof(last_input)
    api.GetLastInputInfo.argtypes = [ctypes.POINTER(LastInput)]
    idle = None
    if api.GetLastInputInfo(ctypes.byref(last_input)):
        kernel = ctypes.WinDLL("kernel32")
        kernel.GetTickCount64.restype = ctypes.c_ulonglong
        idle = (kernel.GetTickCount64() - last_input.dwTime) & 0xFFFFFFFF
    return {
        "visible": bool(api.IsWindowVisible(handle)),
        "minimized": bool(api.IsIconic(handle)),
        "foreground": api.GetForegroundWindow() == handle,
        "clientPixels": [rect.right - rect.left, rect.bottom - rect.top],
        "dwmCloaked": cloaked.value if result == 0 else None,
        "inputIdleMs": idle,
    }


def client(port, method, params=None):
    with socket.create_connection(("127.0.0.1", port), timeout=3) as connection:
        connection.settimeout(15)
        connection.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        with connection.makefile("rwb") as stream:
            stream.write(json.dumps({"id": 770, "method": method, "params": params or {}}).encode() + b"\n")
            stream.flush()
            reply = json.loads(stream.readline())
            assert reply.get("ok"), reply
            return reply["result"]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--mode", choices=["baseline", "awake"], required=True)
    args = parser.parse_args()
    assert os.name == "nt"
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=True)
    root = Path(__file__).resolve().parents[2]
    smoke = runpy.run_path(str(root / "packaging" / "smoke.py"))
    run = smoke["run"]
    summary = {"mode": args.mode, "outcome": "starting", "recovery": []}
    previous_power = None

    def state(port):
        value = client(port, "ui.inspect")
        result = {
            k: value.get(k) for k in ("window", "windowState", "perf", "canvasRaster", "graphicsAdapter")
        }
        result["status"] = str(value.get("ui", {}).get("status", ""))[:600]
        result["typography"] = value.get("typography")
        return result

    def diagnose(process, port, reply):
        nonlocal previous_power
        summary["firstFailure"] = reply
        try:
            summary["nativeWindow"] = owned_window(process.pid)
            summary["state"] = state(port)
        except Exception as error:
            summary["stateError"] = type(error).__name__ + ": " + str(error)[:600]

        def attempt(label, focus=True):
            start = time.monotonic()
            try:
                value = client(
                    port, "ui.screenshot", {"path": str(args.output / (label + ".png")), "focus": focus}
                )
                result = {
                    "phase": label,
                    "ok": True,
                    "width": value.get("width"),
                    "height": value.get("height"),
                }
            except Exception as error:
                result = {
                    "phase": label,
                    "ok": False,
                    "error": type(error).__name__ + ": " + str(error)[:600],
                }
            result["seconds"] = round(time.monotonic() - start, 3)
            summary["recovery"].append(result)

        # No pacing/capture changes until the original suite has failed.
        attempt("unchanged-control")
        if previous_power is None:
            previous_power = power_request(0x80000003)  # continuous display/system request, this thread only
        summary["powerPreviousState"] = previous_power
        attempt("after-display-request")
        attempt("without-focus", False)
        try:
            client(port, "ui.render", {"path": str(args.output / "recovery-artboard.png")})
            summary["artboardRender"] = True
            current = state(port)
            width, height = current["window"]
            client(port, "ui.resize", {"width": max(640, width - 32), "height": max(480, height - 32)})
            attempt("after-real-resize", False)
            summary["finalState"] = state(port)
            summary["finalNativeWindow"] = owned_window(process.pid)
        except Exception as error:
            summary["laterError"] = type(error).__name__ + ": " + str(error)[:600]
        raise CapturedFailure()

    run.__globals__["capture_failure_diagnostic"] = diagnose
    exit_code = 0
    try:
        if args.mode == "awake":
            previous_power = power_request(0x80000003)
            summary["powerPreviousState"] = previous_power
        run(args.bin_dir.resolve(), True, args.output, True)
        summary["outcome"] = "full-suite-completed"
        results = json.loads((args.output / "functional-results.json").read_text())
        summary["functionalChecks"] = len(results)
        summary["functionalFailures"] = sum(row["status"] != "pass" for row in results)
    except CapturedFailure:
        summary["outcome"] = "first-capture-failure-diagnosed"
        exit_code = 2
    except Exception as error:
        summary["outcome"] = "unexpected-error"
        summary["error"] = type(error).__name__ + ": " + str(error)[:800]
        summary["lastRpc"] = run.__globals__.get("last_diagnostic_rpc")
        exit_code = 1
    finally:
        if previous_power is not None:
            power_request(previous_power | 0x80000000)
        log = args.output / "desktop.log"
        if log.exists():
            with log.open("rb") as stream:
                stream.seek(0, os.SEEK_END)
                stream.seek(max(0, stream.tell() - 16384))
                lines = stream.read(16384).decode("utf-8", errors="replace").splitlines()
            summary["newAppDiagnostics"] = [
                line[:600]
                for line in lines
                if re.search(r"panic|error|warn|device|DXGI|screenshot", line, re.I)
            ][-12:]
        (args.output / "diagnosis.json").write_text(
            json.dumps(summary, ensure_ascii=False, indent=2), encoding="utf-8"
        )
        # One annotation avoids GitHub's per-step notice limit; this is new experiment metadata,
        # never screenshot pixels, old blocked logs, font bytes, desktop names or credentials.
        message = "Native capture diagnosis: " + json.dumps(summary, ensure_ascii=False)
        print(
            "::warning::" + message.replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A"), flush=True
        )
    raise SystemExit(exit_code)


if __name__ == "__main__":
    main()
