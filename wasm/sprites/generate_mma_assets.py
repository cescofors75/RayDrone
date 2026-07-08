#!/usr/bin/env python3
"""
Generate the Level 6 MMA sprite sheets and TexturePacker-style atlas.

The game loader expects frame keys such as "idle_01" and a frame rectangle
inside `frames[name].frame`. This script keeps that contract, produces two
fighter colorways from the same rig, and writes a matching octagon backdrop.
"""

from __future__ import annotations

import argparse
import json
import math
from pathlib import Path
from typing import Dict, Iterable, List, Tuple

from PIL import Image, ImageDraw, ImageFilter


CELL = 192
COLS = 12
SCALE = 3

FrameSpec = Tuple[str, int]

ANIMATIONS: List[FrameSpec] = [
    ("idle", 8),
    ("walk", 8),
    ("run", 8),
    ("crouch_guard", 5),
    ("jump", 3),
    ("jab", 5),
    ("taunt", 5),
    ("low_kick", 5),
    ("mid_kick", 5),
    ("high_kick", 5),
    ("roundhouse", 6),
    ("dash_punch", 5),
    ("clinch", 3),
    ("knee", 4),
    ("takedown", 6),
    ("ground_roll", 4),
    ("ko_front", 4),
    ("ko_back", 4),
    ("hit_back", 5),
    ("ground_hit", 4),
    ("get_up", 6),
    ("ready", 3),
    ("victory", 5),
]

PALETTES = {
    "warrior1": {
        "skin": "#d59b67",
        "skin_shadow": "#9d6543",
        "hair": "#263044",
        "shorts": "#1a436d",
        "shorts_trim": "#37d6ff",
        "glove": "#42d9ff",
        "glove_dark": "#12749b",
        "wrap": "#f2f0dc",
        "outline": "#25304a",
        "accent": "#fff07a",
    },
    "warrior": {
        "skin": "#c9825f",
        "skin_shadow": "#844932",
        "hair": "#5d2338",
        "shorts": "#6b2637",
        "shorts_trim": "#ff716f",
        "glove": "#ff655f",
        "glove_dark": "#a82c38",
        "wrap": "#f8e6d0",
        "outline": "#36263b",
        "accent": "#82ffb7",
    },
}


def rgba(hex_color: str, alpha: int = 255) -> Tuple[int, int, int, int]:
    hex_color = hex_color.lstrip("#")
    return (
        int(hex_color[0:2], 16),
        int(hex_color[2:4], 16),
        int(hex_color[4:6], 16),
        alpha,
    )


def lerp(a: float, b: float, t: float) -> float:
    return a + (b - a) * t


def ease(t: float) -> float:
    return 0.5 - 0.5 * math.cos(max(0.0, min(1.0, t)) * math.pi)


def phase(index: int, count: int) -> float:
    return index / max(1, count - 1)


def line(draw: ImageDraw.ImageDraw, a: Tuple[float, float], b: Tuple[float, float], color, width: int) -> None:
    ax, ay = a
    bx, by = b
    draw.line((ax, ay, bx, by), fill=color, width=width)
    r = width / 2
    draw.ellipse((ax - r, ay - r, ax + r, ay + r), fill=color)
    draw.ellipse((bx - r, by - r, bx + r, by + r), fill=color)


def polyline(draw: ImageDraw.ImageDraw, points: Iterable[Tuple[float, float]], color, width: int) -> None:
    pts = list(points)
    for a, b in zip(pts, pts[1:]):
        line(draw, a, b, color, width)


def ellipse(draw: ImageDraw.ImageDraw, cx: float, cy: float, rx: float, ry: float, color, outline=None, width=1) -> None:
    box = (cx - rx, cy - ry, cx + rx, cy + ry)
    draw.ellipse(box, fill=color, outline=outline, width=width)


def limb_points(
    hip: Tuple[float, float],
    knee: Tuple[float, float],
    foot: Tuple[float, float],
) -> List[Tuple[float, float]]:
    return [hip, knee, foot]


def arm_points(
    shoulder: Tuple[float, float],
    elbow: Tuple[float, float],
    fist: Tuple[float, float],
) -> List[Tuple[float, float]]:
    return [shoulder, elbow, fist]


