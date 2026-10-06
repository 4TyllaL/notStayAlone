"""Desenha os quatro mascotes embutidos em 32x32, com volume (luz e sombra),
contorno colorido e todas as poses, e grava `assets/mascots/<id>/mascot.txt`.

    python tools/mascots.py            # regrava os mascotes
    python tools/mascots.py --preview  # e gera uma folha de conferencia em PNG

Tudo vem de formas simples (elipses, triangulos) desenhadas por camadas: o corpo,
as orelhas e o rabo ganham sombra embaixo/a direita e brilho em cima/a esquerda a
partir da propria silhueta; o rosto vem por cima; o contorno e a cor escura do
material vizinho (contorno seletivo), nao preto chapado.
"""
import os
import struct
import sys
import zlib

S = 32
ROOT = os.path.join(os.path.dirname(__file__.replace("\\", "/")) or ".", "..")

FRAMES = ["idle", "idle2", "blink", "look", "walk1", "walk2", "yawn", "sleep1", "sleep2",
          "held1", "held2", "fall", "land", "happy1", "happy2", "eat1", "eat2", "food"]

# Letras do arquivo: cada material tem (base, luz, sombra, contorno).
LETTERS = {}


def hexrgb(h):
    return tuple(int(h[i:i + 2], 16) for i in (0, 2, 4))


def mix(a, b, t):
    a, b = hexrgb(a), hexrgb(b)
    return "%02x%02x%02x" % tuple(round(x + (y - x) * t) for x, y in zip(a, b))


class Palette:
    """Cores do mascote: cada material vira 4 letras (base, luz, sombra, contorno)."""

    def __init__(self):
        self.colors = {}   # letra -> hex
        self.mat = {}      # material -> (base, luz, sombra, contorno) em letras
        self.free = iter("abcdefgijlmnoqrstuvxyABCDEFGIJLMNOQRSTUVXYhkpwzHKPWZ0123456789+*=%&@$!?")

    def letter(self, hexcolor):
        for k, v in self.colors.items():
            if v == hexcolor:
                return k
        k = next(self.free)
        self.colors[k] = hexcolor
        return k

    def material(self, name, base, light=None, shade=None, line=None):
        light = light or mix(base, "fff6e0", 0.35)
        shade = shade or mix(base, "5a2a4a", 0.30)
        line = line or mix(base, "2b1e2f", 0.72)
        self.mat[name] = tuple(self.letter(c) for c in (base, light, shade, line))


class Canvas:
    def __init__(self, pal):
        self.pal = pal
        self.px = {}      # (x, y) -> (material, grupo)
        self.top = {}     # rosto e detalhes: (x, y) -> letra, sem sombra
        self.extra = {}   # zzz e coracoes, desenhados por ultimo com contorno proprio

    def put(self, x, y, mat, group):
        if 0 <= x < S and 0 <= y < S:
            self.px[(x, y)] = (mat, group)

    def ellipse(self, cx, cy, rx, ry, mat, group, power=2.0, clip=None):
        for y in range(S):
            for x in range(S):
                dx, dy = abs(x + 0.5 - cx) / rx, abs(y + 0.5 - cy) / ry
                if dx ** power + dy ** power <= 1 and (clip is None or (x, y) in clip):
                    self.put(x, y, mat, group)

    def tri(self, a, b, c, mat, group):
        def side(p, q, r):
            return (p[0] - r[0]) * (q[1] - r[1]) - (q[0] - r[0]) * (p[1] - r[1])
        for y in range(S):
            for x in range(S):
                p = (x + 0.5, y + 0.5)
                d1, d2, d3 = side(p, a, b), side(p, b, c), side(p, c, a)
                if not ((d1 < 0 or d2 < 0 or d3 < 0) and (d1 > 0 or d2 > 0 or d3 > 0)):
                    self.put(x, y, mat, group)

    def dots(self, pts, letter, layer=None):
        layer = self.top if layer is None else layer
        for x, y in pts:
            if 0 <= x < S and 0 <= y < S:
                layer[(x, y)] = letter

    def mask(self, group):
        return {p for p, (_, g) in self.px.items() if g == group}

    def render(self, outline_key):
        """Sombra por grupo, rosto, contorno seletivo e extras. Devolve as 32 linhas."""
        out = {}
        groups = {g for _, g in self.px.values()}
        for g in groups:
            m = self.mask(g)
            for (x, y) in m:
                mat = self.px[(x, y)][0]
                base, light, shade, _ = self.pal.mat[mat]
                c = base
                if (x + 1, y + 1) not in m or (x, y + 2) not in m or (x + 2, y) not in m and y > 0 and (x + 2, y - 1) not in m:
                    c = shade
                elif (x - 1, y - 1) not in m and (x, y - 1) not in m:
                    c = light
                out[(x, y)] = c
        out.update(self.top)
        # Contorno: cor escura do material vizinho; embaixo, o escuro do mascote.
        filled = set(out)
        for y in range(S):
            for x in range(S):
                if (x, y) in filled:
                    continue
                for nx, ny in ((x, y - 1), (x - 1, y), (x + 1, y), (x, y + 1)):
                    if (nx, ny) in self.px:
                        line = self.pal.mat[self.px[(nx, ny)][0]][3]
                        out[(x, y)] = outline_key if ny < y else line
                        break
        filled = set(self.extra)
        for (x, y), k in self.extra.items():
            out[(x, y)] = k
        for (x, y) in list(filled):
            for nx, ny in ((x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)):
                if (nx, ny) not in filled and (nx, ny) not in out and 0 <= nx < S and 0 <= ny < S:
                    out[(nx, ny)] = self.pal.mat["zline"][0]
        return ["".join(out.get((x, y), ".") for x in range(S)) for y in range(S)]


