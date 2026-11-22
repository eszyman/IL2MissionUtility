"""Capture the IL-2 mission editor map and stitch it to one image.

The editor status bar under the map shows Mouse(m) and Camera (X Z Zoom). This
script reads those numbers, so the zoom you set is the zoom it records. The camera
is moved by the View menu command Move Camera to (the Ctrl+M item), posted
to the editor so a maximized window stays maximized. Alt+F9 is sent only by
the `init` command.

Usage
  py -3 tools/mapcapture/capture.py init
  py -3 tools/mapcapture/capture.py capture --limit 4
  py -3 tools/mapcapture/capture.py capture --yes
  py -3 tools/mapcapture/capture.py stitch
  py -3 tools/mapcapture/capture.py selftest

`init` is once per editor session, before you frame the shot: it presses
Alt+F9 twice. Then set the 2D view, zoom all the way in, and leave the marked
woods on screen. Hide icons and the grid. `capture` takes the mouse and the
keyboard. Press Esc to stop; run the same command to resume.

The picture is written to References/MapCapture/. Pixel (col, row) is
IL-2 Z = z0 + col * meters_per_pixel_z, X = x_north - row * meters_per_pixel_x.
X is meters north from the south edge. Z is meters east from the west edge.
"""

from __future__ import annotations

import argparse
import ctypes
import json
import math
import re
import subprocess
import sys
import time
import traceback
from ctypes import wintypes
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[2]
MASK_PATH = ROOT / "assets" / "combined_terrain.bin"
DEFAULT_OUT = ROOT / "References" / "MapCapture"
OCR_PS1 = Path(__file__).resolve().parent / "ocr.ps1"

MAP_MIN = 0.0
MAP_MAX = 499_200.0
SEA = (78, 112, 138)
VK_MENU = 0x12
VK_CONTROL = 0x11
VK_ESCAPE = 0x1B
VK_RETURN = 0x0D
VK_TAB = 0x09
VK_A = 0x41
VK_M = 0x4D
VK_F9 = 0x78
KEYUP = 0x0002
WM_SETTEXT = 0x000C
WM_GETTEXT = 0x000D
BM_CLICK = 0x00F5
SRCCOPY = 0x00CC0020


class CaptureError(RuntimeError):
    pass


def say(msg: str) -> None:
    print(msg, flush=True)


# --- geometry ---------------------------------------------------------------

def sample_bounds(cal: dict) -> tuple[float, float, float, float]:
    """A 2 by 2 block of views centered on the map.

    The full plan starts at the south edge, which is empty ground. A stitch
    check needs coast, rivers, or contours, and those sit near the middle.
    """
    _vx, _vy, vw, vh = cal["viewport"]
    overlap = float(cal["overlap"])
    span_z = vw * float(cal["mpp_z"])
    span_x = vh * float(cal["mpp_x"])
    # Two centers: one view, plus one step of (1 - overlap).
    width_x = span_x * (2.0 - overlap)
    width_z = span_z * (2.0 - overlap)
    mid = (MAP_MIN + MAP_MAX) / 2.0
    return (mid - width_x / 2.0, mid + width_x / 2.0, mid - width_z / 2.0, mid + width_z / 2.0)


def centers_for(span: float, overlap: float, start: float, end: float) -> list[float]:
    """Camera-center samples whose views cover [start, end]."""
    if end <= start:
        raise CaptureError("the map range is empty")
    if span <= 1:
        raise CaptureError("the map view is too small to measure")
    if span >= end - start:
        return [start + (end - start) / 2.0]
    step = span * (1.0 - overlap)
    if step <= 1:
        raise CaptureError("overlap leaves no step between captures")
    first = start + span / 2.0
    last = end - span / 2.0
    out = []
    c = first
    while c < last - 0.5:
        out.append(c)
        c += step
    out.append(last)
    return out


def tile_origin(cx: float, cz: float, anchor_col: float, anchor_row: float,
                mpp_z: float, mpp_x: float) -> tuple[float, float]:
    """World coordinate of the tile image's left and top pixels."""
    z_left = cz - anchor_col * mpp_z
    x_top = cx + anchor_row * mpp_x
    return z_left, x_top


def dest_pixel(z_left: float, x_top: float, origin_z: float, origin_x_north: float,
               mpp_z: float, mpp_x: float) -> tuple[int, int]:
    col = int(round((z_left - origin_z) / mpp_z))
    row = int(round((origin_x_north - x_top) / mpp_x))
    return col, row


def blit(mosaic: np.ndarray, tile: np.ndarray, dest_col: int, dest_row: int) -> None:
    height, width = mosaic.shape[:2]
    th, tw = tile.shape[:2]
    src_r, src_c = 0, 0
    if dest_col < 0:
        src_c = -dest_col
        dest_col = 0
    if dest_row < 0:
        src_r = -dest_row
        dest_row = 0
    if src_c >= tw or src_r >= th or dest_col >= width or dest_row >= height:
        return
    w2 = min(tw - src_c, width - dest_col)
    h2 = min(th - src_r, height - dest_row)
    if w2 <= 0 or h2 <= 0:
        return
    mosaic[dest_row:dest_row + h2, dest_col:dest_col + w2] = tile[src_r:src_r + h2, src_c:src_c + w2]


def hide_hud(img: np.ndarray, hud: tuple[int, int, int, int] | None) -> np.ndarray:
    """Cover the coordinate digits by stretching the column just left of them."""
    if not hud:
        return img
    x, y, w, h = hud
    out = img.copy()
    height, width = out.shape[:2]
    x0 = max(0, min(width, x))
    y0 = max(0, min(height, y))
    x1 = max(x0, min(width, x + w))
    y1 = max(y0, min(height, y + h))
    if x1 <= x0 or y1 <= y0:
        return out
    if y0 > 0:
        row = out[y0 - 1:y0, x0:x1]
        out[y0:y1, x0:x1] = np.repeat(row, y1 - y0, axis=0)
    elif x0 > 0:
        column = out[y0:y1, x0 - 1:x0]
        out[y0:y1, x0:x1] = np.repeat(column, x1 - x0, axis=1)
    return out


