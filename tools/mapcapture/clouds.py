"""Photograph the cloud list across the day, without moving the camera.

The editor stays on the view already on screen. Mission Properties must
already be open. The script sets the mission clock, then applies each cloud
preset with OK so the 3D view redraws.

8pm through 6am share one night frame, shot at midnight on clear sky.
7am through 7pm get a new frame for every cloud type, on the hour. Each
frame is saved at half the view size. The original time and cloud preset
are put back at the end.

Panels that cover the view are moved aside and put back afterwards.
Press Esc to stop. Frames already saved are kept.

    py -3 tools/mapcapture/clouds.py
"""

from __future__ import annotations

import ctypes
import json
import sys
import time
from ctypes import wintypes
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont

sys.path.insert(0, str(Path(__file__).resolve().parent))
import capture as cap

OUT = cap.ROOT / "References" / "MapCapture" / "clouds"
CB_GETCOUNT = 0x0146
CB_GETCURSEL = 0x0147
CB_GETLBTEXT = 0x0148
CB_SETCURSEL = 0x014E
CBN_SELCHANGE = 1
CBN_SELENDOK = 9
WM_COMMAND = 0x0111
BM_CLICK = 0x00F5
SWP_NOSIZE = 0x0001
SWP_NOZORDER = 0x0004
SWP_NOACTIVATE = 0x0010


def send_uint(hwnd: int, msg: int, wparam: int, lparam: int = 0) -> int:
    proto = ctypes.WINFUNCTYPE(
        ctypes.c_ssize_t, ctypes.c_void_p, wintypes.UINT, ctypes.c_size_t, ctypes.c_ssize_t,
    )
    fn = proto(("SendMessageW", ctypes.windll.user32))
    return int(fn(hwnd, msg, wparam, lparam))


