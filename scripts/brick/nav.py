"""Rendered D-pad rows and focus, shared by the Brick device journey.

The geometry matches tests/nav_explorer.rs and the existing RC walkthrough:
compare drawn controls with controls actually reached, rather than guessing
that the page ends when its text stops changing. This module runs on the host;
no Python installation is needed on the Brick.
"""
import re

FOCUS, ROW, HINT = '‹nav:focus›', '‹nav:row›', '‹nav:hint›'


def clock(s):
    s = s.strip()
    return bool(re.fullmatch(r'\d{1,2}:\d\d', s))


def inside(p, r):
    return r[0] <= p[0] < r[2] and r[1] <= p[1] < r[3]


def centre(r):
    return ((r[0] + r[2]) // 2, (r[1] + r[3]) // 2)


def overlap(a, b):
    w = min(a[2], b[2]) - max(a[0], b[0]); h = min(a[3], b[3]) - max(a[1], b[1])
    return 0 if w <= 0 or h <= 0 else w * h


def area(r):
    return max(0, r[2] - r[0]) * max(0, r[3] - r[1])


class Frame:
    def __init__(self, text):
        self.raw = text
        self.strings, self.rows, self.focus, self.hints = [], [], [], []
        for line in text.splitlines():
            v = line.split('\t', 4)
            if len(v) < 5:
                continue
            try:
                r = tuple(int(x) for x in v[:4])
            except ValueError:
                continue
            s = v[4]
            if s == FOCUS:
                self.focus.append(r)
            elif s == ROW:
                self.rows.append(r)
            elif s.startswith(HINT):
                k, _, l = s[len(HINT):].partition('\t')
                self.hints.append((k, l))
            elif not s.startswith('‹') and s not in ('>', '<') and s.strip():
                self.strings.append((r, s))

    def text(self):
        return '\n'.join(s.rstrip('.…') for _, s in self.strings if not clock(s))

    def controls(self):
        rects = []
        for r in self.rows:
            if not any(overlap(o, r) * 2 > min(area(o), area(r)) for o in rects):
                rects.append(r)
        for r in self.focus:
            if not any(overlap(o, r) * 2 > min(area(o), area(r)) for o in rects):
                rects.append(r)
        out = []
        for r in rects:
            inner = sorted([(sr, s) for sr, s in self.strings if inside(centre(sr), r)], key=lambda t: (t[0][0] // 8, t[0][1]))
            if not inner:
                continue
            name_at, name = inner[0]
            vals = ' | '.join(s for _, s in inner[1:])
            foc = any(inside(centre(r), f) or overlap(f, r) * 2 > area(r) or (overlap(f, r) > 0 and inside(centre(name_at), f)) for f in self.focus)
            out.append({'rect': r, 'name': name, 'name_rect': name_at, 'value': vals, 'focused': foc})
        out.sort(key=lambda c: (c['rect'][1], c['rect'][0]))
        return out

    def focused(self):
        for c in self.controls():
            if c['focused']:
                return c['name']
        return None

    def focused_ctl(self):
        for c in self.controls():
            if c['focused']:
                return c
        return None

    def candidates(self):
        """Strings in the focused row's name column between the title and the help line: the rows drawn (names) as far as the
        record tells without the App's touch list."""
        fc = self.focused_ctl()
        if not fc:
            return []
        x0 = fc['name_rect'][0]
        out = []
        H = max([r[3] for r, _ in self.strings] + [r[3] for r in self.focus] + [1]) / 0.97
        for r, s in self.strings:
            if abs(r[0] - x0) <= 6 * H / 960 and 0.104 * H <= r[1] < 0.81 * H and not clock(s) and s.strip() not in ('>', '<'):
                out.append(s.replace('\u200b', '').strip())
        return out

    def title(self):
        # the topmost wide string that is not a control name, first line of the page
        ctl_names = {c['name'] for c in self.controls()}
        width=max((r[2] for r,_ in self.strings),default=1)
        height=max([r[3] for r,_ in self.strings]+[r[3] for r in self.focus]+[1])/.97
        cand = [(r, s) for r, s in self.strings if not clock(s) and s not in ctl_names
                and r[1] < max(80,height*.22) and r[0] < width/2]
        cand.sort(key=lambda t: (t[0][1], t[0][0]))
        return cand[0][1] if cand else ''

    def sig(self):
        # what a player compares: page title, the rows' names, the focused row with its value, the hint bar. Not the help lines
        # or live readouts (temperatures, clock) that change by themselves.
        fc = self.focused_ctl()
        inf = tuple(s for r, s in self.strings if any(inside(centre(r), f) for f in self.focus) and not clock(s)) if self.focus else ()
        return (self.title(), tuple(self.candidates()), self.focused(), fc['value'] if fc else None, inf, tuple(self.hints))