# ---------------------------------------------------------------- rosto e poses

POSES = {
    #            dy  sx    sy    olhos      boca     pes        extra
    "idle":   (0, 1.00, 1.00, "open", "rest", "stand", None),
    "idle2":  (0, 1.00, 1.00, "open", "rest", "stand", "fidget"),
    "blink":  (0, 1.00, 1.00, "closed", "rest", "stand", None),
    "look":   (0, 1.00, 1.00, "look", "rest", "stand", None),
    "walk1":  (0, 1.00, 1.00, "look", "rest", "wide", "wag"),
    "walk2":  (-1, 1.00, 1.00, "look", "rest", "narrow", None),
    "yawn":   (0, 1.00, 1.02, "squint", "yawn", "stand", None),
    "sleep1": (1, 1.04, 0.94, "sleep", "rest", "tuck", "z1"),
    "sleep2": (1, 1.05, 0.92, "sleep", "rest", "tuck", "z2"),
    "held1":  (-1, 0.94, 1.07, "wide", "o", "dangle1", None),
    "held2":  (-1, 0.94, 1.07, "wide", "o", "dangle2", None),
    "fall":   (-1, 0.92, 1.10, "xx", "yawn", "spread", None),
    "land":   (2, 1.12, 0.84, "closed", "rest", "spread", None),
    "happy1": (0, 1.00, 1.00, "happy", "smile", "stand", "heart"),
    "happy2": (-3, 0.96, 1.05, "happy", "smile", "jump", "wag"),
    "eat1":   (0, 1.00, 1.00, "happy", "chew1", "stand", None),
    "eat2":   (1, 1.02, 0.97, "happy", "chew2", "stand", None),
}

# Base do corpo (o centro desce/sobe com dy; os pes ficam no chao).
CX, CY, RX, RY = 16.0, 21.5, 12.5, 8.6


def body_geom(dy, sx, sy):
    ry = RY * sy
    rx = RX * sx
    bottom = 29.6 + max(dy, 0) * 0 + (dy if dy < 0 else 0)
    cy = bottom - ry
    if dy > 0:
        cy += 0  # agachado: o corpo ja encolheu pelo sy
    return CX, cy, rx, ry


