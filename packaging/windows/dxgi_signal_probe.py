"""Measure DXGI frame permits across resize on owned synthetic CI windows.

Uses the public IDXGIFactory4 / IDXGISwapChain2 ABI and a fresh D3D12 WARP device.
No existing windows, desktop contents, user files, device names or handle values are reported.
"""

import ctypes
import json
import os
import uuid
from ctypes import wintypes


class Guid(ctypes.Structure):
    _fields_ = [("bytes", ctypes.c_uint8 * 16)]

    @classmethod
    def parse(cls, value):
        return cls.from_buffer_copy(uuid.UUID(value).bytes_le)


class SwapDesc(ctypes.Structure):
    _fields_ = [
        ("width", ctypes.c_uint32),
        ("height", ctypes.c_uint32),
        ("format", ctypes.c_uint32),
        ("stereo", ctypes.c_int32),
        ("sample", ctypes.c_uint32 * 2),
        ("usage", ctypes.c_uint32),
        ("buffers", ctypes.c_uint32),
        ("scaling", ctypes.c_uint32),
        ("effect", ctypes.c_uint32),
        ("alpha", ctypes.c_uint32),
        ("flags", ctypes.c_uint32),
    ]


def check(value, label):
    if value < 0:
        raise RuntimeError(f"{label}: HRESULT {value & 0xFFFFFFFF:08x}")


def method(pointer, index, result, *arguments):
    table = ctypes.cast(pointer, ctypes.POINTER(ctypes.POINTER(ctypes.c_void_p))).contents
    address = table[index]
    assert address, "Missing COM entry point"
    native = ctypes.WINFUNCTYPE(result, ctypes.c_void_p, *arguments)(address)
    return lambda *args: native(pointer, *args)


