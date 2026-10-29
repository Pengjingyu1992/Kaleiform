#!/usr/bin/env python3
"""Synthetic desktop regression matrix shared by native Windows and Linux runners.

Catalog counts describe registration, not feature completeness. The matrix checks
panel reachability, preference storage, effect transactions, exports and undo;
physical button dispatch and tool safety are covered by native Rust UI tests.
"""
import base64
import json
import struct
import zlib
import os
import time
import traceback


class Client:
    def __init__(self, call, work):
        self.call, self.work = call, work

    def cmd(self, command, params=None):
        history = len(self.call("document.inspect")["history"]) if command == "imageTrace.make" else None
        result = self.call("engine.execute", {"command": command, "params": params or {}})
        if command == "imageTrace.make" and result.get("background"):
            deadline = time.monotonic() + 30
            while self.call("ui.inspect")["traceJob"]["running"]:
                assert time.monotonic() < deadline, "Image Trace worker did not finish"
                time.sleep(0.05)
            document = self.call("document.inspect")
            assert len(document["history"]) == history + 1, "Trace did not commit exactly once: " + str(
                self.call("ui.inspect")["ui"].get("status")
            )

            def flatten(nodes):
                for node in nodes:
                    yield node
                    yield from flatten(node.get("children", []))

            selected = set(document["selection"])
            root = next(n for n in flatten(document["layers"]) if n["id"] in selected)
            paths = [n for n in flatten([root]) if n["kind"] == "Path"]
            result = {"paths": len(paths), "anchors": sum(n["anchors"] for n in paths), "background": True}
        return result

    def save(self, name, value):
        (self.work / (name + ".json")).write_text(
            json.dumps(value, ensure_ascii=False, indent=2), encoding="utf-8"
        )