def image_translation(before: np.ndarray, after: np.ndarray) -> tuple[int, int, float]:
    """Roll `(dcol, drow)` that lines `after` up with `before`, and a peak score."""
    a = before.mean(axis=2).astype(np.float32)
    b = after.mean(axis=2).astype(np.float32)
    step = max(1, int(max(a.shape) // 700))
    a = a[::step, ::step]
    b = b[::step, ::step]
    if min(a.shape) < 16:
        return 0, 0, 0.0
    wy = np.hanning(a.shape[0]).astype(np.float32)
    wx = np.hanning(a.shape[1]).astype(np.float32)
    a = a * wy[:, None] * wx[None, :]
    b = b * wy[:, None] * wx[None, :]
    cross = np.fft.rfft2(a) * np.conj(np.fft.rfft2(b))
    cross /= np.abs(cross) + 1e-6
    corr = np.fft.irfft2(cross, s=a.shape)
    peak = np.unravel_index(int(np.argmax(corr)), corr.shape)
    drow, dcol = int(peak[0]), int(peak[1])
    if drow > a.shape[0] // 2:
        drow -= a.shape[0]
    if dcol > a.shape[1] // 2:
        dcol -= a.shape[1]
    quality = float(corr[peak] / (np.median(np.abs(corr)) + 1e-6))
    return dcol * step, drow * step, quality


def longest_run(indices: np.ndarray) -> tuple[int, int] | None:
    if indices.size == 0:
        return None
    best_l, best_r = int(indices[0]), int(indices[0])
    start = prev = int(indices[0])
    for value in indices[1:]:
        value = int(value)
        if value == prev + 1:
            prev = value
            continue
        if prev - start > best_r - best_l:
            best_l, best_r = start, prev
        start = prev = value
    if prev - start > best_r - best_l:
        best_l, best_r = start, prev
    return best_l, best_r


def changed_bbox(before: np.ndarray, after: np.ndarray, thresh: int = 14) -> tuple[int, int, int, int] | None:
    """Largest block of rows and columns the camera move changed."""
    delta = np.abs(before.astype(np.int16) - after.astype(np.int16)).max(axis=2)
    changed = delta > thresh
    row_hit = changed.mean(axis=1) > 0.08
    col_hit = changed.mean(axis=0) > 0.08
    rows = longest_run(np.flatnonzero(row_hit))
    cols = longest_run(np.flatnonzero(col_hit))
    if rows is None or cols is None:
        return None
    y0, y1 = rows
    x0, x1 = cols
    if y1 - y0 < 120 or x1 - x0 < 120:
        return None
    return x0, y0, x1 - x0 + 1, y1 - y0 + 1


def tiles_meet(west: np.ndarray, east: np.ndarray, step_px: float) -> bool:
    """True when `east` continues the same ground `step_px` to the right of `west`.

    The planned step comes from the measured scale. A zoom change after that
    measurement makes the pictures cover less ground than the step, so the
    shared edge is different terrain and the stitch tears.
    """
    if west.ndim != 3 or west.shape != east.shape:
        return False
    height, width = west.shape[:2]
    overlap = int(round(width - step_px))
    if overlap < 32 or overlap > width - 16:
        return False
    y0, y1 = 8, height - 8
    x_west = width - overlap + 12
    a = west[y0:y1, x_west : width - 12]
    if a.size == 0 or a.shape[1] < 16:
        return False
    best = 1e9
    for dy in range(-6, 7, 2):
        for dx in range(-6, 7, 2):
            yy, xx = y0 + dy, 12 + dx
            if yy < 0 or xx < 0 or yy + a.shape[0] > height or xx + a.shape[1] > width:
                continue
            b = east[yy : yy + a.shape[0], xx : xx + a.shape[1]]
            err = float(np.abs(a.astype(np.int16) - b.astype(np.int16)).mean())
            if err < best:
                best = err
    return best <= 8.0


def view_changed(previous: np.ndarray | None, shot: np.ndarray, thresh: float = 2.5) -> bool:
    """True when `shot` is not the same picture as `previous`.

    The first frame of a run has nothing to compare with. A later frame that
    still matches means Move Camera closed without applying the new point,
    or the camera was already on that point.
    """
    if previous is None or previous.shape != shot.shape:
        return True
    return float(np.abs(previous.astype(np.int16) - shot.astype(np.int16)).mean()) > thresh


def camera_at(status: dict, x: float, z: float, tol: float = 80.0) -> bool:
    """True when the status-bar camera is already the point Move Camera was given."""
    cam_x, cam_z = status.get("cam_x"), status.get("cam_z")
    if cam_x is None or cam_z is None:
        return False
    return abs(cam_x - x) <= tol and abs(cam_z - z) <= tol


def parse_numbers(text: str) -> list[float]:
    found = []
    for match in re.finditer(r"-?\d+(?:\.\d+)?", text):
        value = float(match.group())
        if -2_000.0 <= value <= MAP_MAX + 2_000.0:
            found.append(value)
    return found


def _coord(token: str) -> float:
    """A status-bar meter reading. A dropped decimal point is put back."""
    token = token.replace(",", ".").strip()
    if token.count(".") > 1:
        head, _, tail = token.rpartition(".")
        token = head.replace(".", "") + "." + tail
    value = float(token)
    if value > MAP_MAX + 5_000:
        scaled = value / 100.0
        if -2_000.0 <= scaled <= MAP_MAX + 5_000:
            return scaled
    return value


def parse_status(text: str) -> dict:
    """Mouse and Camera fields from the editor status bar.

    The bar reads ``Mouse(m): X Z`` and ``Camera (X Z Zoom): X Z zoom``.
    X is meters north from the south edge. Z is meters east from the west edge.
    """
    raw = text.replace(",", ".")
    out = {
        "mouse_x": None, "mouse_z": None,
        "cam_x": None, "cam_z": None, "zoom": None,
        "text": text,
    }
    mouse = re.search(
        r"(?:Mo?u?se|Ise)\s*\([^)]*\)\s*:?\s*([0-9.]+)\s+([0-9.]+)",
        raw, re.I,
    )
    cam = re.search(
        r"Camera\s*\([^)]*\)\s*:?\s*([0-9.]+)\s+([0-9.]+)\s+([0-9.]+)",
        raw, re.I,
    )
    if mouse:
        out["mouse_x"] = _coord(mouse.group(1))
        out["mouse_z"] = _coord(mouse.group(2))
    if cam:
        out["cam_x"] = _coord(cam.group(1))
        out["cam_z"] = _coord(cam.group(2))
        zoom = float(cam.group(3))
        if zoom > 50:
            zoom /= 1000.0
        out["zoom"] = zoom
    # Windows OCR often drops the closing parenthesis and the colon, which
    # leaves "Mouse(m) 250490.96 213914.47" and "Zoom 250000.00 253000.00 2.758".
    if out["mouse_x"] is None:
        nums = _labeled_numbers(raw, r"(?:Mo?u?se|Ise)\b", r"Grid|Camera")
        if len(nums) >= 2:
            out["mouse_x"] = _coord(nums[0])
            out["mouse_z"] = _coord(nums[1])
    if out["cam_x"] is None:
        nums = _labeled_numbers(raw, r"Camera\b", r"$")
        if len(nums) >= 3:
            out["cam_x"] = _coord(nums[0])
            out["cam_z"] = _coord(nums[1])
            zoom = float(nums[2])
            if zoom > 50:
                zoom /= 1000.0
            out["zoom"] = zoom
    return out


def _labeled_numbers(text: str, label: str, stop: str) -> list[str]:
    """Digit groups after a status label, before the next label."""
    match = re.search(label + r"(.*?)(?:" + stop + r")", text, re.I | re.S)
    if not match:
        return []
    return re.findall(r"\d+(?:\.\d+)?", match.group(1))


def status_strip(image: np.ndarray) -> np.ndarray:
    """The status bar along the bottom of an editor capture.

    The bar is a short bright band under the map. At the south edge the map
    itself is a light beige, still brighter than 200, so a flat threshold
    keeps that whole field. The object counts on the right are then read as
    the camera Z. The bar starts at a near-white separator and is only about
    24 rows tall.
    """
    row = image.mean(axis=(1, 2))
    height = len(row)
    if height < 16:
        return image
    floor = max(0, height - 48)
    window = row[floor:]
    bright = np.flatnonzero(window > 250)
    if bright.size:
        top = floor + int(bright[0])
        if height - top > 40:
            top = height - 32
        return image[top:]
    bg = float(np.percentile(row[-12:], 75))
    y = height - 1
    limit = max(0, height - 36)
    while y > limit and row[y] > bg - 30:
        y -= 1
    top = min(y + 1, height - 8)
    return image[top:]


class TerrainMask:
    def __init__(self, width: int, height: int, grid: bytes, map_max: float = MAP_MAX):
        self.width = width
        self.height = height
        self.grid = grid
        self.map_max = map_max

    @classmethod
    def load(cls, path: Path = MASK_PATH) -> "TerrainMask":
        data = path.read_bytes()
        if data[:4] != b"WMAP":
            raise CaptureError(f"{path} is not a terrain mask")
        width, height = int.from_bytes(data[4:8], "little"), int.from_bytes(data[8:12], "little")
        grid = data[12:]
        if len(grid) < width * height:
            raise CaptureError(f"{path} is truncated")
        return cls(width, height, grid)

    def has_land(self, x0: float, x1: float, z0: float, z1: float, samples: int = 6) -> bool:
        if x1 < x0:
            x0, x1 = x1, x0
        if z1 < z0:
            z0, z1 = z1, z0
        pad = 200.0
        x0, x1 = x0 - pad, x1 + pad
        z0, z1 = z0 - pad, z1 + pad
        for i in range(samples):
            for j in range(samples):
                x = x0 + (x1 - x0) * (i + 0.5) / samples
                z = z0 + (z1 - z0) * (j + 0.5) / samples
                if self._land(x, z):
                    return True
        return False

    def _land(self, x: float, z: float) -> bool:
        if self.map_max <= 0:
            return False
        u = z / self.map_max
        v = (self.map_max - x) / self.map_max
        if not (0.0 <= u < 1.0 and 0.0 <= v < 1.0):
            return False
        gx = min(self.width - 1, int(u * self.width))
        gy = min(self.height - 1, int(v * self.height))
        return (self.grid[gy * self.width + gx] & 1) == 0


# --- windows ----------------------------------------------------------------

def _win32():
    user32 = ctypes.windll.user32
    gdi32 = ctypes.windll.gdi32
    kernel32 = ctypes.windll.kernel32
    user32.GetWindowTextLengthW.argtypes = [ctypes.c_void_p]
    user32.GetWindowTextLengthW.restype = ctypes.c_int
    user32.GetWindowTextW.argtypes = [ctypes.c_void_p, ctypes.c_wchar_p, ctypes.c_int]
    user32.GetWindowTextW.restype = ctypes.c_int
    user32.GetClassNameW.argtypes = [ctypes.c_void_p, ctypes.c_wchar_p, ctypes.c_int]
    user32.IsWindowVisible.argtypes = [ctypes.c_void_p]
    user32.IsWindowVisible.restype = ctypes.c_int
    user32.SetForegroundWindow.argtypes = [ctypes.c_void_p]
    user32.GetForegroundWindow.restype = ctypes.c_void_p
    user32.ShowWindow.argtypes = [ctypes.c_void_p, ctypes.c_int]
    user32.BringWindowToTop.argtypes = [ctypes.c_void_p]
    user32.GetWindowThreadProcessId.argtypes = [ctypes.c_void_p, ctypes.POINTER(wintypes.DWORD)]
    user32.GetWindowThreadProcessId.restype = wintypes.DWORD
    user32.keybd_event.argtypes = [ctypes.c_ubyte, ctypes.c_ubyte, wintypes.DWORD, ctypes.c_void_p]
    user32.SetCursorPos.argtypes = [ctypes.c_int, ctypes.c_int]
    user32.mouse_event.argtypes = [wintypes.DWORD, wintypes.DWORD, wintypes.DWORD, wintypes.DWORD, ctypes.c_void_p]
    user32.GetClientRect.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
    user32.GetWindowRect.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
    user32.ClientToScreen.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
    user32.GetDC.argtypes = [ctypes.c_void_p]
    user32.GetDC.restype = ctypes.c_void_p
    user32.ReleaseDC.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
    user32.SendMessageW.argtypes = [ctypes.c_void_p, wintypes.UINT, ctypes.c_void_p, ctypes.c_wchar_p]
    user32.SendMessageW.restype = ctypes.c_void_p
    user32.GetAsyncKeyState.argtypes = [ctypes.c_int]
    user32.GetAsyncKeyState.restype = ctypes.c_short
    gdi32.CreateCompatibleDC.argtypes = [ctypes.c_void_p]
    gdi32.CreateCompatibleDC.restype = ctypes.c_void_p
    gdi32.CreateCompatibleBitmap.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int]
    gdi32.CreateCompatibleBitmap.restype = ctypes.c_void_p
    gdi32.SelectObject.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
    gdi32.SelectObject.restype = ctypes.c_void_p
    gdi32.BitBlt.argtypes = [
        ctypes.c_void_p, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int,
        ctypes.c_void_p, ctypes.c_int, ctypes.c_int, wintypes.DWORD,
    ]
    gdi32.GetDIBits.argtypes = [
        ctypes.c_void_p, ctypes.c_void_p, wintypes.UINT, wintypes.UINT,
        ctypes.c_void_p, ctypes.c_void_p, wintypes.UINT,
    ]
    gdi32.DeleteObject.argtypes = [ctypes.c_void_p]
    gdi32.DeleteDC.argtypes = [ctypes.c_void_p]
    kernel32.GetCurrentThreadId.restype = wintypes.DWORD
    user32.AttachThreadInput.argtypes = [wintypes.DWORD, wintypes.DWORD, wintypes.BOOL]
    return user32, gdi32, kernel32


class RECT(ctypes.Structure):
    _fields_ = [("left", ctypes.c_long), ("top", ctypes.c_long),
                ("right", ctypes.c_long), ("bottom", ctypes.c_long)]


class POINT(ctypes.Structure):
    _fields_ = [("x", ctypes.c_long), ("y", ctypes.c_long)]


class BITMAPINFOHEADER(ctypes.Structure):
    _fields_ = [
        ("biSize", wintypes.DWORD), ("biWidth", ctypes.c_long), ("biHeight", ctypes.c_long),
        ("biPlanes", wintypes.WORD), ("biBitCount", wintypes.WORD), ("biCompression", wintypes.DWORD),
        ("biSizeImage", wintypes.DWORD), ("biXPelsPerMeter", ctypes.c_long),
        ("biYPelsPerMeter", ctypes.c_long), ("biClrUsed", wintypes.DWORD),
        ("biClrImportant", wintypes.DWORD),
    ]


class BITMAPINFO(ctypes.Structure):
    _fields_ = [("bmiHeader", BITMAPINFOHEADER), ("bmiColors", wintypes.DWORD * 3)]


def dpi_aware() -> None:
    try:
        ctypes.windll.shcore.SetProcessDpiAwareness(2)
    except Exception:
        try:
            ctypes.windll.user32.SetProcessDPIAware()
        except Exception:
            pass


def key_down(vk: int) -> bool:
    user32, _, _ = _win32()
    return bool(user32.GetAsyncKeyState(vk) & 0x8000)


def wait_key_up(vk: int) -> None:
    while key_down(vk):
        time.sleep(0.02)


def tap(vk: int) -> None:
    user32, _, _ = _win32()
    user32.keybd_event(vk, 0, 0, None)
    time.sleep(0.02)
    user32.keybd_event(vk, 0, KEYUP, None)
    time.sleep(0.04)


def chord(vk: int, ctrl: bool = False, alt: bool = False) -> None:
    user32, _, _ = _win32()
    if alt:
        user32.keybd_event(VK_MENU, 0, 0, None)
    if ctrl:
        user32.keybd_event(VK_CONTROL, 0, 0, None)
    time.sleep(0.02)
    try:
        tap(vk)
    finally:
        if ctrl:
            user32.keybd_event(VK_CONTROL, 0, KEYUP, None)
        if alt:
            user32.keybd_event(VK_MENU, 0, KEYUP, None)
        time.sleep(0.04)


def window_title(hwnd: int) -> str:
    user32, _, _ = _win32()
    length = user32.GetWindowTextLengthW(hwnd)
    buf = ctypes.create_unicode_buffer(length + 1)
    user32.GetWindowTextW(hwnd, buf, length + 1)
    return buf.value


def window_class(hwnd: int) -> str:
    user32, _, _ = _win32()
    buf = ctypes.create_unicode_buffer(256)
    user32.GetClassNameW(hwnd, buf, 256)
    return buf.value


def window_pid(hwnd: int) -> int:
    user32, _, _ = _win32()
    pid = wintypes.DWORD()
    user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
    return int(pid.value)


def window_rect(hwnd: int) -> tuple[int, int, int, int]:
    user32, _, _ = _win32()
    rect = RECT()
    if not user32.GetWindowRect(hwnd, ctypes.byref(rect)):
        return (0, 0, 0, 0)
    return rect.left, rect.top, rect.right, rect.bottom


def _enum(callback, parent: int | None = None) -> None:
    user32, _, _ = _win32()
    proto = ctypes.WINFUNCTYPE(wintypes.BOOL, ctypes.c_void_p, ctypes.c_void_p)
    kept = proto(callback)
    if parent is None:
        user32.EnumWindows(kept, 0)
    else:
        user32.EnumChildWindows(parent, kept, 0)


def visible_hwnds(editor: int | None = None) -> set[int]:
    user32, _, _ = _win32()
    found: set[int] = set()

    def top(hwnd, _lp):
        if user32.IsWindowVisible(hwnd):
            found.add(int(hwnd))
        return True

    _enum(top)
    if editor:
        def child(hwnd, _lp):
            if user32.IsWindowVisible(hwnd):
                found.add(int(hwnd))
            return True
        _enum(child, editor)
    return found


def find_editor(needle: str) -> tuple[int, str]:
    user32, _, _ = _win32()
    hits: list[tuple[int, int, str]] = []

    def top(hwnd, _lp):
        if not user32.IsWindowVisible(hwnd):
            return True
        title = window_title(int(hwnd))
        if needle.lower() in title.lower():
            left, top_, right, bottom = window_rect(int(hwnd))
            hits.append(((right - left) * (bottom - top_), int(hwnd), title))
        return True

    _enum(top)
    if not hits:
        raise CaptureError(
            f'no visible window title contains "{needle}". '
            "Open an empty Korea mission and leave the editor in front."
        )
    hits.sort(reverse=True)
    return hits[0][1], hits[0][2]


def thread_of(hwnd: int) -> int:
    if not hwnd:
        return 0
    user32, _, _ = _win32()
    pid = wintypes.DWORD()
    return int(user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid)))


