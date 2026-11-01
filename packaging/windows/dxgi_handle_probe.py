"""Check DXGI waitable-handle lifetime on an owned hidden CI window, without rendering.

Uses the public IDXGIFactory4 / IDXGISwapChain2 ABI and a fresh D3D12 WARP device.
No existing windows, desktop contents, user files, device names or handle values are reported.
"""

import ctypes
import json
import os
import uuid


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


def main():
    assert os.name == "nt" and os.environ.get("GITHUB_ACTIONS") == "true"
    assert os.environ.get("GITHUB_REPOSITORY") == "Pengjingyu1992/Kaleiform"
    assert ctypes.sizeof(SwapDesc) == 48 and ctypes.sizeof(Guid) == 16
    pointer = ctypes.c_void_p
    output = ctypes.POINTER(pointer)
    u32, hresult = ctypes.c_uint32, ctypes.c_int32
    kernel, user = ctypes.WinDLL("kernel32", use_last_error=True), ctypes.WinDLL(
        "user32", use_last_error=True
    )
    dxgi, d3d = ctypes.WinDLL("dxgi"), ctypes.WinDLL("d3d12")
    kernel.GetHandleInformation.argtypes = [pointer, ctypes.POINTER(u32)]
    kernel.GetHandleInformation.restype = ctypes.c_int32
    kernel.CloseHandle.argtypes = [pointer]
    kernel.CloseHandle.restype = ctypes.c_int32
    user.CreateWindowExW.argtypes = [
        u32,
        ctypes.c_wchar_p,
        ctypes.c_wchar_p,
        u32,
        ctypes.c_int32,
        ctypes.c_int32,
        ctypes.c_int32,
        ctypes.c_int32,
        pointer,
        pointer,
        pointer,
        pointer,
    ]
    user.CreateWindowExW.restype = pointer
    user.DestroyWindow.argtypes = [pointer]
    user.DestroyWindow.restype = ctypes.c_int32
    dxgi.CreateDXGIFactory2.argtypes = [u32, ctypes.POINTER(Guid), output]
    dxgi.CreateDXGIFactory2.restype = hresult
    d3d.D3D12CreateDevice.argtypes = [pointer, u32, ctypes.POINTER(Guid), output]
    d3d.D3D12CreateDevice.restype = hresult

    def valid(handle):
        flags = u32()
        return bool(handle and kernel.GetHandleInformation(handle, ctypes.byref(flags)))

    factory, adapter, device, queue, swap1, swap2 = (pointer() for _ in range(6))
    hwnd = None
    handles, closed = set(), set()
    result = {}
    try:
        hwnd = user.CreateWindowExW(
            0,
            "STATIC",
            "Kaleiform owned hidden handle test",
            0x00CF0000,
            0,
            0,
            640,
            480,
            None,
            None,
            None,
            None,
        )
        assert hwnd, "Could not create the owned hidden test window"
        factory_iid = Guid.parse("1bc6ea02-ef36-464f-bf0c-21ca39e5168a")
        check(dxgi.CreateDXGIFactory2(0, ctypes.byref(factory_iid), ctypes.byref(factory)), "Create factory")
        adapter_iid = Guid.parse("29038f61-3839-4626-91fd-086879011a05")
        check(
            method(factory, 27, hresult, ctypes.POINTER(Guid), output)(
                ctypes.byref(adapter_iid), ctypes.byref(adapter)
            ),
            "Get WARP adapter",
        )
        device_iid = Guid.parse("189819f1-1db6-4b57-be54-1821339b85f7")
        check(
            d3d.D3D12CreateDevice(adapter, 0xB000, ctypes.byref(device_iid), ctypes.byref(device)),
            "Create D3D12 WARP device",
        )
        queue_desc = (ctypes.c_int32 * 4)(0, 0, 0, 0)
        queue_iid = Guid.parse("0ec870a6-5d7e-4c22-8cfc-5baae07616ed")
        check(
            method(device, 8, hresult, pointer, ctypes.POINTER(Guid), output)(
                ctypes.byref(queue_desc), ctypes.byref(queue_iid), ctypes.byref(queue)
            ),
            "Create direct queue",
        )
        desc = SwapDesc(800, 600, 87, 0, (u32 * 2)(1, 0), 0x20, 2, 0, 4, 3, 0x40)
        create = method(
            factory, 15, hresult, pointer, pointer, ctypes.POINTER(SwapDesc), pointer, pointer, output
        )
        check(
            create(queue, hwnd, ctypes.byref(desc), None, None, ctypes.byref(swap1)),
            "Create HWND swap chain",
        )
        swap_iid = Guid.parse("a8be2ac4-199f-4946-b331-79599fb98de7")
        check(
            method(swap1, 0, hresult, ctypes.POINTER(Guid), output)(
                ctypes.byref(swap_iid), ctypes.byref(swap2)
            ),
            "Query swap chain 2",
        )
        check(method(swap2, 31, hresult, u32)(1), "Set frame latency")
        get = method(swap2, 33, pointer)
        resize = method(swap2, 13, hresult, u32, u32, u32, u32, u32)
        first, repeated = get(), get()
        handles.update([first, repeated])
        result.update(
            initialValid=valid(first), repeatedValid=valid(repeated), sameGetterHandle=first == repeated
        )
        check(resize(2, 960, 600, 87, 0x40), "Resize while retaining the waitable handle")
        retained = get()
        handles.add(retained)
        result.update(
            retainedAfterResize=valid(first),
            getterAfterRetainedResize=valid(retained),
            sameAfterRetainedResize=retained == first,
        )
        assert kernel.CloseHandle(first), "Could not close the test's first waitable handle"
        closed.add(first)
        result["closedHandleInvalid"] = not valid(first)
        resized = resize(2, 800, 600, 87, 0x40)
        result["resizeAfterCloseHresult"] = f"{resized & 0xFFFFFFFF:08x}"
        after_close = get()
        handles.add(after_close)
        result.update(getterAfterCloseValid=valid(after_close), sameValueAfterClose=after_close == first)
        result["outcome"] = "completed"
    except Exception as error:
        result.update(outcome="error", error=type(error).__name__ + ": " + str(error))
    finally:
        # Never close an already-closed numeric handle again, even if the OS has reused it.
        for handle in handles - closed:
            if valid(handle):
                kernel.CloseHandle(handle)
        for interface in (swap2, swap1, queue, device, adapter, factory):
            if interface:
                method(interface, 2, u32)()
        if hwnd:
            user.DestroyWindow(hwnd)
        print("::warning::DXGI wait handle lifetime: " + json.dumps(result), flush=True)
    assert result.get("outcome") == "completed", "DXGI probe could not complete"


if __name__ == "__main__":
    main()