def run(call, w):
    c = Client(call, w)
    checks = []
    (w / "screenshots").mkdir(exist_ok=True)
    (w / "formats").mkdir(exist_ok=True)
    (w / "effects").mkdir(exist_ok=True)

    def check(name, fn):
        t = time.monotonic()
        try:
            detail = fn()
            r = {"name": name, "status": "pass", "seconds": round(time.monotonic() - t, 3), "detail": detail}
        except Exception as e:
            r = {
                "name": name,
                "status": "fail",
                "seconds": round(time.monotonic() - t, 3),
                "error": str(e),
                "trace": traceback.format_exc(),
            }
            print("FAIL", name, str(e), flush=True)
            try:
                c.call("ui.dialog.cancel")
            except Exception:
                pass
        checks.append(r)
        (w / "functional-results.json").write_text(
            json.dumps(checks, ensure_ascii=False, indent=2), encoding="utf-8"
        )
        return r

    def shot(name):
        path = w / "screenshots" / f"{name}.png"
        c.call("ui.screenshot", {"path": str(path)})
        assert path.read_bytes()[:8] == b"\x89PNG\r\n\x1a\n"

    def snapshot():
        return c.cmd("document.serialize")["dataBase64"]

    def fixture():
        c.cmd("file.new", {"width": 320, "height": 240})
        a = c.cmd("shape.rectangle", {"x": 20, "y": 20, "width": 130, "height": 100})["id"]
        c.cmd("paint.setFill", {"color": "#3366ff"})
        b = c.cmd("shape.ellipse", {"x": 90, "y": 65, "width": 110, "height": 95})["id"]
        c.cmd("paint.setFill", {"color": "#ff8833"})
        t = c.cmd(
            "text.create",
            {"x": 25, "y": 200, "text": "虹构 Desktop QA — 中文 ABC 123", "size": 16, "font": "Noto Sans CJK SC"},
        )["id"]
        c.cmd("select.set", {"ids": [a, b]})
        return a, b, t

    ids = fixture()
    c.cmd("prefs.set", {"values": {"backgroundSave": False, "backgroundExport": False}})
    c.cmd("app.language", {"lang": "zh-hans"})
    c.cmd("view.fitArtboard")
    commands = c.call("engine.commands")
    tools = sum(c.call("ui.tool.list"), [])
    menus = c.call("ui.menu.list")
    effects = c.cmd("effect.list")["catalog"]
    prefs = c.cmd("prefs.list")
    fmts = c.cmd("document.formats")
    c.save(
        "inventory",
        {
            "commands": len(commands),
            "tools": len(tools),
            "panels": len({m["params"]["panel"] for m in menus if m["command"] == "window.panel"}),
            "effects": len(effects),
            "preferences": len(prefs),
            "formats": fmts,
            "disabledMenus": [m for m in menus if not m["command"]],
        },
    )
    print(
        "Inventory",
        len(commands),
        "commands",
        len(tools),
        "tools",
        len(effects),
        "effects",
        len(prefs),
        "preferences",
        flush=True,
    )

    def resize(width, height):
        c.call("ui.resize", {"width": width, "height": height})
        for _ in range(100):
            size = c.call("ui.inspect")["window"]
            if abs(size[0] - width) < 2 and abs(size[1] - height) < 2:
                return
            time.sleep(0.1)
        raise AssertionError(("resize", size))

    for width, height in [(1440, 900), (950, 700)]:
        check(f"window {width}x{height}", lambda: resize(width, height))
        for panel in sorted({m["params"]["panel"] for m in menus if m["command"] == "window.panel"}):

            def run_panel(panel=panel):
                c.cmd("window.panel", {"panel": panel})
                shot(f"{width}-{panel}")

            check(f"panel {width} {panel}", run_panel)
        print("Panels completed", width, flush=True)
    for category in sorted({p["category"] for p in prefs}):

        def run_prefs(category=category):
            c.cmd("edit.preferences", {"category": category})
            assert c.call("ui.inspect")["ui"]["dialog"]["kind"] == "preferences"
            shot("prefs-" + category.replace(" ", "-").replace("&", "and"))
            c.call("ui.dialog.cancel")

        check("preferences page " + category, run_prefs)
    for p in prefs:

        def pref(p=p):
            old = p["value"]
            kind = p["kind"]
            values = [old]
            if kind == "bool":
                values = [not old, old]
            elif kind == "choice":
                values = p["options"]
            elif kind in ["integer", "number"]:
                values = [p.get("min", old), p.get("max", old)]
            elif kind == "color":
                values = ["#123456", old]
            try:
                for value in values:
                    c.cmd("prefs.set", {"key": p["key"], "value": value})
                    actual = c.cmd("prefs.get", {"key": p["key"]})
                    assert actual == value or (kind in ["integer", "number"] and abs(actual - value) < 1e-6), (
                        p["key"],
                        value,
                        actual,
                    )
            finally:
                c.cmd("prefs.set", {"key": p["key"], "value": old})
            return {"values": len(values), "storageOnly": kind == "text"}

        check("preference " + p["key"], pref)
    # Restore performance-affecting options changed by the catalog sweep.
    c.cmd("prefs.set", {"values": {"backgroundSave": False, "backgroundExport": False}})
    resize(1440, 900)
    for brightness in ["dark", "mediumDark", "mediumLight", "light"]:
        for glass in [False, True]:

            def appearance(brightness=brightness, glass=glass):
                c.call("ui.set", {"brightness": brightness, "glass": glass})
                shot(f"theme-{brightness}-{glass}")

            check(f"theme {brightness} glass {glass}", appearance)
    for name in c.cmd("window.workspace.list")["workspaces"]:

        def workspace(name=name):
            c.cmd("window.workspace", {"name": name})
            assert c.cmd("window.workspace.list")["current"] == name
            shot("workspace-" + name.replace(" ", "-"))

        check("workspace " + name, workspace)
    c.cmd("window.workspace", {"name": "Essentials"})
    c.call("ui.set", {"brightness": "mediumDark", "glass": True})
    for panel in ["character", "paragraph", "imageTrace", "stroke", "tools"]:

        def floating(panel=panel):
            c.cmd("window.panel.float", {"panel": panel, "x": 450, "y": 120})
            shot("floating-" + panel)
            c.cmd("window.panel.dock", {"panel": panel})

        check("floating " + panel, floating)
    print("Preferences and layouts completed", flush=True)
    # Each default live effect is previewed, cancelled, committed, rendered, and undone.
    c.cmd("select.set", {"ids": [ids[0], ids[1]]})
    for effect in effects:

        def live_effect(effect=effect):
            before = snapshot()
            history = len(c.call("document.inspect")["history"])
            c.cmd("effect.dialog", {"effect": effect["id"]})
            assert c.call("ui.inspect")["ui"]["dialog"] is not None
            c.call("ui.dialog.cancel")
            assert snapshot() == before, "cancel changed document"
            c.cmd("effect.dialog", {"effect": effect["id"]})
            c.call("ui.dialog.confirm")
            assert len(c.call("document.inspect")["history"]) == history + 1, "effect must make one undo step"
            r = c.call("ui.render", {"path": str(w / "effects" / (effect["id"] + ".png"))})
            assert r["width"] == 320 and r["height"] == 240
            c.cmd("edit.undo")
            assert snapshot() == before, "undo changed original art"

        check("live effect " + effect["id"], live_effect)
    print("Effects completed", flush=True)
    # Format option matrix, with every readable export reopened by the app.
    c.cmd("select.all")
    base_count = len(c.call("ui.inspect")["documents"])
    variants = {
        "vectorcraft": [
            {},
            {"compress": True, "preview": True, "pdfCompatible": True},
            {"version": 1},
            {"version": 2},
            {"version": 3},
        ],
        "svg": [
            {},
            {"svg": {"outlineText": True, "minify": True}},
            {"svg": {"encoding": "utf16", "preserveEditing": True}},
            {"svg": {"responsive": True, "decimals": 2}},
        ],
        "pdf": [{}, {"compatibility": "1.4"}, {"preserveEditing": True}],
        "png": [
            {},
            {"ppi": 144, "background": "white", "interlaced": True},
            {"antiAlias": "none", "background": "#123456"},
        ],
        "jpg": [
            {},
            {"quality": 40, "colorModel": "cmyk", "method": "progressive"},
            {"colorModel": "gray", "method": "optimized"},
        ],
        "webp": [{}, {"lossless": False, "quality": 60}],
        "gif": [{}, {"colors": 8, "dither": "pattern", "interlaced": True}],
        "png8": [{}, {"colors": 16, "dither": "diffusion", "transparency": False}],
        "txt": [{}, {"encoding": "utf16", "lineEndings": "crlf"}],
        "dxf": [{}, {"version": "R12", "unit": "mm"}],
        "eps": [{}, {"previewFormat": "none"}],
        "tiff": [{}, {"lzw": False, "byteOrder": "big"}],
        "psd": [{}, {"layers": False}],
    }

    def close_loaded():
        r = c.cmd("file.close")
        if isinstance(r, dict) and r.get("pending") == "saveChanges":
            c.call("ui.dialog.set", {"field": "discard", "value": True})
            c.call("ui.dialog.confirm")

    for fmt in fmts["formats"]:
        if not fmt["write"]:
            continue
        for index, options in enumerate(variants.get(fmt["id"], [{}])):

            def export(fmt=fmt, index=index, options=options):
                assert not (
                    set(options) - set(fmt["options"]) - {"svg"}
                ), "Matrix used an unregistered export option"
                path = w / "formats" / f"{fmt['id']}-{index}.{fmt['extensions'][0]}"
                r = c.call("app.export", dict(options, path=str(path), format=fmt["id"]))
                assert path.exists() and path.stat().st_size > 0, (path, r)
                if fmt["read"] or fmt["id"] == "png8":
                    opened = c.cmd("document.open", {"path": str(path)})
                    assert (
                        opened["format"] == ("png" if fmt["id"] == "png8" else fmt["id"])
                        or fmt["id"] == "template"
                    ), opened
                    assert len(c.call("ui.inspect")["documents"]) == base_count + 1
                    assert c.call("document.inspect")["artboards"]
                    c.call("ui.render", {"path": str(path) + ".reopened.png"})
                    close_loaded()
                    assert len(c.call("ui.inspect")["documents"]) == base_count
                return {
                    "bytes": path.stat().st_size,
                    "warnings": r.get("warnings", []) if isinstance(r, dict) else [],
                    "reopened": fmt["read"] or fmt["id"] == "png8",
                }

            check(f"format {fmt['id']} options {index}", export)
    print("Formats completed", flush=True)
    for name in [
        "unite",
        "minusFront",
        "intersect",
        "exclude",
        "divide",
        "trim",
        "merge",
        "crop",
        "outline",
        "minusBack",
    ]:

        def pathfinder(name=name):
            c.cmd("select.set", {"ids": [ids[0], ids[1]]})
            before = snapshot()
            result = c.cmd("object.pathfinder." + name)
            assert result["ids"]
            c.call("ui.render")
            c.cmd("edit.undo")
            assert before == snapshot(), "Pathfinder undo did not restore art"

        check("Pathfinder " + name, pathfinder)

    def clipboard():
        c.cmd("select.set", {"ids": [ids[0], ids[1]]})
        before = snapshot()
        c.cmd("edit.copy")
        flavours = c.cmd("clipboard.flavours")["flavours"]
        assert "image/svg+xml" in flavours and "image/png" in flavours
        c.cmd("edit.pasteInPlace")
        assert len(c.call("document.inspect")["selection"]) == 2
        c.cmd("edit.undo")
        assert snapshot() == before
        return flavours

    check("clipboard copy paste undo", clipboard)

    def text():
        c.cmd("select.set", {"ids": [ids[2]]})
        before = snapshot()
        c.cmd("text.setStyle", {"size": 22, "tracking": 120})
        c.call("ui.render", {"path": str(w / "text-styled.png")})
        c.cmd("edit.undo")
        assert snapshot() == before
        c.cmd("edit.copy")
        export = c.cmd("clipboard.exportText")
        assert "中文" in export["text"]
        return export

    check("Chinese text styling copy undo", text)
    shot("final")
    print(
        "Done",
        sum(r["status"] == "pass" for r in checks),
        "passed",
        sum(r["status"] == "fail" for r in checks),
        "failed",
        flush=True,
    )

    checks.extend(trace_matrix(c, w))
    checks.extend(jpeg_matrix(c, w))
    (w / "functional-results.json").write_text(json.dumps(checks, ensure_ascii=False, indent=2), encoding="utf-8")
    failures = [r for r in checks if r["status"] != "pass"]
    if os.environ.get("GITHUB_ACTIONS") == "true":
        print(
            f"::notice::Desktop functional matrix: {len(checks) - len(failures)} passed, {len(failures)} failed",
            flush=True,
        )
    if failures:
        raise AssertionError(
            f"{len(failures)} functional checks failed: " + "; ".join(r["name"] for r in failures)
        )
    return checks


