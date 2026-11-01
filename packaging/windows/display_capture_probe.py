"""Run a CI-only desktop check with a temporary supported primary display mode.

DEVMODEW layout: https://learn.microsoft.com/windows/win32/api/wingdi/ns-wingdi-devmodew
No registry updates, desktop/session switches, display drivers or security changes.
Only display geometry is reported; device names and other desktop contents are not read out.
"""

import argparse
from contextlib import contextmanager
import ctypes
import json
import os
import subprocess
import time


class DevMode(ctypes.Structure):
    # Use fixed-width Windows ABI types, also allowing layout validation on Linux.
    _fields_ = [
        ("device_name", ctypes.c_uint16 * 32),
        ("spec_version", ctypes.c_uint16),
        ("driver_version", ctypes.c_uint16),
        ("size", ctypes.c_uint16),
        ("driver_extra", ctypes.c_uint16),
        ("fields", ctypes.c_uint32),
        ("display_union", ctypes.c_uint32 * 4),
        ("color", ctypes.c_int16),
        ("duplex", ctypes.c_int16),
        ("y_resolution", ctypes.c_int16),
        ("tt_option", ctypes.c_int16),
        ("collate", ctypes.c_int16),
        ("form_name", ctypes.c_uint16 * 32),
        ("log_pixels", ctypes.c_uint16),
        ("bits_per_pixel", ctypes.c_uint32),
        ("width", ctypes.c_uint32),
        ("height", ctypes.c_uint32),
        ("display_flags", ctypes.c_uint32),
        ("frequency", ctypes.c_uint32),
        ("icm_method", ctypes.c_uint32),
        ("icm_intent", ctypes.c_uint32),
        ("media_type", ctypes.c_uint32),
        ("dither_type", ctypes.c_uint32),
        ("reserved1", ctypes.c_uint32),
        ("reserved2", ctypes.c_uint32),
        ("panning_width", ctypes.c_uint32),
        ("panning_height", ctypes.c_uint32),
    ]


def geometry(mode):
    return {"width": mode.width, "height": mode.height, "bpp": mode.bits_per_pixel, "hz": mode.frequency}


def notice(value):
    message = "CI display experiment: " + json.dumps(value)
    print("::notice::" + message.replace("%", "%25").replace("\n", "%0A"), flush=True)


@contextmanager
def display_mode(wide):
    assert os.name == "nt", "This helper is only for a native Windows CI host"
    assert os.environ.get("GITHUB_ACTIONS") == "true", "Use a disposable GitHub Actions runner"
    assert os.environ.get("GITHUB_REPOSITORY") == "Pengjingyu1992/Kaleiform"
    assert ctypes.sizeof(DevMode) == 220 and DevMode.width.offset == 172 and DevMode.height.offset == 176
    api = ctypes.WinDLL("user32", use_last_error=True)
    api.EnumDisplaySettingsW.argtypes = [ctypes.c_wchar_p, ctypes.c_uint32, ctypes.POINTER(DevMode)]
    api.EnumDisplaySettingsW.restype = ctypes.c_int32
    api.ChangeDisplaySettingsExW.argtypes = [
        ctypes.c_wchar_p,
        ctypes.POINTER(DevMode),
        ctypes.c_void_p,
        ctypes.c_uint32,
        ctypes.c_void_p,
    ]
    api.ChangeDisplaySettingsExW.restype = ctypes.c_int32

    def read(index):
        mode = DevMode()
        mode.size = ctypes.sizeof(DevMode)
        return mode if api.EnumDisplaySettingsW(None, index, ctypes.byref(mode)) else None

    original = read(0xFFFFFFFF)  # ENUM_CURRENT_SETTINGS
    assert original is not None, "Could not query the current display mode"
    modes = []
    for index in range(4096):
        mode = read(index)
        if mode is None:
            break
        modes.append(mode)
    sizes = sorted({(m.width, m.height) for m in modes})
    notice({"requested": "wide" if wide else "original", "before": geometry(original), "sizes": sizes[:50]})
    changed = False
    try:
        if wide:
            available = [m for m in modes if m.width >= 1600 and m.height >= 1050 and m.bits_per_pixel == 32]
            if available:
                selected = min(
                    available,
                    key=lambda m: (
                        abs(m.width - 1920) + abs(m.height - 1080),
                        abs(m.frequency - original.frequency),
                    ),
                )
                desired = DevMode.from_buffer_copy(original)
                desired.width, desired.height = selected.width, selected.height
                desired.bits_per_pixel, desired.frequency = selected.bits_per_pixel, selected.frequency
                desired.fields |= 0x00040000 | 0x00080000 | 0x00100000 | 0x00400000
            else:
                # Some virtual adapters omit modes they can nevertheless validate with CDS_TEST.
                desired = DevMode.from_buffer_copy(original)
                desired.width, desired.height = 1920, 1080
                desired.fields |= 0x00080000 | 0x00100000
            tested = api.ChangeDisplaySettingsExW(None, ctypes.byref(desired), None, 2, None)  # CDS_TEST
            notice({"tested": geometry(desired), "testResult": tested})
            assert tested == 0, "The virtual adapter does not support the requested wide display mode"
            applied = api.ChangeDisplaySettingsExW(None, ctypes.byref(desired), None, 0, None)
            assert applied == 0, f"Temporary display mode failed: {applied}"
            changed = True
            time.sleep(1)  # Settle the desktop before starting the unchanged application/suite.
            current = read(0xFFFFFFFF)
            assert current is not None and current.width >= 1600 and current.height >= 1050
            notice({"active": geometry(current)})
        yield
    finally:
        if changed:
            restored = api.ChangeDisplaySettingsExW(None, ctypes.byref(original), None, 0, None)
            notice({"restoreResult": restored, "original": geometry(original)})
            assert restored == 0, "Could not restore this disposable runner's original display mode"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--display", choices=["original", "wide"], required=True)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    assert command, "Provide the original test command after --"
    with display_mode(args.display == "wide"):
        result = subprocess.run(command, check=False)
    raise SystemExit(result.returncode)


if __name__ == "__main__":
    main()