def draw_eyes(c, kind, ex, ey, pal):
    k, w = pal.letter("2b1e2f"), pal.letter("ffffff")
    for x0 in ex:
        x, y = x0, ey
        if kind in ("open", "look"):
            dx = 1 if kind == "look" else 0
            c.dots([(x + dx, y), (x + 1 + dx, y), (x + dx, y + 1), (x + 1 + dx, y + 1), (x + dx, y + 2), (x + 1 + dx, y + 2)], k)
            c.dots([(x + 1 + dx, y)], w)
            c.dots([(x + dx, y + 2)], pal.letter("4a3a5a"))
        elif kind == "wide":
            c.dots([(x + i, y + j) for i in range(-1, 2) for j in range(0, 3)] + [(x + 2, y + 1)], k)
            c.dots([(x, y), (x + 1, y + 1)], w)
        elif kind == "closed":
            c.dots([(x - 1, y + 2), (x, y + 2), (x + 1, y + 2), (x + 2, y + 2)], k)
        elif kind == "squint":
            c.dots([(x - 1, y + 1), (x, y + 2), (x + 1, y + 2), (x + 2, y + 1)], k)
        elif kind == "sleep":
            c.dots([(x - 1, y + 1), (x, y + 2), (x + 1, y + 2), (x + 2, y + 1)], k)
        elif kind == "happy":
            c.dots([(x - 1, y + 2), (x, y + 1), (x + 1, y + 1), (x + 2, y + 2)], k)
        elif kind == "xx":
            left = x0 == ex[0]
            pts = [(x, y), (x + 1, y + 1), (x, y + 2)] if left else [(x + 1, y), (x, y + 1), (x + 1, y + 2)]
            c.dots(pts, k)


def draw_mouth(c, kind, mx, my, pal, species):
    k, p, r = pal.letter("2b1e2f"), pal.letter("f07a93"), pal.letter("c2405f")
    if kind == "rest":
        if species == "cat":
            c.dots([(mx - 2, my), (mx - 1, my + 1), (mx, my), (mx + 1, my + 1), (mx + 2, my)], k)
        elif species == "dog":
            c.dots([(mx - 1, my - 1), (mx, my - 1), (mx + 1, my - 1), (mx, my)], k)
            c.dots([(mx - 1, my + 1), (mx + 1, my + 1)], k)
        elif species == "bunny":
            c.dots([(mx, my - 1)], pal.letter("f07a93"))
            c.dots([(mx - 1, my + 1), (mx, my), (mx + 1, my + 1)], k)
        else:
            c.dots([(mx - 1, my), (mx, my + 1), (mx + 1, my + 1), (mx + 2, my)], k)
    elif kind == "smile":
        c.dots([(mx - 2, my), (mx + 2, my), (mx - 1, my + 1), (mx + 1, my + 1), (mx - 1, my), (mx, my), (mx + 1, my)], k)
        c.dots([(mx, my + 1)], p)
        c.dots([(mx - 1, my + 2), (mx, my + 2), (mx + 1, my + 2)], k)
    elif kind == "yawn":
        c.dots([(mx + i, my + j) for i in (-1, 0, 1) for j in (0, 1, 2)], k)
        c.dots([(mx - 1, my + 2), (mx, my + 2), (mx + 1, my + 2)], p)
        c.dots([(mx - 2, my + 1), (mx + 2, my + 1), (mx - 1, my + 3), (mx, my + 3), (mx + 1, my + 3)], k)
    elif kind == "o":
        c.dots([(mx, my), (mx - 1, my + 1), (mx + 1, my + 1), (mx, my + 2)], k)
        c.dots([(mx, my + 1)], r)
    elif kind == "chew1":
        c.dots([(mx - 1, my), (mx, my + 1), (mx + 1, my)], k)
    elif kind == "chew2":
        c.dots([(mx - 1, my), (mx, my), (mx + 1, my), (mx - 1, my + 1), (mx + 1, my + 1), (mx, my + 1)], k)
        c.dots([(mx, my + 1)], p)


def draw_feet(c, kind, bottom):
    y = round(bottom) - 1
    spots = {
        "stand": [(10.5, y, 0), (21.5, y, 0)],
        "wide": [(8.5, y, 0), (23.5, y, 0)],
        "narrow": [(12.5, y, 0), (19.5, y, 0)],
        "tuck": [],
        "dangle1": [(10.5, y + 2, 0), (21.5, y + 1, 0)],
        "dangle2": [(10.5, y + 1, 0), (21.5, y + 2, 0)],
        "spread": [(7.5, y, 0), (24.5, y, 0)],
        "jump": [(11.5, y + 1, 0), (20.5, y + 1, 0)],
    }[kind]
    for fx, fy, _ in spots:
        c.ellipse(fx, fy + 0.5, 2.6, 1.6, "foot", "foot")