def png_pixel(path, x, y):
    """Independently inspect one RGB/RGBA PNG pixel using the public PNG filter rules."""
    data = path.read_bytes()
    assert data[:8] == b"\x89PNG\r\n\x1a\n"
    at, packed, header = 8, bytearray(), None
    while at < len(data):
        length = struct.unpack_from(">I", data, at)[0]
        kind, payload = data[at + 4 : at + 8], data[at + 8 : at + 8 + length]
        if kind == b"IHDR":
            header = struct.unpack(">IIBBBBB", payload)
        elif kind == b"IDAT":
            packed.extend(payload)
        at += length + 12
    width, height, depth, color, compression, filtering, interlaced = header
    assert depth == 8 and color in (2, 6) and (compression, filtering, interlaced) == (0, 0, 0)
    assert 0 <= x < width and 0 <= y < height and width * height <= 1024 * 1024
    channels = 4 if color == 6 else 3
    stride = width * channels
    raw = zlib.decompress(packed)
    assert len(raw) == height * (stride + 1)
    previous = bytearray(stride)
    for row_index in range(y + 1):
        offset = row_index * (stride + 1)
        mode = raw[offset]
        row = bytearray(raw[offset + 1 : offset + 1 + stride])
        assert mode in range(5)
        for i in range(stride):
            left = row[i - channels] if i >= channels else 0
            above = previous[i]
            corner = previous[i - channels] if i >= channels else 0
            if mode == 1:
                predictor = left
            elif mode == 2:
                predictor = above
            elif mode == 3:
                predictor = (left + above) // 2
            elif mode == 4:
                guess = left + above - corner
                distances = (abs(guess - left), abs(guess - above), abs(guess - corner))
                predictor = (left, above, corner)[distances.index(min(distances))]
            else:
                predictor = 0
            row[i] = (row[i] + predictor) & 255
        previous = row
    pixel = list(previous[x * channels : (x + 1) * channels])
    return pixel if channels == 4 else pixel + [255]