def pose_for(state: str, idx: int, count: int) -> Dict[str, object]:
    t = phase(idx, count)
    cyc = math.sin(t * math.tau)
    cyc2 = math.cos(t * math.tau)
    p: Dict[str, object] = {
        "root": (0.0, 0.0),
        "lean": 0.0,
        "lift": 0.0,
        "squat": 0.0,
        "front_arm": "guard",
        "back_arm": "guard",
        "front_leg": "stance",
        "back_leg": "stance",
        "head_turn": 0.0,
        "motion": 0.0,
    }

    if state in {"idle", "ready"}:
        p["lift"] = -2.0 + cyc * 1.5
        p["front_arm"] = "guard_high"
        p["back_arm"] = "guard"
        p["front_leg"] = "stance"
        p["back_leg"] = "back_stance"
        if state == "ready":
            p["lean"] = -4.0 + t * 4.0
    elif state == "walk":
        p["root"] = (cyc * 3.0, 0.0)
        p["front_leg"] = "step_forward" if cyc >= 0 else "step_back"
        p["back_leg"] = "step_back" if cyc >= 0 else "step_forward"
        p["front_arm"] = "guard"
        p["back_arm"] = "counter_swing"
    elif state == "run":
        p["root"] = (cyc * 5.0, -abs(cyc) * 2.0)
        p["lean"] = 8.0
        p["front_leg"] = "run_forward" if cyc >= 0 else "run_back"
        p["back_leg"] = "run_back" if cyc >= 0 else "run_forward"
        p["front_arm"] = "drive_back" if cyc >= 0 else "drive_forward"
        p["back_arm"] = "drive_forward" if cyc >= 0 else "drive_back"
    elif state == "crouch_guard":
        p["squat"] = 16.0 + ease(t) * 10.0
        p["lean"] = -7.0
        p["front_arm"] = "cover_low"
        p["back_arm"] = "cover_high"
        p["front_leg"] = "crouch_front"
        p["back_leg"] = "crouch_back"
    elif state == "jump":
        arc = math.sin(t * math.pi)
        p["lift"] = -36.0 * max(0.35, arc)
        p["front_leg"] = "knee_up"
        p["back_leg"] = "trail"
        p["front_arm"] = "guard_high"
        p["back_arm"] = "guard"
    elif state == "jab":
        reach = math.sin(t * math.pi)
        p["lean"] = 4.0 + 9.0 * reach
        p["front_arm"] = ("jab", reach)
        p["back_arm"] = "guard_high"
        p["front_leg"] = "stance"
        p["back_leg"] = "brace"
    elif state == "dash_punch":
        reach = ease(min(1.0, t * 1.5))
        p["root"] = (10.0 * reach, 0.0)
        p["lean"] = 14.0 * reach
        p["front_arm"] = ("cross", reach)
        p["back_arm"] = "drive_back"
        p["front_leg"] = "lunge_front"
        p["back_leg"] = "lunge_back"
        p["motion"] = reach
    elif state == "low_kick":
        kick = math.sin(t * math.pi)
        p["lean"] = -5.0
        p["front_leg"] = ("kick_low", kick)
        p["back_leg"] = "brace"
        p["front_arm"] = "counter_swing"
        p["back_arm"] = "guard_high"
    elif state == "mid_kick":
        kick = math.sin(t * math.pi)
        p["lean"] = -8.0
        p["front_leg"] = ("kick_mid", kick)
        p["back_leg"] = "brace"
        p["front_arm"] = "counter_swing"
        p["back_arm"] = "guard_high"
    elif state == "high_kick":
        kick = math.sin(t * math.pi)
        p["lean"] = -12.0
        p["front_leg"] = ("kick_high", kick)
        p["back_leg"] = "brace"
        p["front_arm"] = "counter_swing"
        p["back_arm"] = "guard_high"
    elif state == "roundhouse":
        spin = math.sin(t * math.pi)
        p["lean"] = -14.0 * spin
        p["front_leg"] = ("roundhouse", spin)
        p["back_leg"] = "pivot"
        p["front_arm"] = "wide"
        p["back_arm"] = "wide_back"
        p["head_turn"] = spin
    elif state == "clinch":
        p["lean"] = 8.0
        p["front_arm"] = "clinch"
        p["back_arm"] = "clinch"
        p["front_leg"] = "stance"
        p["back_leg"] = "back_stance"
    elif state == "knee":
        hit = math.sin(t * math.pi)
        p["lean"] = 5.0
        p["front_leg"] = ("knee", hit)
        p["back_leg"] = "brace"
        p["front_arm"] = "clinch"
        p["back_arm"] = "guard_high"
    elif state == "takedown":
        dive = ease(t)
        p["root"] = (18.0 * dive, 10.0 * dive)
        p["lean"] = 28.0 * dive
        p["squat"] = 12.0 * dive
        p["front_arm"] = ("shoot", dive)
        p["back_arm"] = ("shoot_low", dive)
        p["front_leg"] = "lunge_front"
        p["back_leg"] = "trail"
    elif state == "hit_back":
        snap = math.sin(t * math.pi)
        p["root"] = (-12.0 * snap, 0.0)
        p["lean"] = -22.0 * snap
        p["front_arm"] = "flail"
        p["back_arm"] = "flail_back"
        p["front_leg"] = "stumble_front"
        p["back_leg"] = "brace"
    elif state in {"ko_front", "ko_back", "ground_hit", "ground_roll"}:
        roll = t * math.pi
        p["ground"] = state
        p["roll"] = roll
    elif state == "get_up":
        p["get_up"] = ease(t)
    elif state == "taunt":
        p["front_arm"] = "taunt"
        p["back_arm"] = "taunt_back"
        p["lean"] = math.sin(t * math.tau) * 4.0
    elif state == "victory":
        p["front_arm"] = "victory"
        p["back_arm"] = "guard"
        p["lift"] = -3.0 + math.sin(t * math.tau) * 2.0
    return p