def focus_window(hwnd: int, required: bool = True) -> bool:
    """Bring a window forward without restoring it. Restore shrinks a maximized window.

    A Shift key-up satisfies Windows' foreground lock without opening the editor
    menu. Alt would. A later tile still has to run if Windows will not give up
    the foreground, so callers that can fall back pass required=False.
    """
    user32, _, kernel32 = _win32()
    user32.IsIconic.argtypes = [ctypes.c_void_p]
    user32.IsIconic.restype = wintypes.BOOL
    if int(user32.GetForegroundWindow() or 0) == int(hwnd):
        return True
    current = kernel32.GetCurrentThreadId()
    for _ in range(2):
        fg = int(user32.GetForegroundWindow() or 0)
        fg_thread = thread_of(fg)
        target = thread_of(hwnd)
        attached = []
        if fg_thread and fg_thread != current:
            user32.AttachThreadInput(current, fg_thread, True)
            attached.append(fg_thread)
        if target and target != current and target not in attached:
            user32.AttachThreadInput(current, target, True)
            attached.append(target)
        try:
            if user32.IsIconic(hwnd):
                user32.ShowWindow(hwnd, 9)
            # KEYEVENTF_KEYUP. A full Alt press opens the menu, so this is Shift.
            user32.keybd_event(0x10, 0, 0x0002, 0)
            user32.BringWindowToTop(hwnd)
            user32.SetForegroundWindow(hwnd)
        finally:
            for thread in attached:
                user32.AttachThreadInput(current, thread, False)
        for _ in range(12):
            if int(user32.GetForegroundWindow() or 0) == int(hwnd):
                time.sleep(0.08)
                return True
            time.sleep(0.04)
    if required:
        raise CaptureError("could not bring the editor to the front. Click it, then run again.")
    return False


def menu_command(hwnd: int, label: str) -> int | None:
    """Menu item id whose text starts with `label`, ignoring the & and the shortcut."""
    user32, _, _ = _win32()
    user32.GetMenu.argtypes = [ctypes.c_void_p]
    user32.GetMenu.restype = ctypes.c_void_p
    user32.GetMenuItemCount.argtypes = [ctypes.c_void_p]
    user32.GetMenuItemCount.restype = ctypes.c_int
    user32.GetSubMenu.argtypes = [ctypes.c_void_p, ctypes.c_int]
    user32.GetSubMenu.restype = ctypes.c_void_p
    user32.GetMenuStringW.argtypes = [ctypes.c_void_p, wintypes.UINT, ctypes.c_wchar_p, ctypes.c_int, wintypes.UINT]
    user32.GetMenuStringW.restype = ctypes.c_int
    user32.GetMenuItemID.argtypes = [ctypes.c_void_p, ctypes.c_int]
    user32.GetMenuItemID.restype = wintypes.UINT
    root = user32.GetMenu(hwnd)
    if not root:
        return None
    want = label.lower()
    found: list[int] = []

    def walk(hmenu) -> None:
        count = user32.GetMenuItemCount(hmenu)
        for index in range(count):
            buf = ctypes.create_unicode_buffer(256)
            user32.GetMenuStringW(hmenu, index, buf, 256, 0x400)
            text = buf.value.split("\t")[0].replace("&", "").strip().lower()
            item = int(user32.GetMenuItemID(hmenu, index))
            if text.startswith(want) and 0 < item < 0xFFFFFFFF:
                found.append(item)
            sub = user32.GetSubMenu(hmenu, index)
            if sub:
                walk(sub)

    walk(root)
    return found[0] if found else None


def _send_ptr(hwnd: int, msg: int, wparam: int, lparam: int) -> None:
    """SendMessageW with a pointer lParam.

    The shared prototype passes lParam as text, which cannot carry a control
    handle. EN_CHANGE needs the edit's hwnd in lParam.
    """
    proto = ctypes.WINFUNCTYPE(
        ctypes.c_ssize_t, ctypes.c_void_p, wintypes.UINT, ctypes.c_void_p, ctypes.c_void_p,
    )
    send = proto(("SendMessageW", ctypes.windll.user32))
    send(hwnd, msg, wparam, lparam)


def notify_edit(edit: int) -> None:
    """Tell the dialog the field changed. WM_SETTEXT alone leaves the old value cached."""
    user32, _, _ = _win32()
    user32.GetDlgCtrlID.argtypes = [ctypes.c_void_p]
    user32.GetDlgCtrlID.restype = ctypes.c_int
    user32.GetParent.argtypes = [ctypes.c_void_p]
    user32.GetParent.restype = ctypes.c_void_p
    user32.SendMessageW(edit, 0x00B9, 1, None)  # EM_SETMODIFY
    ctrl = int(user32.GetDlgCtrlID(edit))
    parent = int(user32.GetParent(edit) or 0)
    if parent and ctrl >= 0:
        _send_ptr(parent, 0x0111, (0x0300 << 16) | (ctrl & 0xFFFF), edit)  # EN_CHANGE