def jpeg_matrix(c, w):
    root = w / "jpeg"
    root.mkdir(exist_ok=True)
    results = []
    c.cmd("file.new", {"width": 320, "height": 240})
    c.cmd("shape.rectangle", {"x": 20, "y": 20, "width": 130, "height": 100})
    c.cmd("paint.setFill", {"color": "#3366ff"})
    variants = [
        {"colorModel": model, "method": method, "quality": quality}
        for model in ("rgb", "gray", "cmyk")
        for method in ("baseline", "optimized", "progressive")
        for quality in (40, 100)
    ]
    variants += [{"colorModel": "cmyk", "method": "progressive", "quality": 80, "scans": n} for n in (3, 4, 5)]
    for index, options in enumerate(variants):
        opened = False
        try:
            path = root / f"variant-{index}.jpg"
            c.call("app.export", dict(options, path=str(path), format="jpg"))
            c.cmd("document.open", {"path": str(path)})
            opened = True
            raster = root / f"variant-{index}.png"
            c.call("ui.render", {"path": str(raster)})
            pixel = png_pixel(raster, 50, 50)
            assert pixel[0] < 160 and (options["colorModel"] == "gray" or pixel[2] > pixel[0] + 60), (
                options,
                pixel,
            )
            c.cmd("select.all")
            traced = c.cmd("imageTrace.make")
            assert traced["paths"] > 0
            result = {"name": f"JPEG {index} {options}", "status": "pass", "detail": {"pixel": pixel}}
        except Exception as error:
            result = {"name": f"JPEG {index} {options}", "status": "fail", "error": str(error)}
        finally:
            if opened:
                closed = c.cmd("file.close")
                if isinstance(closed, dict) and closed.get("pending") == "saveChanges":
                    c.call("ui.dialog.set", {"field": "discard", "value": True})
                    c.call("ui.dialog.confirm")
        results.append(result)
    valid = (root / "variant-0.jpg").read_bytes()
    scan = valid.index(b"\xff\xda")
    end = scan + 2 + struct.unpack_from(">H", valid, scan + 2)[0]
    damaged = root / "missing-pixels.jpg"
    damaged.write_bytes(valid[:end])
    for command in ("document.open", "file.place"):
        before = c.cmd("document.serialize")["dataBase64"]
        count = len(c.call("ui.inspect")["documents"])
        rejected = False
        try:
            c.cmd(command, {"path": str(damaged)})
        except AssertionError:
            rejected = True
        unchanged = (
            count == len(c.call("ui.inspect")["documents"]) and before == c.cmd("document.serialize")["dataBase64"]
        )
        results.append({"name": "damaged JPEG " + command, "status": "pass" if rejected and unchanged else "fail"})
    print("JPEG:", len(results), "checks;", sum(r["status"] == "fail" for r in results), "failures", flush=True)
    return results