def draw_shadow(draw: ImageDraw.ImageDraw, palette: Dict[str, str], x: float, y: float, alpha: int = 80) -> None:
    ellipse(draw, x, y + 3, 38, 9, rgba(palette["outline"], alpha))


def draw_ground_body(draw: ImageDraw.ImageDraw, palette: Dict[str, str], pose: Dict[str, object]) -> None:
    mode = str(pose.get("ground"))
    roll = float(pose.get("roll", 0.0))
    skin = rgba(palette["skin"])
    skin_shadow = rgba(palette["skin_shadow"])
    outline = rgba(palette["outline"])
    glove = rgba(palette["glove"])
    shorts = rgba(palette["shorts"])
    trim = rgba(palette["shorts_trim"])
    wrap = rgba(palette["wrap"])
    cx = CELL * SCALE * 0.50
    floor = CELL * SCALE * 0.87
    lift = math.sin(roll) * 5 * SCALE if mode == "ground_roll" else 0
    head_x = cx - 42 * SCALE
    hip_x = cx + 10 * SCALE
    if mode == "ko_back":
        head_x, hip_x = cx + 45 * SCALE, cx - 8 * SCALE
    if mode == "ground_roll":
        head_x = cx + math.cos(roll) * 40 * SCALE
        hip_x = cx - math.cos(roll) * 8 * SCALE
    y = floor - 24 * SCALE - lift
    line(draw, (head_x + 14 * SCALE, y + 8 * SCALE), (hip_x, y + 14 * SCALE), outline, 24 * SCALE)
    line(draw, (head_x + 14 * SCALE, y + 8 * SCALE), (hip_x, y + 14 * SCALE), skin_shadow, 18 * SCALE)
    draw.rounded_rectangle(
        (hip_x - 9 * SCALE, y + 1 * SCALE, hip_x + 27 * SCALE, y + 27 * SCALE),
        radius=8 * SCALE,
        fill=shorts,
        outline=outline,
        width=3 * SCALE,
    )
    draw.line((hip_x - 4 * SCALE, y + 5 * SCALE, hip_x + 23 * SCALE, y + 24 * SCALE), fill=trim, width=3 * SCALE)
    ellipse(draw, head_x, y + 2 * SCALE, 16 * SCALE, 18 * SCALE, skin, outline, 3 * SCALE)
    ellipse(draw, head_x + 7 * SCALE, y - 1 * SCALE, 9 * SCALE, 8 * SCALE, rgba(palette["hair"]))
    line(draw, (hip_x + 16 * SCALE, y + 20 * SCALE), (hip_x + 68 * SCALE, y + 24 * SCALE), outline, 12 * SCALE)
    line(draw, (hip_x + 16 * SCALE, y + 20 * SCALE), (hip_x + 68 * SCALE, y + 24 * SCALE), skin, 8 * SCALE)
    ellipse(draw, hip_x + 78 * SCALE, y + 24 * SCALE, 8 * SCALE, 5 * SCALE, wrap, outline, 2 * SCALE)
    line(draw, (hip_x + 4 * SCALE, y + 20 * SCALE), (hip_x - 55 * SCALE, y + 34 * SCALE), outline, 12 * SCALE)
    line(draw, (hip_x + 4 * SCALE, y + 20 * SCALE), (hip_x - 55 * SCALE, y + 34 * SCALE), skin, 8 * SCALE)
    ellipse(draw, hip_x - 65 * SCALE, y + 36 * SCALE, 8 * SCALE, 5 * SCALE, wrap, outline, 2 * SCALE)
    line(draw, (head_x + 16 * SCALE, y + 12 * SCALE), (head_x + 56 * SCALE, y - 10 * SCALE), outline, 11 * SCALE)
    line(draw, (head_x + 16 * SCALE, y + 12 * SCALE), (head_x + 56 * SCALE, y - 10 * SCALE), skin_shadow, 7 * SCALE)
    ellipse(draw, head_x + 66 * SCALE, y - 14 * SCALE, 8 * SCALE, 8 * SCALE, glove, outline, 2 * SCALE)


