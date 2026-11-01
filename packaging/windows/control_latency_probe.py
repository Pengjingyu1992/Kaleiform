"""Measure original synthetic RPCs without adding calls, logging user values or changing deadlines."""

import argparse
import json
from pathlib import Path
import time


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    root = Path(__file__).resolve().parents[2]
    path = root / "packaging/smoke.py"
    source = path.read_text(encoding="utf-8")
    begin_marker = "                def call(method, params=None):\n"
    finish_marker = '                    assert reply.get("ok"), reply\n'
    assert source.count(begin_marker) == source.count(finish_marker) == 1
    source = source.replace(
        begin_marker, begin_marker + "                    observe_begin(method, params)\n"
    )
    source = source.replace(
        finish_marker, "                    observe_reply(method, params, reply)\n" + finish_marker
    )
    stats = {
        "pending": None,
        "calls": 0,
        "byMethod": {},
        "recent": [],
        "inspections": [],
        "outcome": "starting",
    }

    def label(method, params):
        # Command identifiers only; no argument values, file paths, text or document contents.
        return method + (":" + str(params.get("command")) if method == "engine.execute" and params else "")

    def begin(method, params):
        stats["pending"] = {"method": label(method, params), "started": time.monotonic()}

    def reply(method, params, value):
        pending = stats["pending"]
        seconds = time.monotonic() - pending["started"]
        key = label(method, params)
        aggregate = stats["byMethod"].setdefault(key, {"count": 0, "totalSeconds": 0.0, "maxSeconds": 0.0})
        aggregate["count"] += 1
        aggregate["totalSeconds"] += seconds
        aggregate["maxSeconds"] = max(aggregate["maxSeconds"], seconds)
        stats["calls"] += 1
        stats["recent"] = (
            stats["recent"] + [{"method": key, "seconds": round(seconds, 4), "ok": bool(value.get("ok"))}]
        )[-12:]
        if method == "ui.inspect" and value.get("ok"):
            result = value.get("result", {})
            inspection = {
                k: result.get(k) for k in ("perf", "canvasRaster", "window", "windowState", "graphicsAdapter")
            }
            stats["inspections"] = (stats["inspections"] + [inspection])[-3:]
        stats["pending"] = None

    namespace = {
        "__file__": str(path),
        "__name__": "fresh_latency_probe",
        "observe_begin": begin,
        "observe_reply": reply,
    }
    exec(compile(source, str(path), "exec"), namespace)
    try:
        namespace["run"](args.bin_dir.resolve(), True, args.output, True)
        stats["outcome"] = "original-suite-completed"
    except BaseException as error:
        stats["outcome"] = type(error).__name__
        raise
    finally:
        if stats["pending"]:
            stats["pending"]["ageSeconds"] = round(time.monotonic() - stats["pending"].pop("started"), 3)
        for aggregate in stats["byMethod"].values():
            aggregate["totalSeconds"] = round(aggregate["totalSeconds"], 3)
            aggregate["maxSeconds"] = round(aggregate["maxSeconds"], 3)
        (args.output / "control-latency.json").write_text(json.dumps(stats, indent=2), encoding="utf-8")


if __name__ == "__main__":
    main()
