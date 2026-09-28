"""Gera gallery/index.json com o SHA-256 de cada arquivo da galeria.

Rode antes de publicar mudanças na galeria:
    python tools/gallery.py

Cada item é uma pasta em gallery/mascots/<id>/ (com mascot.txt) ou
gallery/plugins/<id>/ (com plugin.ini). O autor vem de um author.txt opcional.
"""
import hashlib
import json
import os
import re

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "gallery")
ID = re.compile(r"^[a-z0-9_-]{1,40}$")
FILE = re.compile(r"^[A-Za-z0-9._-]{1,60}$")
KINDS = (("mascote", "mascots", "mascot.txt"), ("plugin", "plugins", "plugin.ini"))


def meta(path, keys):
    """Lê 'name'/'about' de um mascot.txt ('name X') ou plugin.ini ('name = X')."""
    found = {}
    with open(path, encoding="utf-8-sig") as f:
        for line in f:
            line = line.strip()
            for key in keys:
                m = re.match(rf"^{key}\s*=?\s+(.*)$", line) or re.match(rf"^{key}\s*=\s*(.*)$", line)
                if m and key not in found:
                    found[key] = m.group(1).strip()
    return found


items = []
for kind, folder, required in KINDS:
    base = os.path.join(ROOT, folder)
    for item in sorted(os.listdir(base)) if os.path.isdir(base) else []:
        path = os.path.join(base, item)
        if not os.path.isdir(path):
            continue
        if not ID.match(item):
            raise SystemExit(f"id inválido: {folder}/{item}")
        names = sorted(n for n in os.listdir(path) if n != "author.txt" and not n.startswith("."))
        if required not in names:
            raise SystemExit(f"{folder}/{item} sem {required}")
        files = []
        for name in names:
            if not FILE.match(name):
                raise SystemExit(f"nome de arquivo inválido: {folder}/{item}/{name}")
            with open(os.path.join(path, name), "rb") as f:
                files.append({"path": f"{folder}/{item}/{name}", "sha256": hashlib.sha256(f.read()).hexdigest()})
        info = meta(os.path.join(path, required), ("name", "about"))
        author_file = os.path.join(path, "author.txt")
        author = open(author_file, encoding="utf-8").read().strip() if os.path.exists(author_file) else "4TyllaL"
        items.append({"kind": kind, "id": item, "name": info.get("name", item), "author": author,
                      "about": info.get("about", ""), "files": files})

with open(os.path.join(ROOT, "index.json"), "w", encoding="utf-8", newline="\n") as f:
    json.dump({"version": 1, "items": items}, f, ensure_ascii=False, indent=2)
    f.write("\n")
print(f"{len(items)} itens em gallery/index.json")