def draw_get_up(draw: ImageDraw.ImageDraw, palette: Dict[str, str], amount: float) -> None:
    if amount < 0.45:
        draw_ground_body(draw, palette, {"ground": "ground_hit", "roll": 0.0})
        return
    stand_amount = (amount - 0.45) / 0.55
    pose = pose_for("crouch_guard", 0, 1)
    pose["squat"] = 22.0 * (1.0 - stand_amount)
    pose["lift"] = 8.0 * (1.0 - stand_amount)
    draw_standing_body(draw, palette, pose)


def draw_arm(
    draw: ImageDraw.ImageDraw,
    palette: Dict[str, str],
    shoulder: Tuple[float, float],
    mode: object,
    side: int,
) -> None:
    sx, sy = shoulder
    skin = rgba(palette["skin"])
    outline = rgba(palette["outline"])
    glove = rgba(palette["glove"])
    glove_dark = rgba(palette["glove_dark"])
    if isinstance(mode, tuple):
        name, amount = mode
    else:
        name, amount = str(mode), 1.0

    if name == "jab":
        elbow = (sx + lerp(22, 46, amount) * SCALE, sy + lerp(4, -5, amount) * SCALE)
        fist = (sx + lerp(36, 78, amount) * SCALE, sy + lerp(-5, -12, amount) * SCALE)
    elif name == "cross":
        elbow = (sx + lerp(20, 54, amount) * SCALE, sy + lerp(9, 0, amount) * SCALE)
        fist = (sx + lerp(36, 88, amount) * SCALE, sy + lerp(4, -6, amount) * SCALE)
    elif name in {"shoot", "shoot_low"}:
        yoff = 18 if name == "shoot_low" else 8
        elbow = (sx + lerp(16, 50, amount) * SCALE, sy + yoff * SCALE)
        fist = (sx + lerp(30, 88, amount) * SCALE, sy + (yoff + 4) * SCALE)
    elif name == "guard_high":
        elbow = (sx + 19 * SCALE, sy + 12 * SCALE)
        fist = (sx + 30 * SCALE, sy - 18 * SCALE)
    elif name == "cover_low":
        elbow = (sx + 15 * SCALE, sy + 25 * SCALE)
        fist = (sx + 28 * SCALE, sy + 36 * SCALE)
    elif name == "cover_high":
        elbow = (sx + 8 * SCALE, sy + 10 * SCALE)
        fist = (sx + 18 * SCALE, sy - 24 * SCALE)
    elif name == "counter_swing":
        elbow = (sx - 10 * side * SCALE, sy + 32 * SCALE)
        fist = (sx - 24 * side * SCALE, sy + 44 * SCALE)
    elif name == "drive_forward":
        elbow = (sx + 16 * SCALE, sy + 20 * SCALE)
        fist = (sx + 34 * SCALE, sy + 32 * SCALE)
    elif name == "drive_back":
        elbow = (sx - 18 * SCALE, sy + 22 * SCALE)
        fist = (sx - 30 * SCALE, sy + 38 * SCALE)
    elif name in {"clinch", "wide", "taunt", "victory"}:
        if name == "victory":
            elbow = (sx + 10 * SCALE, sy - 34 * SCALE)
            fist = (sx + 18 * SCALE, sy - 68 * SCALE)
        elif name == "taunt":
            elbow = (sx + 28 * SCALE, sy + 5 * SCALE)
            fist = (sx + 52 * SCALE, sy + 2 * SCALE)
        elif name == "wide":
            elbow = (sx + 32 * SCALE, sy + 12 * SCALE)
            fist = (sx + 55 * SCALE, sy + 28 * SCALE)
        else:
            elbow = (sx + 32 * SCALE, sy + 15 * SCALE)
            fist = (sx + 58 * SCALE, sy + 12 * SCALE)
    elif name in {"taunt_back", "wide_back", "flail_back"}:
        elbow = (sx - 20 * SCALE, sy + 12 * SCALE)
        fist = (sx - 44 * SCALE, sy + (4 if name != "flail_back" else -18) * SCALE)
    elif name == "flail":
        elbow = (sx + 26 * SCALE, sy - 20 * SCALE)
        fist = (sx + 48 * SCALE, sy - 34 * SCALE)
    else:
        elbow = (sx + 12 * SCALE, sy + 24 * SCALE)
        fist = (sx + 24 * SCALE, sy + 2 * SCALE)

    polyline(draw, [(sx, sy), elbow, fist], outline, 13 * SCALE)
    polyline(draw, [(sx, sy), elbow, fist], skin, 8 * SCALE)
    ellipse(draw, fist[0], fist[1], 9 * SCALE, 8 * SCALE, glove, outline, 2 * SCALE)
    ellipse(draw, fist[0] + 2 * SCALE, fist[1] + 1 * SCALE, 5 * SCALE, 4 * SCALE, glove_dark)