def trace_matrix(c, w):
    root = w / "trace"
    root.mkdir(exist_ok=True)
    results = []

    def png_fixture():
        width = height = 96
        pixels = bytearray()
        for y in range(height):
            pixels.append(0)
            for x in range(width):
                radius = (x - 48) ** 2 + (y - 48) ** 2
                if radius < 12**2 or radius > 35**2:
                    rgb = (255, 255, 255)
                elif x < 48 and y < 48:
                    rgb = (0, 0, 0)
                elif x >= 48 and y < 48:
                    rgb = (255, 0, 0)
                elif x < 48:
                    rgb = (0, 128, 255)
                else:
                    rgb = (0, 128, 0)
                pixels.extend((*rgb, 255))

        def chunk(name, data):
            return struct.pack(">I", len(data)) + name + data + struct.pack(">I", zlib.crc32(name + data))

        return (
            b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(pixels))
            + chunk(b"IEND", b"")
        )

    image = png_fixture()
    (root / "original.png").write_bytes(image)
    c.cmd("file.new", {"width": 96, "height": 96})
    placed = c.cmd(
        "file.place", {"name": "original.png", "dataBase64": base64.b64encode(image).decode(), "at": [48, 48]}
    )
    id = placed["ids"][0]

    def snapshot():
        return c.cmd("document.serialize")["dataBase64"]

    def check(name, fn):
        history = len(c.call("document.inspect")["history"])
        try:
            detail = fn()
            r = {"name": name, "status": "pass", "detail": detail}
        except Exception as e:
            r = {"name": name, "status": "fail", "error": str(e), "trace": traceback.format_exc()}
            print("FAIL", name, str(e), flush=True)
        while len(c.call("document.inspect")["history"]) > history:
            c.cmd("edit.undo")
        results.append(r)
        (root / "results.json").write_text(json.dumps(results, ensure_ascii=False, indent=2), encoding="utf-8")

    presets = c.cmd("imageTrace.presets")["presets"]
    cases = []
    for p in presets:
        for ignore in [False, True]:
            cases.append(
                (p["name"] + f"-white-{ignore}", {"preset": p["name"], "params": {"ignoreWhite": ignore}})
            )
    for mode in ["blackAndWhite", "grayscale", "color", "logo"]:
        cases.append(("mode-" + mode, {"params": {"mode": mode, "ignoreWhite": True}}))
    for palette in ["limited", "fullTone", "automatic", "documentLibrary"]:
        cases.append(
            (
                "palette-" + palette,
                {"params": {"mode": "color", "palette": palette, "ignoreWhite": True, "colors": 8}},
            )
        )
    for threshold in [1, 64, 128, 192, 254]:
        cases.append(("threshold-" + str(threshold), {"params": {"threshold": threshold, "ignoreWhite": True}}))
    for method in ["abutting", "overlapping"]:
        cases.append(("method-" + method, {"params": {"mode": "color", "method": method, "ignoreWhite": True}}))
    for name, params in cases:

        def tracing(name=name, params=params):
            before = snapshot()
            r = c.cmd("imageTrace.make", dict(params, id=id))
            assert r["paths"] > 0, r
            path = root / (name.replace("/", "-") + ".png")
            c.call("app.export", {"path": str(path), "format": "png", "background": "transparent"})
            pixel = png_pixel(path, 48, 48)
            if params["params"].get("ignoreWhite"):
                assert pixel[3] == 0, ("white hole filled", pixel)
            c.cmd("edit.undo")
            assert snapshot() == before, "trace undo changed source"
            return {"paths": r["paths"], "anchors": r["anchors"], "center": pixel}

        check(name, tracing)
    for view in [
        "tracingResult",
        "tracingResultWithOutlines",
        "outlines",
        "outlinesWithSourceImage",
        "sourceImage",
    ]:

        def trace_view(view=view):
            before = snapshot()
            c.cmd("imageTrace.make", {"id": id, "params": {"mode": "color"}})
            made = snapshot()
            traced = c.call("document.inspect")["selection"][0]
            r = c.cmd("imageTrace.setView", {"id": traced, "view": view})
            assert r["view"] == view
            c.call("ui.screenshot", {"path": str(root / (view + ".png"))})
            # setting tracingResult on a newly traced group is a no-op; other views add one undo.
            if snapshot() != made:
                c.cmd("edit.undo")
            c.cmd("edit.undo")
            assert before == snapshot()

        check("view-" + view, trace_view)

    def expand():
        before = snapshot()
        c.cmd("imageTrace.make", {"id": id})
        made = snapshot()
        c.cmd("imageTrace.expand", {"id": c.call("document.inspect")["selection"][0]})
        c.call("ui.render")
        c.cmd("edit.undo")
        assert snapshot() == made
        c.cmd("edit.undo")
        assert snapshot() == before

    check("expand and two undos", expand)

    def release():
        before = snapshot()
        first = c.call("app.export", {"format": "png", "background": "transparent"})["dataBase64"]
        c.cmd("imageTrace.make", {"id": id})
        made = snapshot()
        c.cmd("imageTrace.release", {"id": c.call("document.inspect")["selection"][0]})
        released = c.call("app.export", {"format": "png", "background": "transparent"})["dataBase64"]
        assert released == first, "release changed image pixels"
        c.cmd("edit.undo")
        assert snapshot() == made
        c.cmd("edit.undo")
        assert snapshot() == before

    check("release and two undos", release)
    print("Trace:", len(results), "checks;", sum(r["status"] == "fail" for r in results), "failures", flush=True)
    return results