def heart(c, x, y, pal):
    h = pal.letter("ef476f")
    hl = pal.letter("ff9fb5")
    rows = [".hh.hh.", "hlhhhhh", "hhhhhhh", ".hhhhh.", "..hhh..", "...h..."]
    for j, row in enumerate(rows):
        for i, ch in enumerate(row):
            if ch != ".":
                c.extra[(x + i, y + j)] = hl if ch == "l" else h


def zzz(c, x, y, pal, big):
    z = pal.letter("9fc5ff")
    small = ["zzz", "..z", ".z.", "zzz"]
    large = ["zzzz", "...z", "..z.", ".z..", "zzzz"]
    for j, row in enumerate(large if big else small):
        for i, ch in enumerate(row):
            if ch == "z":
                c.extra[(x + i, y + j)] = z


# ---------------------------------------------------------------- especies

def cat_parts(c, cx, cy, rx, ry, top, extra):
    # rabo enrolado atras, a esquerda
    tip = (2, 14) if extra == "fidget" else (3, 15)
    for i, (x, y) in enumerate([(5, 26), (4, 25), (3, 23), (3, 21), (3, 19), (3, 17), tip]):
        c.ellipse(x + 0.5, y + 0.5, 1.6, 1.6, "fur", "tail")
    c.ellipse(tip[0] + 0.5, tip[1] + 0.5, 1.7, 1.7, "tip", "tail")
    # orelhas
    ear_r = (25.5, top - 6.5) if extra != "fidget" else (27.0, top - 5.5)
    c.tri((6.0, top + 4), (7.5, top - 6.5), (14.0, top + 2), "fur", "earL")
    c.tri((26.0, top + 4), ear_r, (18.0, top + 2), "fur", "earR")
    c.tri((8.0, top + 2.5), (8.4, top - 3.5), (12.0, top + 2), "pink", "earL")
    c.tri((24.0, top + 2.5), (ear_r[0] - 1.6, top - 3.5), (20.0, top + 2), "pink", "earR")


def dog_parts(c, cx, cy, rx, ry, top, extra):
    wag = extra in ("wag",)
    tail = [(5, 24), (4, 22), (3, 20), (3, 18)] if not wag else [(5, 24), (4, 23), (2, 22), (1, 21)]
    for x, y in tail:
        c.ellipse(x + 0.5, y + 0.5, 1.5, 1.5, "fur", "tail")


def dog_front(c, cx, cy, rx, ry, top, extra):
    lift = 1.5 if extra == "fidget" else 0
    c.ellipse(cx - rx + 2.0, top + 6.0 - lift, 3.0, 5.6, "ear", "earL", power=2.2)
    c.ellipse(cx + rx - 2.0, top + 6.0, 3.0, 5.6, "ear", "earR", power=2.2)


def bunny_parts(c, cx, cy, rx, ry, top, extra):
    c.ellipse(4.5, 25.0, 2.8, 2.6, "tail", "tail")
    bend = extra == "fidget"
    c.ellipse(11.5, top - 5.0, 2.6, 7.2, "fur", "earL")
    c.ellipse(11.5, top - 4.5, 1.1, 5.2, "pink", "earL")
    if bend:
        c.ellipse(22.0, top - 3.5, 2.6, 5.6, "fur", "earR")
        c.ellipse(24.5, top - 8.0, 2.4, 2.4, "fur", "earR")
        c.ellipse(22.0, top - 3.0, 1.1, 3.8, "pink", "earR")
    else:
        c.ellipse(20.5, top - 5.0, 2.6, 7.2, "fur", "earR")
        c.ellipse(20.5, top - 4.5, 1.1, 5.2, "pink", "earR")


def dino_parts(c, cx, cy, rx, ry, top, extra):
    up = 2 if extra in ("fidget", "wag") else 0
    c.tri((6.0, 22.0), (0.6, 27.6 - up), (7.0, 28.5), "fur", "tail")
    for x in (10.0, 15.5, 21.0):
        c.tri((x - 2.6, top + 2.2), (x, top - 3.2), (x + 2.6, top + 2.2), "spike", "spike%d" % x)