def post_uint(hwnd: int, msg: int, wparam: int, lparam: int = 0) -> None:
    user32, _, _ = cap._win32()
    user32.PostMessageW.argtypes = [ctypes.c_void_p, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
    user32.PostMessageW.restype = wintypes.BOOL
    user32.PostMessageW(hwnd, msg, wparam, lparam)


def combo_items(hwnd: int) -> list[str]:
    user32, _, _ = cap._win32()
    count = send_uint(hwnd, CB_GETCOUNT, 0, 0)
    items = []
    for index in range(count):
        buf = ctypes.create_unicode_buffer(512)
        user32.SendMessageW(hwnd, CB_GETLBTEXT, index, buf)
        items.append(buf.value)
    return items


def find_titled(title: str) -> int | None:
    user32, _, _ = cap._win32()
    found = []

    def top(hwnd, _lp):
        if user32.IsWindowVisible(hwnd) and cap.window_title(int(hwnd)) == title:
            found.append(int(hwnd))
        return True

    cap._enum(top)
    return found[0] if found else None


def visible_by_class(parent: int, class_name: str) -> list[int]:
    user32, _, _ = cap._win32()
    out = []
    for hwnd in cap.child_controls(parent):
        if user32.IsWindowVisible(hwnd) and cap.window_class(hwnd) == class_name:
            out.append(int(hwnd))
    return out


def cloud_combo(dialog: int) -> int:
    for hwnd in visible_by_class(dialog, "ComboBox"):
        items = combo_items(hwnd)
        if items and items[0].startswith("00_clear"):
            return hwnd
    raise cap.CaptureError("the atmosphere dialog has no cloud list")


def notify_combo(combo: int) -> None:
    user32, _, _ = cap._win32()
    user32.GetDlgCtrlID.argtypes = [ctypes.c_void_p]
    user32.GetDlgCtrlID.restype = ctypes.c_int
    user32.GetParent.argtypes = [ctypes.c_void_p]
    user32.GetParent.restype = ctypes.c_void_p
    ctrl = int(user32.GetDlgCtrlID(combo))
    parent = int(user32.GetParent(combo) or 0)
    for code in (CBN_SELENDOK, CBN_SELCHANGE):
        cap._send_ptr(parent, WM_COMMAND, (code << 16) | (ctrl & 0xFFFF), combo)


def select_cloud(combo: int, index: int) -> str:
    send_uint(combo, CB_SETCURSEL, index, 0)
    notify_combo(combo)
    items = combo_items(combo)
    chosen = send_uint(combo, CB_GETCURSEL, 0, 0)
    if chosen != index:
        raise cap.CaptureError(f"cloud list stayed on {chosen}, not {index}")
    return items[index]


def edit_value(dialog: int, ctrl_id: int) -> str:
    user32, _, _ = cap._win32()
    user32.GetDlgCtrlID.argtypes = [ctypes.c_void_p]
    user32.GetDlgCtrlID.restype = ctypes.c_int
    for hwnd in cap.child_controls(dialog):
        if int(user32.GetDlgCtrlID(hwnd)) == ctrl_id and cap.window_class(hwnd) == "Edit":
            return cap.edit_text(int(hwnd)).strip()
    return ""


def move_window(hwnd: int, x: int, y: int) -> None:
    user32, _, _ = cap._win32()
    user32.SetWindowPos.argtypes = [
        ctypes.c_void_p, ctypes.c_void_p, ctypes.c_int, ctypes.c_int,
        ctypes.c_int, ctypes.c_int, wintypes.UINT,
    ]
    user32.SetWindowPos(hwnd, None, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE)


def view_rect(editor: int) -> tuple[int, int, int, int]:
    """3D view inside the editor, clear of the left tree, right palette, and status bar."""
    left, top, right, bottom = cap.window_rect(editor)
    return (left + 280, top + 118, right - 210, bottom - 48)


def grab(rect: tuple[int, int, int, int]) -> np.ndarray:
    left, top, right, bottom = rect
    return cap.grab_screen(left, top, right - left, bottom - top)


def shoot(editor: int, rect: tuple[int, int, int, int]) -> np.ndarray:
    """Bring the editor forward, then photograph its view."""
    cap.focus_window(editor, required=False)
    return grab(rect)


def sky_delta(before: np.ndarray, after: np.ndarray) -> float:
    """Mean change in the upper sky. The sea keeps moving, so it is left out."""
    rows = max(40, before.shape[0] // 5)
    a = before[:rows:4, ::4].astype(np.int16)
    b = after[:rows:4, ::4].astype(np.int16)
    return float(np.abs(a - b).mean())


def wait_for(title: str, present: bool, seconds: float = 3.0) -> int | None:
    deadline = time.time() + seconds
    while time.time() < deadline:
        hwnd = find_titled(title)
        if present and hwnd:
            return hwnd
        if not present and hwnd is None:
            return None
        time.sleep(0.05)
    if present:
        raise cap.CaptureError(f"{title} did not open")
    raise cap.CaptureError(f"{title} stayed open")


def mission_properties() -> int:
    props = find_titled("Mission Properties")
    if props is None:
        raise cap.CaptureError("Mission Properties is not open")
    return props


def atmosphere_button() -> int:
    user32, _, _ = cap._win32()
    user32.IsWindowEnabled.argtypes = [ctypes.c_void_p]
    user32.IsWindowEnabled.restype = wintypes.BOOL
    user32.GetDlgCtrlID.argtypes = [ctypes.c_void_p]
    user32.GetDlgCtrlID.restype = ctypes.c_int
    for hwnd in cap.child_controls(mission_properties()):
        if (
            user32.IsWindowVisible(hwnd)
            and user32.IsWindowEnabled(hwnd)
            and cap.window_title(hwnd).startswith("Atmosphere")
        ):
            return int(hwnd)
    raise cap.CaptureError("Atmosphere options is not on Mission Properties")


def wait_until_enabled(hwnd: int, seconds: float = 8.0) -> None:
    """The properties dialog stays disabled while a modal atmosphere box is closing."""
    user32, _, _ = cap._win32()
    user32.IsWindowEnabled.argtypes = [ctypes.c_void_p]
    user32.IsWindowEnabled.restype = wintypes.BOOL
    deadline = time.time() + seconds
    while time.time() < deadline:
        if user32.IsWindowEnabled(hwnd):
            return
        time.sleep(0.05)
    raise cap.CaptureError("Mission Properties stayed disabled")


def click_atmosphere() -> None:
    """Ask Mission Properties to open the atmosphere dialog.

    A posted button click is dropped while the editor is still rebuilding the
    sky. The command is sent again until the dialog is up.
    """
    props = mission_properties()
    wait_until_enabled(props)
    cap.focus_window(props, required=False)
    button = atmosphere_button()
    user32, _, _ = cap._win32()
    user32.GetDlgCtrlID.argtypes = [ctypes.c_void_p]
    user32.GetDlgCtrlID.restype = ctypes.c_int
    ctrl = int(user32.GetDlgCtrlID(button))
    # BN_CLICKED is what the button would send its parent after BM_CLICK.
    post_uint(props, WM_COMMAND, ctrl & 0xFFFF, button)
    post_uint(button, BM_CLICK, 0, 0)


def open_atmosphere() -> int:
    existing = find_titled("Mission Atmosphere Options")
    if existing:
        move_window(existing, 1760, 40)
        return existing
    last = "Mission Atmosphere Options did not open"
    for _ in range(6):
        click_atmosphere()
        deadline = time.time() + 2.0
        while time.time() < deadline:
            dialog = find_titled("Mission Atmosphere Options")
            if dialog:
                move_window(dialog, 1760, 40)
                return dialog
            time.sleep(0.05)
        last = "Mission Atmosphere Options did not open"
    raise cap.CaptureError(last)


def press_dialog(dialog: int, ctrl_id: int, label: str) -> None:
    user32, _, _ = cap._win32()
    user32.GetDlgCtrlID.argtypes = [ctypes.c_void_p]
    user32.GetDlgCtrlID.restype = ctypes.c_int
    button = None
    for hwnd in cap.child_controls(dialog):
        if int(user32.GetDlgCtrlID(hwnd)) == ctrl_id and cap.window_title(hwnd) == label:
            button = int(hwnd)
            break
    if button is None:
        raise cap.CaptureError(f"the atmosphere dialog has no {label} button")
    for _ in range(4):
        post_uint(dialog, WM_COMMAND, ctrl_id & 0xFFFF, button)
        post_uint(button, BM_CLICK, 0, 0)
        deadline = time.time() + 2.0
        while time.time() < deadline:
            if find_titled("Mission Atmosphere Options") is None:
                return
            time.sleep(0.05)
    raise cap.CaptureError(f"the atmosphere dialog stayed open after {label}")


def contact_sheet(paths: list[Path], labels: list[str], dest: Path) -> None:
    cols = 10
    thumb_w, thumb_h, label_h = 160, 96, 18
    rows = (len(paths) + cols - 1) // cols
    sheet = Image.new("RGB", (cols * thumb_w, rows * (thumb_h + label_h)), (24, 26, 28))
    draw = ImageDraw.Draw(sheet)
    font = ImageFont.load_default()
    for index, path in enumerate(paths):
        col, row = index % cols, index // cols
        x, y = col * thumb_w, row * (thumb_h + label_h)
        with Image.open(path) as image:
            image = image.convert("RGB")
            image.thumbnail((thumb_w, thumb_h))
            sheet.paste(image, (x, y))
        draw.text((x + 2, y + thumb_h + 2), labels[index], fill=(230, 230, 230), font=font)
    sheet.save(dest)


ASSETS = cap.ROOT / "assets" / "clouds"
JPEG_QUALITY = 35
DAY_HOURS = list(range(7, 20))
NIGHT_HOUR = 0
TIME_ID = 1039
# Fractions across the time field: hour, minute, and the AM/PM marker.
HOUR_AT = 6 / 128
MINUTE_AT = 28 / 128
MERIDIEM_AT = 78 / 128


class Clock:
    def __init__(self, hour: int, minute: int) -> None:
        self.wHour = hour
        self.wMinute = minute


def control_by_id(parent: int, ctrl_id: int, class_name: str) -> int:
    user32, _, _ = cap._win32()
    user32.GetDlgCtrlID.argtypes = [ctypes.c_void_p]
    user32.GetDlgCtrlID.restype = ctypes.c_int
    for hwnd in cap.child_controls(parent):
        if (
            user32.IsWindowVisible(hwnd)
            and int(user32.GetDlgCtrlID(hwnd)) == ctrl_id
            and cap.window_class(hwnd) == class_name
        ):
            return int(hwnd)
    raise cap.CaptureError(f"Mission Properties has no {class_name} {ctrl_id}")


def parse_clock(text: str) -> tuple[int, int]:
    parts = text.replace(".", ":").split(":")
    hour = int(parts[0].strip())
    minute = int(parts[1].strip()) if len(parts) > 1 else 0
    low = text.lower()
    if "pm" in low and hour != 12:
        hour += 12
    if "am" in low and hour == 12:
        hour = 0
    return hour % 24, minute


def mission_clock(props: int) -> Clock:
    """Read the time field. Its text crosses processes safely; a time pointer does not."""
    text = cap.edit_text(control_by_id(props, TIME_ID, "SysDateTimePick32")).strip()
    try:
        hour, minute = parse_clock(text)
    except ValueError:
        raise cap.CaptureError(f"could not read the mission time {text!r}") from None
    return Clock(hour, minute)


def click_time_field(picker: int, across: float) -> None:
    """Click one part of the time field. The click has to land on that field."""
    user32, _, _ = cap._win32()
    user32.WindowFromPoint.argtypes = [wintypes.POINT]
    user32.WindowFromPoint.restype = ctypes.c_void_p
    props = mission_properties()
    wait_until_enabled(props)
    cap.focus_window(props, required=False)
    left, top, right, bottom = cap.window_rect(picker)
    width = max(right - left, 1)
    x = left + int(width * across)
    y = (top + bottom) // 2
    screen_w = user32.GetSystemMetrics(0)
    screen_h = user32.GetSystemMetrics(1)
    if not (0 <= x < screen_w and 0 <= y < screen_h):
        pl, pt, _pr, _pb = cap.window_rect(props)
        move_window(props, pl + (40 - left), pt + (80 - top))
        left, top, right, bottom = cap.window_rect(picker)
        x = left + int(max(right - left, 1) * across)
        y = (top + bottom) // 2
    hit = int(user32.WindowFromPoint(wintypes.POINT(x, y)) or 0)
    if hit != int(picker):
        raise cap.CaptureError("the time field is covered. Move other windows off Mission Properties.")
    user32.SetCursorPos(x, y)
    time.sleep(0.06)
    user32.mouse_event(0x0002, 0, 0, 0, None)
    user32.mouse_event(0x0004, 0, 0, 0, None)
    time.sleep(0.1)


def type_text(text: str) -> None:
    for ch in text:
        cap.tap(ord(ch))
        time.sleep(0.06)


def set_clock(hour: int, minute: int) -> None:
    """Type the mission time and Apply it. The date is left as it is."""
    props = mission_properties()
    parked = cap.window_rect(props)
    picker = control_by_id(props, TIME_ID, "SysDateTimePick32")
    shown = cap.edit_text(picker)
    twelve = "am" in shown.lower() or "pm" in shown.lower()
    display = (hour % 12 or 12) if twelve else hour
    click_time_field(picker, HOUR_AT)
    type_text(f"{display:02d}")
    click_time_field(picker, MINUTE_AT)
    type_text(f"{minute:02d}")
    if twelve:
        click_time_field(picker, MERIDIEM_AT)
        cap.tap(ord("A" if hour < 12 else "P"))
        time.sleep(0.08)
    # Typing notifies the editor. Apply on an unsaved mission opens Save As.
    time.sleep(0.3)
    move_window(props, parked[0], parked[1])
    read = cap.edit_text(picker).strip()
    try:
        got_hour, got_minute = parse_clock(read)
    except ValueError:
        got_hour, got_minute = -1, -1
    if got_hour != hour or got_minute != minute:
        raise cap.CaptureError(
            f"the mission clock stayed {read or '?'}, not {hour:02d}:{minute:02d}"
        )


def save_jpeg(pixels: np.ndarray, path: Path) -> None:
    image = Image.fromarray(pixels).convert("RGB")
    image = image.resize(
        (max(1, image.width // 2), max(1, image.height // 2)),
        Image.Resampling.LANCZOS,
    )
    path.parent.mkdir(parents=True, exist_ok=True)
    image.save(path, quality=JPEG_QUALITY, optimize=True)


def apply_preset(index: int) -> tuple[str, str]:
    """Choose one cloud preset and press OK. Returns its name and cloud level."""
    dialog = open_atmosphere()
    combo = cloud_combo(dialog)
    name = select_cloud(combo, index)
    level = edit_value(dialog, 1058)
    press_dialog(dialog, 1, "OK")
    time.sleep(0.45)
    return name, level


def shoot_preset(editor: int, rect: tuple[int, int, int, int], index: int, previous: np.ndarray | None):
    name, level = apply_preset(index)
    shot = shoot(editor, rect)
    if previous is not None and sky_delta(previous, shot) < 0.3:
        time.sleep(0.5)
        shot = shoot(editor, rect)
    return name, level, shot


def main() -> None:
    cap.dpi_aware()
    if cap.key_down(cap.VK_ESCAPE):
        raise cap.CaptureError("Esc is held. Let it go, then run again.")
    editor, title = cap.find_editor("IL2")
    cap.focus_window(editor)
    cap.say(f"Editor in front: {title}")
    clock = mission_clock(mission_properties())
    dialog = open_atmosphere()
    names = combo_items(cloud_combo(dialog))
    original = send_uint(cloud_combo(dialog), CB_GETCURSEL, 0, 0)
    original_level = edit_value(dialog, 1058)
    press_dialog(dialog, 2, "Cancel")
    cap.say(f"Editor: {title}")
    cap.say(f"{len(names)} cloud presets. The camera is not moved.")
    cap.say(
        f"Starting at {clock.wHour:02d}:{clock.wMinute:02d}, preset {names[original]} "
        f"at {original_level or '?'} m. Both are restored at the end."
    )
    shots = 1 + len(DAY_HOURS) * len(names)
    cap.say(
        f"{shots} frames: one night picture, then every cloud type from 7am through 7pm. Esc stops."
    )

    props = find_titled("Mission Properties")
    cursor = find_titled("Cursor Agents")
    parked = []
    for hwnd in (props, cursor):
        if not hwnd:
            continue
        left, top, _right, _bottom = cap.window_rect(hwnd)
        parked.append((hwnd, left, top))
    if props:
        move_window(props, 1760, 40)
    if cursor:
        move_window(cursor, 40, 1100)

    rect = view_rect(editor)
    OUT.mkdir(parents=True, exist_ok=True)
    saved: list[dict[str, str]] = []
    stopped = False
    try:
        set_clock(12, 0)
        _, _, noon = shoot_preset(editor, rect, 0, None)
        set_clock(NIGHT_HOUR, 0)
        _, _, midnight = shoot_preset(editor, rect, 0, noon)
        if sky_delta(noon, midnight) < 1.0:
            raise cap.CaptureError("Changing the clock from noon to midnight did not change the view.")
        night_path = ASSETS / "night.jpg"
        save_jpeg(midnight, night_path)
        saved.append({"hour": "night", "name": names[0], "file": "night.jpg"})
        cap.say(f"Night frame saved ({names[0]} at {NIGHT_HOUR:02d}:00).")

        for hour in DAY_HOURS:
            if stopped:
                break
            set_clock(hour, 0)
            cap.say(f"{hour:02d}:00")
            previous = None
            for index, name in enumerate(names):
                if cap.key_down(cap.VK_ESCAPE):
                    cap.say("Stopped. Esc was pressed. Frames already saved are kept.")
                    stopped = True
                    break
                name, level, shot = shoot_preset(editor, rect, index, previous)
                path = ASSETS / f"h{hour:02d}" / f"{name}.jpg"
                save_jpeg(shot, path)
                saved.append({"hour": f"{hour:02d}", "name": name, "level_m": level, "file": str(path.relative_to(cap.ROOT))})
                previous = shot
                cap.say(f"  {hour:02d}:00  {index + 1:2}/{len(names)} {name}  level {level}")
        note = {
            "editor": title,
            "camera_note": "The camera was left where it was.",
            "original_preset": names[original],
            "original_level_m": original_level,
            "original_time": f"{clock.wHour:02d}:{clock.wMinute:02d}",
            "night": "8pm through 6am use night.jpg, shot at midnight on the first cloud preset.",
            "day": "7am through 7pm, every cloud preset, on the hour.",
            "jpeg_quality": JPEG_QUALITY,
            "scale": "half the editor view",
            "frames": saved,
        }
        (OUT / "hours.json").write_text(json.dumps(note, indent=2), encoding="utf-8")
        cap.say(f"Wrote {len(saved)} pictures under {ASSETS}")
    finally:
        try:
            set_clock(int(clock.wHour), int(clock.wMinute))
            cap.say(f"Restored {clock.wHour:02d}:{clock.wMinute:02d}.")
        except cap.CaptureError as err:
            cap.say(f"Could not restore the original time: {err}")
        try:
            apply_preset(original)
            cap.say(f"Restored {names[original]}.")
        except cap.CaptureError as err:
            cap.say(f"Could not restore the original preset: {err}")
        for hwnd, x, y in parked:
            move_window(hwnd, x, y)


if __name__ == "__main__":
    try:
        main()
    except cap.CaptureError as err:
        cap.say(f"Stopped: {err}")
        sys.exit(1)