def leg_pose(
    hip: Tuple[float, float],
    mode: object,
    side: int,
) -> Tuple[Tuple[float, float], Tuple[float, float]]:
    hx, hy = hip
    if isinstance(mode, tuple):
        name, amount = mode
    else:
        name, amount = str(mode), 1.0
    if name == "step_forward":
        return (hx + 11 * SCALE, hy + 40 * SCALE), (hx + 26 * SCALE, hy + 76 * SCALE)
    if name == "step_back":
        return (hx - 9 * SCALE, hy + 40 * SCALE), (hx - 20 * SCALE, hy + 75 * SCALE)
    if name == "run_forward":
        return (hx + 24 * SCALE, hy + 34 * SCALE), (hx + 44 * SCALE, hy + 70 * SCALE)
    if name == "run_back":
        return (hx - 22 * SCALE, hy + 26 * SCALE), (hx - 38 * SCALE, hy + 64 * SCALE)
    if name == "back_stance":
        return (hx - 14 * SCALE, hy + 43 * SCALE), (hx - 32 * SCALE, hy + 78 * SCALE)
    if name == "brace":
        return (hx - 16 * SCALE, hy + 43 * SCALE), (hx - 31 * SCALE, hy + 78 * SCALE)
    if name == "crouch_front":
        return (hx + 23 * SCALE, hy + 45 * SCALE), (hx + 42 * SCALE, hy + 67 * SCALE)
    if name == "crouch_back":
        return (hx - 20 * SCALE, hy + 42 * SCALE), (hx - 40 * SCALE, hy + 67 * SCALE)
    if name == "knee_up":
        return (hx + 19 * SCALE, hy + 16 * SCALE), (hx + 14 * SCALE, hy + 55 * SCALE)
    if name == "trail":
        return (hx - 21 * SCALE, hy + 43 * SCALE), (hx - 34 * SCALE, hy + 73 * SCALE)
    if name == "lunge_front":
        return (hx + 28 * SCALE, hy + 41 * SCALE), (hx + 56 * SCALE, hy + 76 * SCALE)
    if name == "lunge_back":
        return (hx - 25 * SCALE, hy + 34 * SCALE), (hx - 48 * SCALE, hy + 68 * SCALE)
    if name == "stumble_front":
        return (hx + 18 * SCALE, hy + 48 * SCALE), (hx + 9 * SCALE, hy + 82 * SCALE)
    if name == "pivot":
        return (hx - 5 * SCALE, hy + 45 * SCALE), (hx - 2 * SCALE, hy + 78 * SCALE)
    if name == "kick_low":
        return (
            (hx + lerp(12, 36, amount) * SCALE, hy + lerp(42, 48, amount) * SCALE),
            (hx + lerp(22, 83, amount) * SCALE, hy + lerp(74, 68, amount) * SCALE),
        )
    if name == "kick_mid":
        return (
            (hx + lerp(13, 38, amount) * SCALE, hy + lerp(38, 25, amount) * SCALE),
            (hx + lerp(24, 90, amount) * SCALE, hy + lerp(72, 35, amount) * SCALE),
        )
    if name == "kick_high":
        return (
            (hx + lerp(10, 33, amount) * SCALE, hy + lerp(36, 10, amount) * SCALE),
            (hx + lerp(22, 86, amount) * SCALE, hy + lerp(71, -4, amount) * SCALE),
        )
    if name == "roundhouse":
        return (
            (hx + lerp(6, 28, amount) * SCALE, hy + lerp(36, 2, amount) * SCALE),
            (hx + lerp(14, 93, amount) * SCALE, hy + lerp(72, 18, amount) * SCALE),
        )
    if name == "knee":
        return (
            (hx + lerp(9, 35, amount) * SCALE, hy + lerp(38, 18, amount) * SCALE),
            (hx + lerp(18, 44, amount) * SCALE, hy + lerp(72, 38, amount) * SCALE),
        )
    return (hx + 11 * SCALE, hy + 43 * SCALE), (hx + 20 * SCALE, hy + 78 * SCALE)


def draw_leg(draw: ImageDraw.ImageDraw, palette: Dict[str, str], hip: Tuple[float, float], mode: object, side: int) -> None:
    knee, foot = leg_pose(hip, mode, side)
    skin = rgba(palette["skin"])
    outline = rgba(palette["outline"])
    wrap = rgba(palette["wrap"])
    polyline(draw, limb_points(hip, knee, foot), outline, 15 * SCALE)
    polyline(draw, limb_points(hip, knee, foot), skin, 10 * SCALE)
    ellipse(draw, foot[0] + 6 * SCALE, foot[1] + 2 * SCALE, 12 * SCALE, 6 * SCALE, wrap, outline, 2 * SCALE)