def dino_front(c, cx, cy, rx, ry, top, extra):
    y = top + 0.5
    c.tri((21.0, y - 3.6), (21.0, y + 2.4), (25.0, y - 0.5), "bow", "bowL")
    c.tri((29.6, y - 3.6), (29.6, y + 2.4), (25.6, y - 0.5), "bow", "bowR")
    c.ellipse(25.4, y - 0.5, 1.4, 1.4, "bowknot", "bowK")


SPECIES = {
    "calcifer": dict(kind="cat", back=cat_parts, front=None,
                     fur="f4a259", belly="ffe8c2", foot="d9853b", extra={"pink": "f59bb0", "tip": "ffe8c2"}),
    "lance": dict(kind="dog", back=dog_parts, front=dog_front,
                  fur="d99a4e", belly="f7e3bd", foot="b97c35", extra={"ear": "9b5a24"}),
    "zeze": dict(kind="bunny", back=bunny_parts, front=None,
                 fur="f0cb6e", belly="fff3d9", foot="d9ad4f", extra={"pink": "f5a3b5", "tail": "fffaf0"}),
    "jujubs": dict(kind="dino", back=dino_parts, front=dino_front,
                   fur="7ccf6a", belly="fff0c9", foot="5aa84c",
                   extra={"spike": "ffb347", "bow": "ff6fa8", "bowknot": "e0457f"}),
}


def palette_for(spec):
    pal = Palette()
    pal.letter("2b1e2f")  # 'a' = olhos/contorno de cima
    pal.material("fur", spec["fur"])
    pal.material("belly", spec["belly"], shade=mix(spec["belly"], spec["fur"], 0.45))
    pal.material("foot", spec["foot"])
    for name, color in spec["extra"].items():
        pal.material(name, color)
    pal.material("zline", "6b8fc7")
    pal.material("cheek", "f59bb0")
    return pal


def draw(id_, frame):
    spec = SPECIES[id_]
    pal = PALETTES[id_]
    c = Canvas(pal)
    if frame == "food":
        FOODS[id_](c, pal)
        return c.render(pal.letter("2b1e2f"))
    dy, sx, sy, eyes, mouth, feet, extra = POSES[frame]
    ry, rx = RY * sy, RX * sx
    bottom = 29.6 + min(dy, 0)
    cy = bottom - ry
    top = cy - ry
    if feet == "tuck":
        bottom = 29.6
    spec["back"](c, CX, cy, rx, ry, top, extra)
    draw_feet(c, feet, bottom + 0.4)
    c.ellipse(CX, cy, rx, ry, "fur", "body", power=2.6)
    body = c.mask("body")
    c.ellipse(CX, cy + ry * 0.55, rx * 0.62, ry * 0.62, "belly", "body", clip=body)
    if spec["front"]:
        spec["front"](c, CX, cy, rx, ry, top, extra)
    # rosto
    ey = round(cy - ry * 0.25)
    draw_eyes(c, eyes, (9, 21), ey, pal)
    cheek = pal.mat["cheek"][0]
    if eyes != "xx":
        c.dots([(6, ey + 3), (7, ey + 3), (8, ey + 3), (23, ey + 3), (24, ey + 3), (25, ey + 3)], cheek)
    mx = 16 + (1 if eyes == "look" else 0)
    draw_mouth(c, mouth, mx, ey + 3, pal, spec["kind"])
    if extra == "heart":
        heart(c, 22, 1, pal)
    if extra in ("z1", "z2"):
        zzz(c, 22 if extra == "z1" else 24, 1 if extra == "z1" else 0, pal, extra == "z2")
        zzz(c, 18 if extra == "z1" else 19, 6 if extra == "z1" else 6, pal, False)
    return c.render(pal.letter("2b1e2f"))


# ---------------------------------------------------------------- comidas

def food_fish(c, pal):
    pal.material("fish", "8ab4d8")
    c.ellipse(14.0, 27.0, 6.0, 3.2, "fish", "fish")
    c.tri((19.0, 27.0), (24.5, 23.5), (24.5, 30.5), "fish", "fish")
    c.dots([(11, 26)], pal.letter("2b1e2f"))
    c.dots([(15, 25), (15, 26), (15, 27), (15, 28)], pal.mat["fish"][2])