def probe(mode):
    pointer, u32, u64 = ctypes.c_void_p, ctypes.c_uint32, ctypes.c_uint64
    output, hresult = ctypes.POINTER(pointer), ctypes.c_int32
    kernel, user = ctypes.WinDLL("kernel32", use_last_error=True), ctypes.WinDLL(
        "user32", use_last_error=True
    )
    dxgi, d3d = ctypes.WinDLL("dxgi"), ctypes.WinDLL("d3d12")
    kernel.CloseHandle.argtypes = [pointer]
    kernel.CloseHandle.restype = ctypes.c_int32
    kernel.WaitForSingleObject.argtypes = [pointer, u32]
    kernel.WaitForSingleObject.restype = u32
    kernel.CreateEventW.argtypes = [pointer, ctypes.c_int32, ctypes.c_int32, ctypes.c_wchar_p]
    kernel.CreateEventW.restype = pointer
    user.CreateWindowExW.argtypes = (
        [u32, ctypes.c_wchar_p, ctypes.c_wchar_p, u32] + [ctypes.c_int32] * 4 + [pointer] * 4
    )
    user.CreateWindowExW.restype = pointer
    user.ShowWindow.argtypes = [pointer, ctypes.c_int32]
    user.DestroyWindow.argtypes = [pointer]
    user.PeekMessageW.argtypes = [ctypes.POINTER(wintypes.MSG), pointer, u32, u32, u32]
    user.TranslateMessage.argtypes = [ctypes.POINTER(wintypes.MSG)]
    user.DispatchMessageW.argtypes = [ctypes.POINTER(wintypes.MSG)]
    user.DispatchMessageW.restype = ctypes.c_ssize_t
    dxgi.CreateDXGIFactory2.argtypes = [u32, ctypes.POINTER(Guid), output]
    dxgi.CreateDXGIFactory2.restype = hresult
    d3d.D3D12CreateDevice.argtypes = [pointer, u32, ctypes.POINTER(Guid), output]
    d3d.D3D12CreateDevice.restype = hresult
    factory, adapter, device, queue, swap1, swap2, fence = (pointer() for _ in range(7))
    hwnd, waitable, idle_event, fence_value = None, None, None, 0
    result = {"mode": mode, "frames": [], "resizes": [], "outcome": "starting"}

    def pump():
        message = wintypes.MSG()
        while user.PeekMessageW(ctypes.byref(message), None, 0, 0, 1):
            user.TranslateMessage(ctypes.byref(message))
            user.DispatchMessageW(ctypes.byref(message))

    def idle():
        nonlocal fence_value
        fence_value += 1
        check(method(queue, 14, hresult, pointer, u64)(fence, fence_value), "Signal present queue")
        check(method(fence, 9, hresult, u64, pointer)(fence_value, idle_event), "Set fence completion event")
        assert kernel.WaitForSingleObject(idle_event, 3000) == 0, "Present queue idle deadline"

    def frames(phase, count):
        for index in range(count):
            pump()
            waited = kernel.WaitForSingleObject(waitable, 1000)
            row = {"phase": phase, "index": index, "wait": int(waited)}
            if waited != 0:
                result["frames"].append(row)
                result["outcome"] = "frame-permit-timeout" if waited == 258 else "frame-wait-error"
                return False
            value = method(swap2, 8, hresult, u32, u32)(1, 0)
            row["present"] = f"{value & 0xFFFFFFFF:08x}"
            result["frames"].append(row)
            check(value, "Present the owned synthetic buffer")
            if value != 0:
                result["outcome"] = "present-status"
                return False
        return True

    try:
        hwnd = user.CreateWindowExW(
            0,
            "STATIC",
            "Kaleiform owned DXGI signal test",
            0x00CF0000,
            20,
            20,
            800,
            600,
            None,
            None,
            None,
            None,
        )
        assert hwnd, "Create owned synthetic window"
        user.ShowWindow(hwnd, 4)
        pump()
        iid = Guid.parse("1bc6ea02-ef36-464f-bf0c-21ca39e5168a")
        check(dxgi.CreateDXGIFactory2(0, ctypes.byref(iid), ctypes.byref(factory)), "Create factory")
        iid = Guid.parse("29038f61-3839-4626-91fd-086879011a05")
        check(
            method(factory, 27, hresult, ctypes.POINTER(Guid), output)(
                ctypes.byref(iid), ctypes.byref(adapter)
            ),
            "Get WARP adapter",
        )
        iid = Guid.parse("189819f1-1db6-4b57-be54-1821339b85f7")
        check(
            d3d.D3D12CreateDevice(adapter, 0xB000, ctypes.byref(iid), ctypes.byref(device)),
            "Create WARP device",
        )
        desc = (ctypes.c_int32 * 4)(0, 0, 0, 0)
        iid = Guid.parse("0ec870a6-5d7e-4c22-8cfc-5baae07616ed")
        check(
            method(device, 8, hresult, pointer, ctypes.POINTER(Guid), output)(
                ctypes.byref(desc), ctypes.byref(iid), ctypes.byref(queue)
            ),
            "Create presentation queue",
        )
        iid = Guid.parse("0a753dcf-c4d8-4b91-adf6-be5a60d95a76")
        check(
            method(device, 36, hresult, u64, u32, ctypes.POINTER(Guid), output)(
                0, 0, ctypes.byref(iid), ctypes.byref(fence)
            ),
            "Create idle fence",
        )
        idle_event = kernel.CreateEventW(None, 0, 0, None)
        assert idle_event, "Create owned idle event"
        desc = SwapDesc(800, 600, 87, 0, (u32 * 2)(1, 0), 0x20, 2, 0, 4, 3, 0x40)
        check(
            method(
                factory, 15, hresult, pointer, pointer, ctypes.POINTER(SwapDesc), pointer, pointer, output
            )(queue, hwnd, ctypes.byref(desc), None, None, ctypes.byref(swap1)),
            "Create swap chain",
        )
        iid = Guid.parse("a8be2ac4-199f-4946-b331-79599fb98de7")
        check(
            method(swap1, 0, hresult, ctypes.POINTER(Guid), output)(ctypes.byref(iid), ctypes.byref(swap2)),
            "Query swap chain 2",
        )
        latency = method(swap2, 31, hresult, u32)
        get = method(swap2, 33, pointer)
        resize = method(swap2, 13, hresult, u32, u32, u32, u32, u32)
        check(latency(1), "Set frame latency")
        waitable = get()
        assert waitable, "Get owned frame wait handle"
        if not frames("initial", 8):
            return result
        for index, (width, height) in enumerate([(950, 700), (800, 600), (720, 450)] * 2):
            idle()
            if mode == "reacquire":
                assert kernel.CloseHandle(waitable), "Close owned frame wait handle"
                waitable = None
            check(resize(2, width, height, 87, 0x40), "Resize swap-chain buffers")
            check(latency(1), "Reset frame latency")
            if mode == "reacquire":
                waitable = get()
                assert waitable, "Get replacement frame wait handle"
            result["resizes"].append({"index": index, "size": [width, height]})
            if not frames("resize-" + str(index), 4):
                return result
        result["outcome"] = "completed"
    except Exception as error:
        result.update(outcome="probe-error", error=type(error).__name__ + ": " + str(error))
    finally:
        if queue and fence and idle_event:
            try:
                idle()
            except Exception:
                result["cleanupQueueIdle"] = False
        for handle in (waitable, idle_event):
            if handle:
                kernel.CloseHandle(handle)
        for interface in (swap2, swap1, fence, queue, device, adapter, factory):
            if interface:
                method(interface, 2, u32)()
        if hwnd:
            user.DestroyWindow(hwnd)
    return result


def main():
    assert os.name == "nt" and os.environ.get("GITHUB_ACTIONS") == "true"
    assert os.environ.get("GITHUB_REPOSITORY") == "Pengjingyu1992/Kaleiform"
    assert ctypes.sizeof(SwapDesc) == 48 and ctypes.sizeof(Guid) == 16
    results = [probe(mode) for mode in ("retain", "reacquire")]
    print("::warning::Fresh DXGI frame-permit probe: " + json.dumps(results), flush=True)
    assert all(r["outcome"] != "probe-error" for r in results), "Native ABI probe could not complete"


if __name__ == "__main__":
    main()
