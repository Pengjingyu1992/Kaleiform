#!/usr/bin/env python3
"""Capture synthetic native UI/font evidence on the same Windows and Linux runners."""
import json
import time


def run(call, work):
    out = work / "ui-audit"
    out.mkdir(exist_ok=True)

    def command(name, params=None):
        return call("engine.execute", {"command": name, "params": params or {}})

    def wait(predicate, label):
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            state = call("ui.inspect")
            if predicate(state):
                return state
            time.sleep(0.1)
        raise AssertionError(f"UI audit did not settle: {label}")

    def resize(width, height):
        call("ui.resize", {"width": width, "height": height})
        wait(lambda s: all(abs(a - b) <= 2 for a, b in zip(s["window"], (width, height))), "resize")

    def capture(name):
        state = wait(lambda s: s["typography"]["chineseReady"], "Chinese glyph fallback")
        # Allow the newly installed fallback to be laid out and presented before capture.
        time.sleep(0.2)
        call("ui.screenshot", {"path": str(out / (name + ".png"))})
        (out / (name + ".json")).write_text(json.dumps(state, ensure_ascii=False, indent=2), encoding="utf-8")

    command("app.language", {"lang": "zh-hans"})
    command("prefs.set", {"values": {"uiScaling": 1.0}})
    command("window.workspace", {"name": "Essentials"})
    command("file.new", {"name": "品牌画板 / Brand Board", "width": 960, "height": 640})
    command("shape.rectangle", {"x": 80, "y": 80, "width": 440, "height": 300})
    command("paint.setFill", {"color": "#5787e4"})
    command("text.create", {"x": 100, "y": 440, "text": "虹构设计 / Kaleiform Design 2026", "size": 28})
    command("select.all")
    resize(1440, 900)
    command("view.fitArtboard")
    command("window.panel", {"panel": "properties"})
    for brightness in ("dark", "light"):
        call("ui.set", {"brightness": brightness, "glass": False})
        capture("properties-" + brightness)
    resize(800, 600)
    capture("inspector-narrow")
    command("file.newDialog")
    capture("new-document-compact")
    call("ui.dialog.cancel")
    resize(1440, 900)
    for zoom in (1.5, 2.0):
        command("prefs.set", {"key": "uiScaling", "value": zoom})
        wait(lambda s: abs(s["typography"]["zoom"] - zoom) < 0.01, "UI scaling")
        command("file.newDialog")
        capture(f"new-document-{zoom}")
        call("ui.dialog.cancel")
    command("prefs.set", {"key": "uiScaling", "value": 1.0})
    wait(lambda s: abs(s["typography"]["zoom"] - 1.0) < 0.01, "restore scaling")
    resize(1440, 900)
    print("Native UI audit: six light/dark, compact and 150%/200% captures retained", flush=True)