def food_bone(c, pal):
    pal.material("bone", "fdf6e3")
    c.ellipse(16.0, 27.0, 6.5, 1.8, "bone", "bone")
    for x in (9.0, 23.0):
        c.ellipse(x, 25.6, 2.1, 2.1, "bone", "bone")
        c.ellipse(x, 28.4, 2.1, 2.1, "bone", "bone")


def food_carrot(c, pal):
    pal.material("carrot", "f28c28")
    pal.material("leaf", "6cbf4f")
    c.tri((10.0, 25.0), (24.0, 28.0), (10.0, 30.6), "carrot", "carrot")
    c.ellipse(9.5, 27.8, 2.6, 2.8, "carrot", "carrot")
    c.tri((7.5, 27.5), (3.0, 23.0), (5.5, 28.5), "leaf", "leaf")
    c.tri((7.5, 27.0), (5.5, 21.5), (8.5, 26.0), "leaf", "leaf")
    c.dots([(14, 27), (18, 28)], pal.mat["carrot"][2])


def food_jelly(c, pal):
    for name, color, x, y in (("j1", "ef476f", 9.5, 27.5), ("j2", "4cc9f0", 16.0, 28.0), ("j3", "ffd23f", 22.5, 27.5), ("j4", "7ccf6a", 13.0, 24.0)):
        pal.material(name, color)
        c.ellipse(x, y, 2.8, 1.9, name, name)
        c.dots([(int(x) - 1, int(y) - 1)], pal.letter("ffffff"))


FOODS = {"calcifer": food_fish, "lance": food_bone, "zeze": food_carrot, "jujubs": food_jelly}
PALETTES = {k: palette_for(v) for k, v in SPECIES.items()}


# ---------------------------------------------------------------- saida

def header(path):
    """Mantem o comentario e as linhas name/article/about do arquivo atual."""
    keep = []
    with open(path, encoding="utf-8") as f:
        for line in f:
            if line.startswith("color") or line.startswith("frame") or line.startswith("size"):
                break
            if not line.startswith("# Gerado por"):
                keep.append(line.rstrip("\n"))
    while keep and not keep[-1].strip():
        keep.pop()
    return keep


def build(id_):
    frames = {f: draw(id_, f) for f in FRAMES}  # desenha antes: comidas criam cores
    pal = PALETTES[id_]
    path = os.path.join(ROOT, "assets", "mascots", id_, "mascot.txt")
    lines = header(path)
    lines.append("# Gerado por tools/mascots.py (32x32): edite o script e rode de novo.")
    lines.append("")
    lines.append("size 32")
    for k, v in pal.colors.items():
        lines.append("color %s %s" % (k, v))
    for name in FRAMES:
        lines.append("")
        lines.append("frame " + name)
        lines.extend(frames[name])
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(lines) + "\n")
    return frames, pal


def png(path, w, h, rgba):
    raw = b"".join(b"\0" + bytes(rgba[y * w * 4:(y + 1) * w * 4]) for y in range(h))

    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d))
    data = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
    data += chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")
    with open(path, "wb") as f:
        f.write(data)


def preview(sheets, path, scale=4):
    cols, rows = len(FRAMES), len(sheets)
    cell = S * scale + 8
    w, h = cols * cell, rows * cell
    buf = bytearray([0xf4, 0xef, 0xe6, 255] * (w * h))
    for r, (frames, pal) in enumerate(sheets):
        for ci, name in enumerate(FRAMES):
            for y, row in enumerate(frames[name]):
                for x, ch in enumerate(row):
                    if ch == ".":
                        continue
                    rgb = hexrgb(pal.colors[ch])
                    for yy in range(scale):
                        for xx in range(scale):
                            px = ci * cell + 4 + x * scale + xx
                            py = r * cell + 4 + y * scale + yy
                            i = (py * w + px) * 4
                            buf[i:i + 4] = bytes((*rgb, 255))
    png(path, w, h, buf)


if __name__ == "__main__":
    sheets = [build(i) for i in SPECIES]
    if "--preview" in sys.argv:
        out = sys.argv[sys.argv.index("--preview") + 1] if len(sys.argv) > sys.argv.index("--preview") + 1 else "preview.png"
        preview(sheets, out)
        print("prÃ©via em", out)