def draw_standing_body(draw: ImageDraw.ImageDraw, palette: Dict[str, str], pose: Dict[str, object]) -> None:
    skin = rgba(palette["skin"])
    skin_shadow = rgba(palette["skin_shadow"])
    hair = rgba(palette["hair"])
    shorts = rgba(palette["shorts"])
    trim = rgba(palette["shorts_trim"])
    outline = rgba(palette["outline"])
    accent = rgba(palette["accent"])

    root_x = CELL * SCALE * 0.50 + float(pose.get("root", (0.0, 0.0))[0]) * SCALE
    root_y = CELL * SCALE * 0.87 + float(pose.get("root", (0.0, 0.0))[1]) * SCALE
    root_y += float(pose.get("lift", 0.0)) * SCALE
    squat = float(pose.get("squat", 0.0)) * SCALE
    lean = float(pose.get("lean", 0.0)) * SCALE

    hip = (root_x - 2 * SCALE, root_y - 58 * SCALE + squat)
    chest = (root_x + lean * 0.55, root_y - 112 * SCALE + squat * 0.42)
    neck = (root_x + lean * 0.70, root_y - 132 * SCALE + squat * 0.20)
    head = (root_x + lean * 0.77, root_y - 153 * SCALE + squat * 0.12)

    draw_shadow(draw, palette, root_x, CELL * SCALE * 0.88)

    back_hip = (hip[0] - 8 * SCALE, hip[1] + 2 * SCALE)
    front_hip = (hip[0] + 11 * SCALE, hip[1] + 2 * SCALE)
    draw_leg(draw, palette, back_hip, pose.get("back_leg", "back_stance"), -1)
    draw_leg(draw, palette, front_hip, pose.get("front_leg", "stance"), 1)

    shoulder_back = (chest[0] - 17 * SCALE, chest[1] + 8 * SCALE)
    shoulder_front = (chest[0] + 19 * SCALE, chest[1] + 8 * SCALE)
    draw_arm(draw, palette, shoulder_back, pose.get("back_arm", "guard"), -1)
    draw_arm(draw, palette, shoulder_front, pose.get("front_arm", "guard_high"), 1)

    body = [
        (chest[0] - 21 * SCALE, chest[1] - 1 * SCALE),
        (chest[0] + 21 * SCALE, chest[1] - 4 * SCALE),
        (hip[0] + 24 * SCALE, hip[1] + 16 * SCALE),
        (hip[0] - 22 * SCALE, hip[1] + 16 * SCALE),
    ]
    draw.polygon(body, fill=skin_shadow, outline=outline)
    draw.line((body[0][0], body[0][1], body[1][0], body[1][1]), fill=skin, width=4 * SCALE)

    shorts_box = (
        hip[0] - 26 * SCALE,
        hip[1] + 3 * SCALE,
        hip[0] + 27 * SCALE,
        hip[1] + 34 * SCALE,
    )
    draw.rounded_rectangle(shorts_box, radius=7 * SCALE, fill=shorts, outline=outline, width=3 * SCALE)
    draw.line((hip[0] - 22 * SCALE, hip[1] + 15 * SCALE, hip[0] + 22 * SCALE, hip[1] + 15 * SCALE), fill=trim, width=3 * SCALE)
    draw.line((hip[0] + 1 * SCALE, hip[1] + 8 * SCALE, hip[0] + 2 * SCALE, hip[1] + 34 * SCALE), fill=trim, width=2 * SCALE)

    line(draw, chest, neck, outline, 15 * SCALE)
    line(draw, chest, neck, skin, 9 * SCALE)
    ellipse(draw, head[0], head[1], 16 * SCALE, 19 * SCALE, skin, outline, 3 * SCALE)
    ellipse(draw, head[0] - 3 * SCALE, head[1] - 11 * SCALE, 15 * SCALE, 9 * SCALE, hair, outline, 2 * SCALE)
    ellipse(draw, head[0] + 8 * SCALE, head[1] - 1 * SCALE, 3 * SCALE, 2 * SCALE, outline)
    draw.arc(
        (head[0] + 3 * SCALE, head[1] + 4 * SCALE, head[0] + 12 * SCALE, head[1] + 12 * SCALE),
        5,
        70,
        fill=accent,
        width=2 * SCALE,
    )