def post_command(hwnd: int, command: int) -> None:
    user32, _, _ = _win32()
    user32.PostMessageW.argtypes = [ctypes.c_void_p, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
    user32.PostMessageW.restype = wintypes.BOOL
    if not user32.PostMessageW(hwnd, 0x0111, command, 0):
        raise CaptureError("could not send the Move Camera command to the editor")


def client_origin(hwnd: int) -> tuple[int, int, int, int]:
    user32, _, _ = _win32()
    rect = RECT()
    if not user32.GetClientRect(hwnd, ctypes.byref(rect)):
        raise CaptureError("could not read the editor client area")
    pt = POINT(0, 0)
    if not user32.ClientToScreen(hwnd, ctypes.byref(pt)):
        raise CaptureError("could not map the editor onto the screen")
    return pt.x, pt.y, rect.right - rect.left, rect.bottom - rect.top


def grab_screen(left: int, top: int, width: int, height: int) -> np.ndarray:
    user32, gdi32, _ = _win32()
    if width < 1 or height < 1:
        raise CaptureError("the capture rectangle is empty")
    hdc = user32.GetDC(0)
    mem = gdi32.CreateCompatibleDC(hdc)
    bmp = gdi32.CreateCompatibleBitmap(hdc, width, height)
    old = gdi32.SelectObject(mem, bmp)
    try:
        if not gdi32.BitBlt(mem, 0, 0, width, height, hdc, left, top, SRCCOPY):
            raise CaptureError("screen capture failed")
        info = BITMAPINFO()
        info.bmiHeader.biSize = ctypes.sizeof(BITMAPINFOHEADER)
        info.bmiHeader.biWidth = width
        info.bmiHeader.biHeight = -height
        info.bmiHeader.biPlanes = 1
        info.bmiHeader.biBitCount = 32
        info.bmiHeader.biCompression = 0
        buf = ctypes.create_string_buffer(width * height * 4)
        lines = gdi32.GetDIBits(mem, bmp, 0, height, buf, ctypes.byref(info), 0)
        if lines == 0:
            raise CaptureError("could not read the captured pixels")
        raw = np.frombuffer(buf, dtype=np.uint8).reshape((height, width, 4))
        return raw[:, :, :3][:, :, ::-1].copy()
    finally:
        gdi32.SelectObject(mem, old)
        gdi32.DeleteObject(bmp)
        gdi32.DeleteDC(mem)
        user32.ReleaseDC(0, hdc)


def set_clipboard(text: str) -> None:
    user32 = ctypes.windll.user32
    kernel32 = ctypes.windll.kernel32
    user32.OpenClipboard.argtypes = [ctypes.c_void_p]
    user32.EmptyClipboard.restype = wintypes.BOOL
    user32.SetClipboardData.argtypes = [wintypes.UINT, ctypes.c_void_p]
    user32.SetClipboardData.restype = ctypes.c_void_p
    user32.CloseClipboard.restype = wintypes.BOOL
    kernel32.GlobalAlloc.argtypes = [wintypes.UINT, ctypes.c_size_t]
    kernel32.GlobalAlloc.restype = ctypes.c_void_p
    kernel32.GlobalLock.argtypes = [ctypes.c_void_p]
    kernel32.GlobalLock.restype = ctypes.c_void_p
    kernel32.GlobalUnlock.argtypes = [ctypes.c_void_p]
    kernel32.GlobalFree.argtypes = [ctypes.c_void_p]
    wide = (text + "\x00").encode("utf-16-le")
    opened = False
    for _ in range(8):
        if user32.OpenClipboard(None):
            opened = True
            break
        time.sleep(0.03)
    if not opened:
        raise CaptureError("could not open the clipboard")
    try:
        user32.EmptyClipboard()
        mem = kernel32.GlobalAlloc(0x0002, len(wide))
        if not mem:
            raise CaptureError("could not allocate clipboard memory")
        ptr = kernel32.GlobalLock(mem)
        if not ptr:
            kernel32.GlobalFree(mem)
            raise CaptureError("could not lock clipboard memory")
        ctypes.memmove(ptr, wide, len(wide))
        kernel32.GlobalUnlock(mem)
        if not user32.SetClipboardData(13, mem):
            kernel32.GlobalFree(mem)
            raise CaptureError("could not put the coordinate on the clipboard")
    finally:
        user32.CloseClipboard()


def paste_text(text: str) -> None:
    set_clipboard(text)
    time.sleep(0.05)
    chord(0x56, ctrl=True)  # Ctrl+V


def edit_text(hwnd: int) -> str:
    user32, _, _ = _win32()
    buf = ctypes.create_unicode_buffer(128)
    user32.SendMessageW(hwnd, WM_GETTEXT, 128, buf)
    return buf.value


def _send_uint(hwnd: int, msg: int, wparam: int, lparam: int) -> int:
    proto = ctypes.WINFUNCTYPE(
        ctypes.c_ssize_t, ctypes.c_void_p, wintypes.UINT, ctypes.c_size_t, ctypes.c_ssize_t,
    )
    send = proto(("SendMessageW", ctypes.windll.user32))
    return int(send(hwnd, msg, wparam, lparam))


def set_edit_text(hwnd: int, text: str) -> bool:
    user32, _, _ = _win32()
    user32.SendMessageW(hwnd, WM_SETTEXT, 0, text)
    # Selecting and replacing makes the edit send EN_CHANGE itself. WM_SETTEXT
    # updates the letters and leaves the dialog's cached point alone.
    _send_uint(hwnd, 0x00B1, 0, -1)  # EM_SETSEL
    user32.SendMessageW(hwnd, 0x00C2, 1, text)  # EM_REPLACESEL
    time.sleep(0.05)
    try:
        stuck = abs(float(edit_text(hwnd).replace(",", "")) - float(text)) < 0.5
    except ValueError:
        return False
    if stuck:
        notify_edit(hwnd)
    return stuck


def child_controls(parent: int) -> list[int]:
    found: list[int] = []

    def child(hwnd, _lp):
        found.append(int(hwnd))
        return True

    _enum(child, parent)
    return found


def control_point(hwnd: int) -> tuple[int, int]:
    left, top, _right, _bottom = window_rect(hwnd)
    return top, left


def mission_properties_rects(editor: int) -> list[tuple[int, int, int, int]]:
    user32, _, _ = _win32()
    rects = []

    def consider(hwnd, _lp):
        if int(hwnd) == int(editor) or not user32.IsWindowVisible(hwnd):
            return True
        title = window_title(int(hwnd)).lower()
        if "mission properties" not in title:
            return True
        left, top, right, bottom = window_rect(int(hwnd))
        rects.append((left - 8, top - 8, right + 8, bottom + 8))
        return True

    _enum(consider)
    _enum(consider, editor)
    return rects


def click_view(editor: int) -> None:
    """One click in the view, clear of Mission Properties, so keys reach the map."""
    user32, _, _ = _win32()
    sx, sy, width, height = client_origin(editor)
    if width < 64 or height < 64:
        return
    blocked = mission_properties_rects(editor)
    candidates = [
        (width // 4, height // 2),
        (width // 8, height // 2),
        (width // 2, height // 2),
        (width // 4, height // 4),
    ]
    for cx, cy in candidates:
        x, y = sx + cx, sy + cy
        if any(l <= x < r and t <= y < b for l, t, r, b in blocked):
            continue
        user32.SetCursorPos(x, y)
        time.sleep(0.05)
        user32.mouse_event(0x0002, 0, 0, 0, None)
        user32.mouse_event(0x0004, 0, 0, 0, None)
        time.sleep(0.15)
        return
    say("Could not click the map without hitting Mission Properties. Move that panel off the map.")


def ocr_image(path: Path) -> str:
    # A visible PowerShell window takes the foreground. The next Move Camera
    # then closes on the old point because the editor is no longer in front.
    flags = getattr(subprocess, "CREATE_NO_WINDOW", 0)
    completed = subprocess.run(
        [
            "powershell", "-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden",
            "-ExecutionPolicy", "Bypass", "-File", str(OCR_PS1), "-Path", str(path),
        ],
        capture_output=True, text=True, timeout=30, creationflags=flags,
    )
    if completed.returncode != 0:
        detail = (completed.stderr or completed.stdout or "").strip()
        raise CaptureError(f"Windows OCR failed: {detail}")
    return completed.stdout.strip()


def _ocr_padded(img: Image.Image, path: Path) -> str:
    """White margin. A crop that touches the image edge loses the first digit."""
    pad = 28
    canvas = Image.new("RGB", (img.width + pad * 2, img.height + pad * 2), (255, 255, 255))
    canvas.paste(img, (pad, pad))
    canvas.save(path)
    return ocr_image(path)


def read_status(image: np.ndarray, save_path: Path) -> dict:
    """Read Mouse and Camera from the status bar of one editor capture.

    The bar is already black on white. Stretching the contrast turns ".00"
    into "oo", so the crop is sent to OCR unchanged. A right-hand crop is the
    fallback when the full bar drops a number.
    """
    strip = status_strip(image)
    width = strip.shape[1]
    merged = {
        "mouse_x": None, "mouse_z": None,
        "cam_x": None, "cam_z": None, "zoom": None,
        "text": "",
    }
    crops = [strip, strip[:, int(width * 0.55):]]
    for index, crop in enumerate(crops):
        img = Image.fromarray(np.ascontiguousarray(crop))
        if index:
            img = img.resize((img.width * 3, img.height * 3), Image.Resampling.LANCZOS)
        elif img.height < 64:
            # The bar is only ~24 px. Windows OCR drops it unless it is enlarged.
            img = img.resize((img.width * 2, img.height * 2), Image.Resampling.LANCZOS)
        path = save_path if index == 0 else save_path.with_name(f"{save_path.stem}_{index}.png")
        parsed = parse_status(_ocr_padded(img, path))
        merged["text"] += (" | " if merged["text"] else "") + parsed["text"]
        for key in ("mouse_x", "mouse_z", "cam_x", "cam_z", "zoom"):
            if merged[key] is None and parsed[key] is not None:
                merged[key] = parsed[key]
        if None not in (merged["cam_x"], merged["cam_z"], merged["mouse_x"], merged["mouse_z"]):
            break
    save_path.with_suffix(".txt").write_text(merged["text"], encoding="utf-8")
    return merged


# --- editor session ----------------------------------------------------------

class Session:
    def __init__(self, out: Path, window: str):
        self.out = out
        self.window = window
        self.hwnd = 0
        self.title = ""
        self.swap = False
        self.debug = out / "debug"
        self.debug.mkdir(parents=True, exist_ok=True)

    def connect(self) -> None:
        needles = [self.window] if self.window else ["IL2 Series Editor", "Mission Editor"]
        last = None
        for needle in needles:
            try:
                self.hwnd, self.title = find_editor(needle)
                break
            except CaptureError as exc:
                last = exc
        if not self.hwnd:
            raise last or CaptureError("the mission editor is not open")
        say(f"Editor: {self.title}")
        focus_window(self.hwnd)

    def client(self) -> tuple[int, int, int, int]:
        return client_origin(self.hwnd)

    def park_mouse(self) -> None:
        user32, _, _ = _win32()
        left, top, right, _bottom = window_rect(self.hwnd)
        user32.SetCursorPos((left + right) // 2, top + 12)
        time.sleep(0.05)

    def grab_client(self) -> np.ndarray:
        self.park_mouse()
        time.sleep(0.08)
        sx, sy, w, h = self.client()
        return grab_screen(sx, sy, w, h)

    def alt_f9_twice(self) -> None:
        focus_window(self.hwnd)
        wait_key_up(VK_ESCAPE)
        tap(VK_ESCAPE)
        time.sleep(0.2)
        click_view(self.hwnd)
        say("Pressing Alt+F9 twice.")
        chord(VK_F9, alt=True)
        time.sleep(0.7)
        chord(VK_F9, alt=True)
        time.sleep(0.8)

    def prepare_view(self) -> None:
        focus_window(self.hwnd)
        wait_key_up(VK_ESCAPE)
        tap(VK_ESCAPE)
        time.sleep(0.25)
        click_view(self.hwnd)

    def _dialog_from(self, before: set[int]) -> int:
        user32, _, _ = _win32()
        time.sleep(0.35)
        deadline = time.time() + 2.0
        chosen = 0
        while time.time() < deadline:
            fresh = []
            for hwnd in visible_hwnds(self.hwnd) - before:
                if hwnd == self.hwnd or not user32.IsWindowVisible(hwnd):
                    continue
                left, top, right, bottom = window_rect(hwnd)
                area = max(0, right - left) * max(0, bottom - top)
                title = window_title(hwnd)
                if area < 4_000:
                    continue
                fresh.append((title, hwnd, area))
            ranked = []
            for title, hwnd, area in fresh:
                title_l = title.lower()
                edits = [h for h in child_controls(hwnd) if "edit" in window_class(h).lower()]
                score = 0
                if "move" in title_l or "camera" in title_l:
                    score += 2
                if len(edits) >= 2:
                    score += 2
                if score:
                    ranked.append((-score, area, hwnd))
            if ranked:
                ranked.sort()
                chosen = ranked[0][2]
                break
            time.sleep(0.08)
        if not chosen:
            names = [window_title(h) for h in visible_hwnds(self.hwnd) - before]
            raise CaptureError(
                "Move Camera to did not open, so no coordinates were typed. "
                f"New windows: {', '.join(n for n in names if n) or 'none'}."
            )
        return chosen

    def _fields_match(self, edits: list[int], first: int, second: int) -> bool:
        try:
            got = [float(edit_text(h).replace(",", "")) for h in edits[:2]]
        except ValueError:
            return False
        return abs(got[0] - first) < 0.5 and abs(got[1] - second) < 0.5

    def _type_fields(self, edits: list[int], first: int, second: int) -> None:
        """Paste into each box. Tab from X lands on OK, not on Z."""
        user32, _, _ = _win32()
        user32.SetFocus.argtypes = [ctypes.c_void_p]
        for edit, value in ((edits[0], first), (edits[1], second)):
            user32.SetFocus(edit)
            time.sleep(0.05)
            chord(VK_A, ctrl=True)
            paste_text(str(value))
            notify_edit(edit)

    def _press_ok(self, dialog: int, last_edit: int) -> None:
        buttons = [h for h in child_controls(dialog) if window_class(h).lower() == "button"]
        ok = next((h for h in buttons if window_title(h).replace("&", "").strip().lower() == "ok"), 0)
        user32, _, _ = _win32()
        if ok:
            # The last edit has to lose focus or the dialog keeps the old point.
            # Posted, so the dialog's own loop handles the click. SendMessage
            # BM_CLICK runs inside our call and can close the box without applying.
            if last_edit:
                _send_uint(last_edit, 0x0008, ok, 0)  # WM_KILLFOCUS
            user32.PostMessageW.argtypes = [ctypes.c_void_p, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
            user32.PostMessageW.restype = wintypes.BOOL
            user32.PostMessageW(ok, 0x00F5, 0, 0)  # BM_CLICK
        else:
            focus_window(dialog, required=False)
            tap(VK_RETURN)

    def _fill_dialog(self, dialog: int, first: int, second: int) -> None:
        edits = [h for h in child_controls(dialog) if "edit" in window_class(h).lower()]
        edits.sort(key=control_point)
        if len(edits) < 2:
            focus_window(dialog, required=False)
            tap(VK_ESCAPE)
            raise CaptureError("Move Camera to did not show an X field and a Z field.")
        for _ in range(4):
            if set_edit_text(edits[0], str(first)) and set_edit_text(edits[1], str(second)):
                break
            time.sleep(0.08)
        if not self._fields_match(edits, first, second):
            focus_window(dialog, required=False)
            self._type_fields(edits, first, second)
        if not self._fields_match(edits, first, second):
            focus_window(dialog, required=False)
            tap(VK_ESCAPE)
            raise CaptureError(
                f"Move Camera kept the previous point instead of X {first}, Z {second}."
            )
        focus_window(dialog, required=False)
        self._press_ok(dialog, edits[1])
        deadline = time.time() + 2.0
        while time.time() < deadline and ctypes.windll.user32.IsWindowVisible(dialog):
            time.sleep(0.05)
        if ctypes.windll.user32.IsWindowVisible(dialog):
            focus_window(dialog, required=False)
            tap(VK_RETURN)
            time.sleep(0.3)
        if ctypes.windll.user32.IsWindowVisible(dialog):
            focus_window(dialog, required=False)
            tap(VK_ESCAPE)
            raise CaptureError("the Move Camera dialog stayed open. Nothing further was captured.")

    def _dismiss_move_dialog(self) -> None:
        """Close a Move Camera box left open by the previous try."""
        for hwnd in visible_hwnds(self.hwnd):
            title = window_title(hwnd).lower()
            if hwnd != self.hwnd and "move" in title and "camera" in title:
                focus_window(hwnd, required=False)
                tap(VK_ESCAPE)
                time.sleep(0.2)
                return

    def move_camera(self, x: float, z: float) -> None:
        # X is the top field, Z the bottom field, unless calibration swapped them.
        first, second = (int(round(z)), int(round(x))) if self.swap else (int(round(x)), int(round(z)))
        # The status-bar read between tiles starts PowerShell and can leave the
        # editor in the background. A dialog opened there keeps the old point.
        focus_window(self.hwnd, required=False)
        self._dismiss_move_dialog()
        command = menu_command(self.hwnd, "Move Camera to")
        if command is None:
            raise CaptureError("the editor View menu has no Move Camera to command")
        before = visible_hwnds(self.hwnd)
        post_command(self.hwnd, command)
        dialog = self._dialog_from(before)
        focus_window(dialog, required=False)
        self._fill_dialog(dialog, first, second)
        time.sleep(0.25)

    def move_until_changed(self, x: float, z: float, previous: np.ndarray | None) -> np.ndarray:
        """Open Move Camera until the view shows that point.

        A point the camera is already on does not change the picture. Waiting
        for a pixel change there sits in the screen grab until something else,
        such as Alt-tab, alters the screen. The status bar is the check.
        """
        caught: CaptureError | None = None
        for attempt in range(3):
            self.move_camera(x, z)
            shot = self.wait_stable(previous)
            user32, _, _ = _win32()
            if int(user32.GetForegroundWindow() or 0) != int(self.hwnd):
                # The grab is the screen. A console in front hides the move.
                focus_window(self.hwnd, required=False)
                shot = self.wait_stable(None)
            if view_changed(previous, shot):
                return shot
            # Blank ground at the south edge does not change pixels. The status
            # bar is what shows the camera actually arrived.
            say(f"Picture unchanged. Checking the camera for X {x:.0f}, Z {z:.0f}.")
            # Blank ground does not change pixels, and the status numbers can
            # update a moment after the grab that decided nothing moved.
            for _ in range(4):
                if self._camera_at(shot, x, z) or (
                    previous is not None and self._camera_at(previous, x, z)
                ):
                    return shot
                time.sleep(0.4)
                shot = self.grab_client()
            caught = CaptureError("the camera did not move. The Move Camera values were not applied.")
            if attempt == 2:
                raise caught
            say(f"Move Camera did not apply X {x:.0f}, Z {z:.0f}. Trying again.")
        raise caught or CaptureError("the camera did not move. The Move Camera values were not applied.")

    def _camera_at(self, shot: np.ndarray, x: float, z: float) -> bool:
        status = read_status(shot, self.debug / "status_here.png")
        return camera_at(status, x, z)

    def wait_stable(self, previous: np.ndarray | None = None, timeout: float = 8.0) -> np.ndarray:
        """Wait until the view stops changing.

        A move over detailed ground returns as soon as the new picture settles.
        Blank ground never changes, so a short calm stretch returns too and the
        caller reads the status bar. The wait is long enough that a terrain
        redraw has already started if one is coming.
        """
        time.sleep(0.25)
        shot = self.grab_client()
        started = time.time()
        calm = 0
        while time.time() - started < timeout:
            if key_down(VK_ESCAPE):
                raise CaptureError("stopped")
            time.sleep(0.22)
            nxt = self.grab_client()
            diff = float(np.abs(shot.astype(np.int16) - nxt.astype(np.int16)).mean())
            shot = nxt
            calm = calm + 1 if diff < 1.4 else 0
            if calm < 2:
                continue
            if previous is None or view_changed(previous, shot):
                return shot
            if time.time() - started >= 1.6:
                return shot
        return shot

    def put_mouse_client(self, cx: int, cy: int) -> tuple[int, int]:
        user32, _, _ = _win32()
        sx, sy, _, _ = self.client()
        user32.SetCursorPos(sx + cx, sy + cy)
        time.sleep(0.22)
        return sx + cx, sy + cy


def properties_cover(editor: int, viewport: tuple[int, int, int, int]) -> bool:
    sx, sy, _, _ = client_origin(editor)
    vx, vy, vw, vh = viewport
    view = (sx + vx, sy + vy, sx + vx + vw, sy + vy + vh)
    for left, top, right, bottom in mission_properties_rects(editor):
        ix = max(0, min(view[2], right) - max(view[0], left))
        iy = max(0, min(view[3], bottom) - max(view[1], top))
        if ix * iy > 0.05 * vw * vh:
            return True
    return False


def calibrate(session: Session) -> dict:
    say("Measuring the view at the current zoom.")
    session.swap = False
    cap_a = session.move_until_changed(250_000, 250_000, None)
    cap_b = session.move_until_changed(250_000, 253_000, cap_a)
    Image.fromarray(cap_a).save(session.debug / "cap_a.png")
    Image.fromarray(cap_b).save(session.debug / "cap_b.png")
    box = changed_bbox(cap_a, cap_b)
    if box is None:
        dcol, drow, quality = image_translation(cap_a, cap_b)
        raise CaptureError(
            "the map area could not be found after Move Camera "
            f"(image shift {dcol}, {drow}, quality {quality:.1f}). "
            "Leave the 2D map visible, with nothing covering it."
        )
    vx, vy, vw, vh = box
    if properties_cover(session.hwnd, box):
        raise CaptureError("Mission Properties covers the map. Move that panel aside and run again.")
    say(f"Map view is {vw}×{vh} px inside the editor window.")

    anchor = (vx + vw // 2, vy + vh // 2)
    dx = int(min(400, max(80, vw // 5)))
    dy = int(min(400, max(80, vh // 5)))
    right = (min(vx + vw - 40, anchor[0] + dx), anchor[1])
    down = (anchor[0], min(vy + vh - 40, anchor[1] + dy))
    dx = right[0] - anchor[0]
    dy = down[1] - anchor[1]
    if dx < 24 or dy < 24:
        raise CaptureError("the map view is too small to measure. Make the editor window larger.")

    placed = read_status(cap_b, session.debug / "status_camera.png")
    if placed["cam_x"] is None or placed["cam_z"] is None:
        raise CaptureError(
            "could not read Camera (X Z Zoom) on the status bar. "
            f"The crop is in {session.debug}."
        )
    as_typed = abs(placed["cam_x"] - 250_000) + abs(placed["cam_z"] - 253_000)
    swapped = abs(placed["cam_z"] - 250_000) + abs(placed["cam_x"] - 253_000)
    if min(as_typed, swapped) > 2_500:
        raise CaptureError(
            f"the status bar camera reads X {placed['cam_x']:.0f}, Z {placed['cam_z']:.0f} "
            "after a move to 250000, 253000."
        )
    session.swap = swapped < as_typed
    if session.swap:
        say("Move Camera's second field is north/south. The first field is east/west.")
        cap_b = session.move_until_changed(260_000, 270_000, cap_b)
        placed = read_status(cap_b, session.debug / "status_camera.png")
        if placed["cam_x"] is None or abs(placed["cam_x"] - 260_000) > 2_500 or abs(placed["cam_z"] - 270_000) > 2_500:
            raise CaptureError("could not re-read the camera after correcting the axes")

    def pointer_at(point, stem) -> tuple[float, float]:
        session.put_mouse_client(*point)
        sx, sy, w, h = session.client()
        frame = grab_screen(sx, sy, w, h)
        status = read_status(frame, session.debug / f"{stem}.png")
        if status["mouse_x"] is None or status["mouse_z"] is None:
            raise CaptureError(
                "could not read Mouse(m) on the status bar. "
                f"The crop is in {session.debug}."
            )
        return status["mouse_x"], status["mouse_z"], status

    mx, mz, _at_center = pointer_at(anchor, "status_center")
    rx, rz, _at_right = pointer_at(right, "status_right")
    dx_m, dz_m = rx - mx, rz - mz
    # The east/west probe should change Z. A north/south probe should change X.
    if abs(dz_m) < abs(dx_m):
        raise CaptureError(
            "moving the pointer right changed north/south more than east/west. "
            "Use the flat 2D view, with north at the top."
        )
    mpp_z = dz_m / dx
    bx, bz, _at_down = pointer_at(down, "status_down")
    mpp_x = (mx - bx) / dy
    if mpp_z <= 0.05 or mpp_x <= 0.05:
        raise CaptureError(
            f"the measured scale is {mpp_z:.3f} m/px east and {mpp_x:.3f} m/px north. "
            "Use the flat 2D view, with north at the top."
        )
    if max(mpp_z, mpp_x) / min(mpp_z, mpp_x) > 1.08:
        say(f"East and north scales differ ({mpp_z:.3f} and {mpp_x:.3f} m/px). The view may not be straight overhead.")
    offset_x = mx - placed["cam_x"]
    offset_z = mz - placed["cam_z"]
    if abs(offset_x) > 20_000 or abs(offset_z) > 20_000:
        raise CaptureError(
            f"the view center is {offset_x:.0f} m north and {offset_z:.0f} m east of the camera readout."
        )
    say(
        f"Scale is {mpp_z:.2f} m/px east and {mpp_x:.2f} m/px north. "
        f"Camera X {placed['cam_x']:.0f}, Z {placed['cam_z']:.0f}. "
        f"The view center is {offset_x:.0f} m, {offset_z:.0f} m from that."
    )
    _sx, _sy, cw, ch = session.client()
    # The coordinates live on the status bar, under the map, so they are not in the picture.
    return {
        "window": session.title,
        "client": [cw, ch],
        "viewport": [vx, vy, vw, vh],
        "anchor": [anchor[0] - vx, anchor[1] - vy],
        "mpp_x": mpp_x,
        "mpp_z": mpp_z,
        "offset_x": offset_x,
        "offset_z": offset_z,
        "swap": session.swap,
        "hud": None,
        "overlap": 0.10,
        "zoom": placed.get("zoom"),
    }


def build_plan(cal: dict, mask: TerrainMask, bounds: tuple[float, float, float, float],
               include_sea: bool) -> list[dict]:
    vx, vy, vw, vh = cal["viewport"]
    del vx, vy
    mpp_z, mpp_x = cal["mpp_z"], cal["mpp_x"]
    overlap = cal["overlap"]
    x0, x1, z0, z1 = bounds
    span_z = vw * mpp_z
    span_x = vh * mpp_x
    xs = centers_for(span_x, overlap, x0, x1)
    zs = centers_for(span_z, overlap, z0, z1)
    anchor_col, anchor_row = cal["anchor"]
    plan = []
    for row, cx in enumerate(xs):
        for col, cz in enumerate(zs):
            z_left, x_top = tile_origin(cx, cz, anchor_col, anchor_row, mpp_z, mpp_x)
            land = mask.has_land(x_top - span_x, x_top, z_left, z_left + span_z)
            plan.append({
                "row": row,
                "col": col,
                "cx": cx,
                "cz": cz,
                "land": land,
                "capture": bool(include_sea or land),
            })
    return plan


def save_json(path: Path, data) -> None:
    path.write_text(json.dumps(data, indent=2), encoding="utf-8")


def load_index(path: Path) -> dict[tuple[int, int], dict]:
    done = {}
    if not path.exists():
        return done
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            rec = json.loads(line)
        except json.JSONDecodeError:
            continue
        done[(int(rec["row"]), int(rec["col"]))] = rec
    return done


def capture_tiles(session: Session, cal: dict, plan: list[dict], limit: int | None) -> None:
    index_path = session.out / "tiles" / "index.jsonl"
    index_path.parent.mkdir(parents=True, exist_ok=True)
    done = load_index(index_path)
    pending = [tile for tile in plan if tile["capture"] and (tile["row"], tile["col"]) not in done]
    if limit is not None:
        pending = pending[:limit]
    if not pending:
        say("Every planned tile is already captured.")
        return
    say(f"Capturing {len(pending)} tiles. Press Esc to stop and keep what is done.")
    wait_key_up(VK_ESCAPE)
    anchor_col, anchor_row = cal["anchor"]
    mpp_z, mpp_x = cal["mpp_z"], cal["mpp_x"]
    vx, vy, vw, vh = cal["viewport"]
    offset_x, offset_z = cal["offset_x"], cal["offset_z"]
    checks = {0, len(pending) // 2, len(pending) - 1}
    # Compare with the picture already on screen, so a dialog that closes on
    # the old point is not accepted as a successful move.
    previous = session.grab_client()
    stop = session.out / "STOP"
    if stop.exists():
        stop.unlink()
    with index_path.open("a", encoding="utf-8") as index:
        for n, tile in enumerate(pending):
            if key_down(VK_ESCAPE) or stop.exists():
                say("Stopped. Run the same command to resume.")
                return
            cx, cz = tile["cx"], tile["cz"]
            # The typed point is offset from the view center. Ask for the center we want.
            shot = session.move_until_changed(cx - offset_x, cz - offset_z, previous)
            previous = shot
            if n in checks:
                cx, cz = _check_landing(session, cal, shot, cx, cz, n)
            view = shot[vy:vy + vh, vx:vx + vw]
            if float(view.mean()) < 4:
                raise CaptureError("the map image is black. Leave the editor visible on the screen, not covered and not minimized.")
            west_key = (tile["row"], tile["col"] - 1)
            west = done.get(west_key)
            if west is not None:
                west_path = session.out / "tiles" / west["file"]
                if west_path.exists():
                    west_view = np.asarray(Image.open(west_path).convert("RGB"))
                    step_px = (cz - west["cz"]) / mpp_z
                    if not tiles_meet(west_view, view, step_px):
                        raise CaptureError(
                            "this picture does not meet the tile to its west. "
                            "The zoom changed after the scale was measured, so the photos leave gaps and the stitch tears. "
                            "Leave the zoom where you want it. Move this capture folder aside, then run the same command with --recalibrate."
                        )
            name = f"r{tile['row']:04d}_c{tile['col']:04d}.png"
            Image.fromarray(view).save(session.out / "tiles" / name)
            z_left, x_top = tile_origin(cx, cz, anchor_col, anchor_row, mpp_z, mpp_x)
            rec = {
                "row": tile["row"], "col": tile["col"], "cx": cx, "cz": cz,
                "file": name, "z_left": z_left, "x_top": x_top,
            }
            index.write(json.dumps(rec) + "\n")
            index.flush()
            done[(tile["row"], tile["col"])] = rec
            say(f"{n + 1}/{len(pending)}  X {cx:.0f}  Z {cz:.0f}")
    say("Capture finished.")


def _check_landing(session: Session, cal: dict, shot: np.ndarray, cx: float, cz: float, n: int) -> tuple[float, float]:
    status = read_status(shot, session.debug / f"check_{n}.png")
    if status["cam_x"] is None or status["cam_z"] is None:
        say("Could not read the camera on the status bar. Keeping the requested position.")
        return cx, cz
    typed_x = cx - cal["offset_x"]
    typed_z = cz - cal["offset_z"]
    err_x = status["cam_x"] - typed_x
    err_z = status["cam_z"] - typed_z
    if abs(err_x) <= 40 and abs(err_z) <= 40:
        return cx, cz
    if abs(err_x) <= 2_500 and abs(err_z) <= 2_500:
        say(
            f"Camera reads X {status['cam_x']:.0f}, Z {status['cam_z']:.0f} "
            f"(asked for {typed_x:.0f}, {typed_z:.0f}). Using the readout."
        )
        return cx + err_x, cz + err_z
    say(
        f"Camera X {status['cam_x']:.0f}, Z {status['cam_z']:.0f} does not match "
        f"{typed_x:.0f}, {typed_z:.0f}. Keeping the request."
    )
    return cx, cz


def stitch(out: Path, max_gb: float, preview_edge: int = 4096) -> Path | None:
    cal = json.loads((out / "calibration.json").read_text(encoding="utf-8"))
    plan_meta = json.loads((out / "plan.json").read_text(encoding="utf-8"))
    done = load_index(out / "tiles" / "index.jsonl")
    if not done:
        raise CaptureError("there are no captured tiles to stitch")
    bounds = plan_meta["bounds"]
    full_ready = _coverage_complete(plan_meta["tiles"], done)
    if full_ready:
        region = bounds
        label = "korea_editor_map"
    else:
        region = _bounds_of(done.values(), cal)
        label = "korea_editor_map_sample"
        say("Some tiles are still missing, so this stitch covers only what has been captured.")
    path = _write_mosaic(out, cal, done, region, label, max_gb, preview_edge)
    return path


def _coverage_complete(plan: list[dict], done: dict) -> bool:
    needed = [(t["row"], t["col"]) for t in plan if t["capture"]]
    return bool(needed) and all(key in done for key in needed)


def _bounds_of(records, cal: dict) -> list[float]:
    mpp_z, mpp_x = cal["mpp_z"], cal["mpp_x"]
    vw, vh = cal["viewport"][2], cal["viewport"][3]
    xs, zs = [], []
    for rec in records:
        z0, x1 = rec["z_left"], rec["x_top"]
        zs.extend((z0, z0 + vw * mpp_z))
        xs.extend((x1 - vh * mpp_x, x1))
    return [min(xs), max(xs), min(zs), max(zs)]


def _write_mosaic(out: Path, cal: dict, done: dict, region: list[float], label: str,
                  max_gb: float, preview_edge: int) -> Path | None:
    x0, x1, z0, z1 = region
    mpp_z, mpp_x = float(cal["mpp_z"]), float(cal["mpp_x"])
    width = max(1, int(math.ceil((z1 - z0) / mpp_z - 1e-6)))
    height = max(1, int(math.ceil((x1 - x0) / mpp_x - 1e-6)))
    nbytes = width * height * 3
    say(f"Mosaic {width}×{height} px, {nbytes / 1e9:.2f} GB uncompressed.")
    hud = tuple(cal["hud"]) if cal.get("hud") else None
    # North and west tiles go down first. A later tile then covers the
    # coordinate digits in the lower-right corner of the tile before it.
    records = sorted(done.values(), key=lambda rec: (-rec["row"], rec["col"]))

    def paste_into(mosaic):
        for rec in records:
            tile_path = out / "tiles" / rec["file"]
            if not tile_path.exists():
                continue
            tile = np.asarray(Image.open(tile_path).convert("RGB"))
            tile = hide_hud(tile, hud)
            col, row = dest_pixel(rec["z_left"], rec["x_top"], z0, x1, mpp_z, mpp_x)
            blit(mosaic, tile, col, row)

    georef = {
        "il2_x_north": x1,
        "il2_z_west": z0,
        "width": width,
        "height": height,
        "meters_per_pixel_x": mpp_x,
        "meters_per_pixel_z": mpp_z,
        "note": "col, row -> Z = il2_z_west + col * meters_per_pixel_z; "
                "X = il2_x_north - row * meters_per_pixel_x",
    }
    save_json(out / f"{label}.json", georef)

    preview = _preview_array(records, out, cal, region, hud, preview_edge)
    preview_path = out / f"{label}_preview.png"
    Image.fromarray(preview).save(preview_path)
    say(f"Preview: {preview_path}")

    if nbytes <= 800_000_000:
        mosaic = np.empty((height, width, 3), np.uint8)
        mosaic[:] = SEA
        paste_into(mosaic)
        dest = out / f"{label}.png"
        Image.fromarray(mosaic).save(dest)
        say(f"Image: {dest}")
        return dest
    if nbytes <= max_gb * 1e9:
        import tifffile
        dest = out / f"{label}.tif"
        mosaic = tifffile.memmap(dest, shape=(height, width, 3), dtype=np.uint8, photometric="rgb")
        sea = np.empty((1, width, 3), np.uint8)
        sea[:] = SEA
        for y in range(0, height, 64):
            mosaic[y:y + 64] = sea
        paste_into(mosaic)
        mosaic.flush()
        say(f"Image: {dest}")
        return dest
    say(
        f"The full image is {nbytes / 1e9:.1f} GB, above the {max_gb:g} GB limit. "
        "The tiles are the full detail. Raise it with: stitch --max-gb N"
    )
    return preview_path


def _preview_array(records, out: Path, cal: dict, region: list[float], hud, edge: int) -> np.ndarray:
    x0, x1, z0, z1 = region
    mpp_z, mpp_x = float(cal["mpp_z"]), float(cal["mpp_x"])
    full_w = max(1, int(math.ceil((z1 - z0) / mpp_z)))
    full_h = max(1, int(math.ceil((x1 - x0) / mpp_x)))
    scale = min(1.0, edge / max(full_w, full_h))
    pw, ph = max(1, int(round(full_w * scale))), max(1, int(round(full_h * scale)))
    preview = np.empty((ph, pw, 3), np.uint8)
    preview[:] = SEA
    for rec in records:
        tile_path = out / "tiles" / rec["file"]
        if not tile_path.exists():
            continue
        tile = hide_hud(np.asarray(Image.open(tile_path).convert("RGB")), hud)
        col, row = dest_pixel(rec["z_left"], rec["x_top"], z0, x1, mpp_z, mpp_x)
        small = np.asarray(Image.fromarray(tile).resize(
            (max(1, int(round(tile.shape[1] * scale))), max(1, int(round(tile.shape[0] * scale)))),
            Image.Resampling.BOX,
        ))
        blit(preview, small, int(round(col * scale)), int(round(row * scale)))
    return preview


def estimate(cal: dict, plan: list[dict]) -> str:
    pending_n = sum(1 for t in plan if t["capture"])
    vw, vh = cal["viewport"][2], cal["viewport"][3]
    mpp_z, mpp_x = cal["mpp_z"], cal["mpp_x"]
    span_z, span_x = vw * mpp_z, vh * mpp_x
    seconds = pending_n * 4.0
    hours, minutes = divmod(int(seconds // 60), 60)
    tile_gb = pending_n * vw * vh * 3 / 5 / 1e9
    return (
        f"Each view covers about {span_z / 1000:.2f} km east-west and {span_x / 1000:.2f} km north-south. "
        f"{pending_n} tiles, about {hours} h {minutes:02d} min, and about {tile_gb:.1f} GB of tile files "
        f"before the stitched image."
    )


def confirm(count: int, yes: bool) -> None:
    if count <= 12 or yes:
        return
    if not sys.stdin.isatty():
        raise CaptureError("this capture is long. Pass --yes to start it.")
    say("Press Enter to start, or Ctrl+C to cancel.")
    input()
    wait_key_up(VK_RETURN)


def countdown() -> None:
    wait_key_up(VK_ESCAPE)
    for left in (3, 2, 1):
        if key_down(VK_ESCAPE):
            raise CaptureError("stopped")
        say(f"Starting in {left}. Leave the mouse and keyboard alone. Esc stops the run.")
        time.sleep(1.0)


def cmd_init(args) -> None:
    session = Session(args.out, args.window)
    session.connect()
    session.alt_f9_twice()
    say("Alt+F9 has been pressed twice.")
    say("Set the flat 2D view and zoom all the way in. Leave the marked woods visible.")
    say("Hide icons and the grid, and move Mission Properties off the map.")
    say("Then run: py -3 tools/mapcapture/capture.py capture --sample")


def cmd_capture(args) -> None:
    out = args.out / "sample" if args.sample else args.out
    out.mkdir(parents=True, exist_ok=True)
    session = Session(out, args.window)
    session.connect()
    if args.init:
        session.alt_f9_twice()
        say("Zoom and the woods view were just reset by Alt+F9. Set them again, then run capture without --init.")
        return
    bounds = [args.x0, args.x1, args.z0, args.z1]
    cal_path = out / "calibration.json"
    plan_path = out / "plan.json"
    if args.recalibrate or not cal_path.exists() or not plan_path.exists():
        session.prepare_view()
        countdown()
        cal = calibrate(session)
        sx = session.client()
        cal["client"] = [sx[2], sx[3]]
        mask = TerrainMask.load()
        if args.sample:
            bounds = list(sample_bounds(cal))
            say(
                f"Sample is the middle of the map: "
                f"X {bounds[0]:.0f}..{bounds[1]:.0f}, Z {bounds[2]:.0f}..{bounds[3]:.0f}."
            )
        plan = build_plan(cal, mask, tuple(bounds), args.include_sea)
        save_json(cal_path, cal)
        save_json(plan_path, {"bounds": bounds, "tiles": plan, "include_sea": args.include_sea})
    else:
        cal = json.loads(cal_path.read_text(encoding="utf-8"))
        stored = json.loads(plan_path.read_text(encoding="utf-8"))
        plan = stored["tiles"]
        session.swap = bool(cal["swap"])
        cw, ch = session.client()[2:]
        if cal["client"] != [cw, ch]:
            raise CaptureError(
                "the editor window size changed since the measurement. "
                "Run capture --recalibrate after the window is back where you want it."
            )
        say("Resuming the existing measurement. Pass --recalibrate to measure the zoom again.")
    say(estimate(cal, plan))
    remaining = [t for t in plan if t["capture"] and (t["row"], t["col"]) not in load_index(out / "tiles" / "index.jsonl")]
    take = len(remaining) if args.limit is None else min(args.limit, len(remaining))
    say(f"{take} tiles will be captured now. {len(remaining)} are not done yet.")
    if take:
        confirm(take, args.yes)
        countdown()
    capture_tiles(session, cal, plan, args.limit)
    stitch(out, args.max_gb)


def cmd_stitch(args) -> None:
    if not (args.out / "calibration.json").exists():
        raise CaptureError("nothing has been captured yet")
    stitch(args.out, args.max_gb)


def selftest() -> None:
    centers = centers_for(1_000, 0.2, 0, 10_000)
    assert centers[0] == 500
    assert centers[-1] == 9_500
    assert all(b - a <= 1_000 for a, b in zip(centers, centers[1:]))
    assert centers_for(20_000, 0.1, 0, 10_000) == [5_000]

    z_left, x_top = tile_origin(100, 200, 10, 5, 2, 4)
    assert z_left == 180
    assert x_top == 120
    assert dest_pixel(z_left, x_top, 0, 200, 2, 4) == (90, 20)

    mosaic = np.zeros((10, 10, 3), np.uint8)
    tile = np.arange(16, dtype=np.uint8).reshape(4, 4, 1).repeat(3, axis=2)
    blit(mosaic, tile, -1, -1)
    assert mosaic[0, 0, 0] == tile[1, 1, 0]
    blit(mosaic, tile, 8, 8)
    assert mosaic[8, 8, 0] == tile[0, 0, 0]
    assert mosaic[9, 9, 0] == tile[1, 1, 0]

    hud = np.zeros((6, 8, 3), np.uint8)
    hud[0, :] = 3
    hud[1:, 6:] = 9
    covered = hide_hud(hud, (6, 1, 2, 4))
    assert np.all(covered[1:5, 6:] == 3)

    base = np.zeros((80, 120, 3), np.uint8)
    base[20:60, 30:90] = 40
    rolled = np.roll(np.roll(base, -6, axis=0), 8, axis=1)
    dcol, drow, quality = image_translation(base, rolled)
    assert (dcol, drow) == (-8, 6), (dcol, drow, quality)
    assert quality > 5

    same = np.zeros((8, 8, 3), np.uint8)
    assert view_changed(None, same)
    assert not view_changed(same, same.copy())
    assert camera_at({"cam_x": 4265.0, "cam_z": 18998.2}, 4265, 18998)
    assert not camera_at({"cam_x": 4265.0, "cam_z": 6785.0}, 4265, 18998)
    assert not camera_at({"cam_x": None, "cam_z": 18998.0}, 4265, 18998)
    nudged = same.copy()
    nudged[:2, :2] = 255
    assert view_changed(same, nudged)
    # The southwest tile and the next one east are different camera points.
    span_z = 1435 * 9.456376306620246
    span_x = 901 * 9.481166666666589
    xs = centers_for(span_x, 0.1, 0, MAP_MAX)
    zs = centers_for(span_z, 0.1, 0, MAP_MAX)
    assert abs(xs[0] - 4271.27) < 1
    assert abs(zs[0] - 6784.95) < 1
    assert zs[1] - zs[0] > 10_000
    box = sample_bounds({
        "viewport": [279, 39, 1435, 901],
        "mpp_x": 9.481166666666589,
        "mpp_z": 9.456376306620246,
        "overlap": 0.1,
    })
    mid_x = centers_for(901 * 9.481166666666589, 0.1, box[0], box[1])
    mid_z = centers_for(1435 * 9.456376306620246, 0.1, box[2], box[3])
    assert len(mid_x) == 2 and len(mid_z) == 2
    assert abs((mid_x[0] + mid_x[1]) / 2 - 249_600) < 1
    assert abs((mid_z[0] + mid_z[1]) / 2 - 249_600) < 1

    ground = np.random.default_rng(1).integers(0, 255, (180, 900, 3), dtype=np.uint8)
    west_view = ground[:, 0:600]
    assert tiles_meet(west_view, ground[:, 180:780], 180)
    slipped = ground[:, 182:782]
    assert tiles_meet(west_view, slipped, 180)
    assert not tiles_meet(west_view, ground[:, 320:920], 180)

    noise = np.random.default_rng(0).integers(0, 255, (240, 320, 3), dtype=np.uint8)
    shifted = np.roll(noise, 15, axis=1)
    box = changed_bbox(noise, shifted, thresh=8)
    assert box is not None and box[2] > 200 and box[3] > 150

    assert parse_numbers("X 124500   Z 305800")[-2:] == [124500, 305800]
    assert parse_numbers("nope") == []
    bar = parse_status(
        "Ready Mouse(m): 250054.15 24498855 Grid(m): 5000 Camera (X Z Zoom): 250000.00 250000.00 1,292 (2)"
    )
    assert abs(bar["mouse_x"] - 250054.15) < 0.02
    assert abs(bar["mouse_z"] - 244988.55) < 0.02
    assert bar["cam_x"] == 250_000 and bar["cam_z"] == 250_000
    assert abs(bar["zoom"] - 1.292) < 0.001
    moved = parse_status(
        "Ise(rn): 250054.15 244988.55 Grid(m): 5000 Camera (X Z Zoom): 250000.00 253000.00 1.292 (2)"
    )
    assert abs(moved["cam_z"] - 253_000) < 0.02
    assert abs(moved["mouse_z"] - 244988.55) < 0.02
    live = parse_status(
        "Mouse (m): 250050.29 245345.49 Grid (m): SODO Camera (X Z Zoom): 250000.00 253000.00 1.222 (2)"
    )
    assert abs(live["mouse_x"] - 250050.29) < 0.02
    assert abs(live["mouse_z"] - 245345.49) < 0.02
    assert live["cam_x"] == 250_000 and live["cam_z"] == 253_000
    assert abs(live["zoom"] - 1.222) < 0.001
    dropped = parse_status(
        "Ready Mouse(m) 250490.96 213914.47 Grid(m): 5000 Camera (X Z Zoom 250000.00 253000.00 2.758 (0)"
    )
    assert abs(dropped["mouse_x"] - 250490.96) < 0.02
    assert abs(dropped["mouse_z"] - 213914.47) < 0.02
    assert dropped["cam_x"] == 250_000 and dropped["cam_z"] == 253_000
    assert abs(dropped["zoom"] - 2.758) < 0.001
    light = np.full((40, 80, 3), 240, np.uint8)
    light[:12] = 40
    assert status_strip(light).shape[0] < 40
    assert status_strip(light).mean() > 200
    # South-edge beige is brighter than 200. The strip stays the status band.
    beige = np.full((220, 400, 3), 213, np.uint8)
    beige[-24:] = 240
    beige[-24] = 255
    assert status_strip(beige).shape[0] <= 32

    mask = TerrainMask(4, 4, bytes([1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1]))
    assert mask.has_land(MAP_MAX * 0.6, MAP_MAX * 0.7, MAP_MAX * 0.2, MAP_MAX * 0.4)
    water = TerrainMask(2, 2, bytes([1, 1, 1, 1]))
    assert not water.has_land(0, MAP_MAX, 0, MAP_MAX)

    import tifffile
    tmp = ROOT / "tools" / "mapcapture" / "_selftest.tif"
    try:
        mm = tifffile.memmap(tmp, shape=(8, 10, 3), dtype=np.uint8, photometric="rgb")
        mm[1, 2] = (4, 5, 6)
        mm.flush()
        del mm
        got = tifffile.imread(tmp)
        assert tuple(int(v) for v in got[1, 2]) == (4, 5, 6)
    finally:
        if tmp.exists():
            tmp.unlink()

    shot = grab_screen(0, 0, 16, 16)
    assert shot.shape == (16, 16, 3)

    sample = Image.new("RGB", (420, 48), (240, 240, 236))
    draw = ImageDraw.Draw(sample)
    font = ImageFont.truetype(r"C:\Windows\Fonts\segoeui.ttf", 22)
    draw.text((8, 8), "124500   305800", fill=(20, 20, 20), font=font)
    sample_path = ROOT / "tools" / "mapcapture" / "_selftest_ocr.png"
    try:
        sample.save(sample_path)
        text = ocr_image(sample_path)
        assert parse_numbers(text)[-2:] == [124500.0, 305800.0], text
    finally:
        if sample_path.exists():
            sample_path.unlink()
    say("selftest passed")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Capture the mission editor map and stitch it.")
    parser.add_argument("--out", type=Path, default=DEFAULT_OUT)
    parser.add_argument("--window", default="", help='editor title contains this. Default tries "IL2 Series Editor".')
    sub = parser.add_subparsers(dest="cmd", required=True)

    sub.add_parser("init", help="press Alt+F9 twice, then stop so you can set the zoom")
    sub.add_parser("selftest", help="check the stitch math, the screen grab, and OCR")

    cap = sub.add_parser("capture", help="measure the current zoom, then tile the map")
    cap.add_argument("--init", action="store_true", help="press Alt+F9 twice and stop, before you set the zoom")
    cap.add_argument("--limit", type=int, default=None, help="capture at most this many tiles, then stitch what exists")
    cap.add_argument("--yes", action="store_true", help="start a long run without the Enter prompt")
    cap.add_argument("--include-sea", action="store_true", help="photograph open water too. Land and coast are always kept.")
    cap.add_argument("--recalibrate", action="store_true", help="measure the zoom again and rebuild the tile plan")
    cap.add_argument("--sample", action="store_true", help="capture a 2 by 2 in the middle of the map, in out/sample")
    cap.add_argument("--max-gb", type=float, default=16, help="largest uncompressed stitched image to write")
    cap.add_argument("--x0", type=float, default=MAP_MIN)
    cap.add_argument("--x1", type=float, default=MAP_MAX)
    cap.add_argument("--z0", type=float, default=MAP_MIN)
    cap.add_argument("--z1", type=float, default=MAP_MAX)

    stitch_p = sub.add_parser("stitch", help="rebuild the image from tiles already captured")
    stitch_p.add_argument("--max-gb", type=float, default=16)
    return parser


def main(argv: list[str] | None = None) -> int:
    if sys.platform != "win32":
        say("This capture runs on Windows, against the mission editor.")
        return 1
    dpi_aware()
    args = build_parser().parse_args(argv)
    try:
        if args.cmd == "selftest":
            selftest()
        elif args.cmd == "init":
            cmd_init(args)
        elif args.cmd == "capture":
            cmd_capture(args)
        elif args.cmd == "stitch":
            cmd_stitch(args)
        return 0
    except CaptureError as exc:
        say(str(exc))
        return 1
    except KeyboardInterrupt:
        say("Stopped.")
        return 1
    except Exception:
        traceback.print_exc()
        return 1


if __name__ == "__main__":
    sys.exit(main())