def draw_frame(state: str, idx: int, count: int, palette: Dict[str, str]) -> Image.Image:
    hi = Image.new("RGBA", (CELL * SCALE, CELL * SCALE), (0, 0, 0, 0))
    draw = ImageDraw.Draw(hi)
    pose = pose_for(state, idx, count)
    if "ground" in pose:
        draw_shadow(draw, palette, CELL * SCALE * 0.50, CELL * SCALE * 0.88, 60)
        draw_ground_body(draw, palette, pose)
    elif "get_up" in pose:
        draw_get_up(draw, palette, float(pose["get_up"]))
    else:
        draw_standing_body(draw, palette, pose)

    # Contact accents make active attack frames readable at gameplay scale.
    active = state in {
        "jab",
        "low_kick",
        "mid_kick",
        "high_kick",
        "roundhouse",
        "dash_punch",
        "knee",
        "takedown",
    }
    if active and 0.28 <= phase(idx, count) <= 0.72:
        cx = CELL * SCALE * 0.67
        cy = CELL * SCALE * (0.50 if state in {"high_kick", "roundhouse"} else 0.62)
        if state == "low_kick":
            cy = CELL * SCALE * 0.76
        if state == "takedown":
            cy = CELL * SCALE * 0.72
        accent = rgba(palette["accent"], 150)
        draw.arc((cx - 18 * SCALE, cy - 18 * SCALE, cx + 18 * SCALE, cy + 18 * SCALE), -35, 35, fill=accent, width=3 * SCALE)

    return hi.resize((CELL, CELL), Image.Resampling.LANCZOS)


def build_sheet(palette_name: str, out_dir: Path) -> Dict[str, object]:
    palette = PALETTES[palette_name]
    total = sum(count for _, count in ANIMATIONS)
    rows = math.ceil(total / COLS)
    sheet = Image.new("RGBA", (COLS * CELL, rows * CELL), (0, 0, 0, 0))
    frames: Dict[str, Dict[str, object]] = {}
    index = 0

    for state, count in ANIMATIONS:
        for i in range(count):
            x = (index % COLS) * CELL
            y = (index // COLS) * CELL
            frame = draw_frame(state, i, count, palette)
            sheet.alpha_composite(frame, (x, y))
            key = f"{state}_{i + 1:02d}"
            frames[key] = {
                "frame": {"x": x, "y": y, "w": CELL, "h": CELL},
                "rotated": False,
                "trimmed": False,
                "spriteSourceSize": {"x": 0, "y": 0, "w": CELL, "h": CELL},
                "sourceSize": {"w": CELL, "h": CELL},
                "pivot": {"x": 0.5, "y": 0.88},
                "index": index,
            }
            index += 1

    png_name = f"{palette_name}.png"
    sheet.save(out_dir / png_name, optimize=True)
    return {
        "image": png_name,
        "size": {"w": sheet.width, "h": sheet.height},
        "frames": frames,
    }


def radial_glow(size: Tuple[int, int], color: Tuple[int, int, int], alpha: int) -> Image.Image:
    w, h = size
    img = Image.new("RGBA", size, (0, 0, 0, 0))
    px = img.load()
    cx, cy = w / 2, h / 2
    maxd = math.hypot(cx, cy)
    for y in range(h):
        for x in range(w):
            d = math.hypot(x - cx, y - cy) / maxd
            a = int(alpha * max(0.0, 1.0 - d) ** 2)
            px[x, y] = (*color, a)
    return img


def generate_octagon(out_dir: Path) -> None:
    w, h = 1600, 900
    img = Image.new("RGBA", (w, h), (10, 12, 18, 255))
    draw = ImageDraw.Draw(img)
    img.alpha_composite(radial_glow((w, h), (80, 180, 210), 90))
    draw.rectangle((0, int(h * 0.72), w, h), fill=(8, 8, 11, 255))

    # Crowd silhouettes and arena LEDs.
    for i in range(36):
        x0 = int(i * w / 36)
        col = (24, 25, 32, 255) if i % 2 else (31, 30, 37, 255)
        draw.rectangle((x0, int(h * 0.60 + (i % 4) * 8), x0 + 44, int(h * 0.74)), fill=col)
    for x in range(90, w, 145):
        draw.ellipse((x - 17, 82, x + 17, 116), fill=(255, 211, 105, 210))
        draw.line((x, 116, w / 2, h * 0.52), fill=(255, 211, 105, 38), width=5)

    cx, cy = w / 2, h * 0.60
    rx, ry = 555, 245
    pts = []
    for i in range(8):
        a = -math.pi / 8 + i * math.tau / 8
        pts.append((cx + math.cos(a) * rx, cy + math.sin(a) * ry))

    # Back cage panel, kept behind the mat so it frames the fight without
    # covering the characters.
    cage_top = cy - 315
    cage_bottom = cy - 118
    cage_left = cx - rx * 0.80
    cage_right = cx + rx * 0.80
    cage = [
        (cage_left + 70, cage_top),
        (cage_right - 70, cage_top),
        (cage_right, cage_bottom),
        (cage_left, cage_bottom),
    ]
    draw.polygon(cage, fill=(14, 23, 30, 82), outline=(146, 165, 172, 95))
    for i in range(7):
        t = i / 6
        y = lerp(cage_top + 15, cage_bottom - 10, t)
        inset = lerp(58, 6, t)
        draw.line((cage_left + inset, y, cage_right - inset, y), fill=(138, 158, 166, 92), width=2)
    for i in range(15):
        t = i / 14
        top_x = lerp(cage_left + 74, cage_right - 74, t)
        bot_x = lerp(cage_left + 6, cage_right - 6, t)
        draw.line((top_x, cage_top + 4, bot_x, cage_bottom - 6), fill=(138, 158, 166, 76), width=2)
    for x in (cage_left + 8, cage_right - 8, cx):
        draw.line((x, cage_top - 18, x, cage_bottom + 25), fill=(190, 204, 205, 110), width=5)

    draw.polygon(pts, fill=(41, 45, 52, 255), outline=(210, 222, 222, 255))
    for offset, color, width_line in [
        (0, (222, 231, 231, 255), 10),
        (22, (33, 36, 44, 255), 14),
        (42, (174, 184, 188, 255), 4),
    ]:
        ring = []
        for i in range(8):
            a = -math.pi / 8 + i * math.tau / 8
            ring.append((cx + math.cos(a) * (rx - offset), cy + math.sin(a) * (ry - offset)))
        draw.line(ring + [ring[0]], fill=color, width=width_line, joint="curve")

    mat = []
    for i in range(8):
        a = -math.pi / 8 + i * math.tau / 8
        mat.append((cx + math.cos(a) * (rx - 58), cy + math.sin(a) * (ry - 58)))
    draw.polygon(mat, fill=(74, 81, 89, 255))
    draw.line(mat + [mat[0]], fill=(188, 198, 200, 230), width=4)
    draw.line((cx - 280, cy + 120, cx + 280, cy - 120), fill=(112, 121, 130, 130), width=3)
    draw.line((cx - 280, cy - 120, cx + 280, cy + 120), fill=(112, 121, 130, 130), width=3)
    draw.ellipse((cx - 150, cy - 66, cx + 150, cy + 66), outline=(220, 230, 226, 125), width=4)

    # Red and blue corners.
    draw.polygon([pts[0], pts[1], ((pts[0][0] + pts[1][0]) / 2, (pts[0][1] + pts[1][1]) / 2 + 42)], fill=(52, 104, 136, 180))
    draw.polygon([pts[4], pts[5], ((pts[4][0] + pts[5][0]) / 2, (pts[4][1] + pts[5][1]) / 2 - 42)], fill=(140, 52, 58, 180))

    # Front posts give the octagon depth, but avoid drawing cage lines across
    # the playable mat.
    for p in (pts[0], pts[1], pts[6], pts[7]):
        draw.line((p[0], p[1], p[0], p[1] - 145), fill=(188, 202, 204, 125), width=6)
        draw.ellipse((p[0] - 10, p[1] - 154, p[0] + 10, p[1] - 134), fill=(188, 202, 204, 150))

    img = img.filter(ImageFilter.UnsharpMask(radius=1.2, percent=105, threshold=3))
    img.save(out_dir / "ocotogono.png", optimize=True)


def write_atlas(out_dir: Path, sheet_info: Dict[str, object]) -> None:
    atlas = {
        "meta": {
            "app": "RayDrone procedural MMA asset generator",
            "version": "1.0",
            "image": sheet_info["image"],
            "format": "RGBA8888",
            "size": sheet_info["size"],
            "scale": "1",
            "note": "Generated transparent 192x192 cells. Frame names match game.html MMA state keys.",
        },
        "grid": {
            "columns": COLS,
            "virtualCell": {"w": CELL, "h": CELL},
            "origin": "top-left",
        },
        "animations": {state: [f"{state}_{i + 1:02d}" for i in range(count)] for state, count in ANIMATIONS},
        "frames": sheet_info["frames"],
    }
    with (out_dir / "warrior1_mma_atlas.json").open("w", encoding="utf-8") as f:
        json.dump(atlas, f, indent=2)
        f.write("\n")


def main() -> None:
    parser = argparse.ArgumentParser(description="Generate Level 6 MMA sprites and atlas JSON.")
    parser.add_argument("--out", default="sprites/generated_mma", help="Output directory.")
    args = parser.parse_args()

    out_dir = Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)
    first_sheet = build_sheet("warrior1", out_dir)
    build_sheet("warrior", out_dir)
    write_atlas(out_dir, first_sheet)
    generate_octagon(out_dir)
    print(f"Generated MMA assets in {out_dir}")
    print(f"Frames: {len(first_sheet['frames'])}, sheet: {first_sheet['size']['w']}x{first_sheet['size']['h']}")


if __name__ == "__main__":
    main()
