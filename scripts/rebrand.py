#!/usr/bin/env python3
"""Rebrand the repository to Retinue. Rerunnable: on a tree that is already rebranded it changes nothing.

    python scripts/rebrand.py            # apply
    python scripts/rebrand.py --dry-run  # list what would change

Steps, in order:
1. Collect the files: every tracked file (git ls-files) plus the untracked apps/native
   workspace when it is present (build output is skipped).
2. Rename files and folders whose names carry an old name (git mv for tracked files).
   Binary image and icon files keep their names.
3. Rewrite text content. Protected spans keep their old spelling: the GitHub repository
   slug, the checkout folder in absolute paths, and binary image or icon file names.
   Look ids come first: blobots becomes stone, and bitbots becomes frost where it names a look.
   Then the product names, case by case, longest first.

The migration modules (EXCLUDED_CONTENT) are never rewritten: they hold the old names on purpose,
so that the owner's existing data is moved. This script itself also names the old products,
because it is the rule table; it is excluded for the same reason.
"""
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(subprocess.check_output(["git", "rev-parse", "--show-toplevel"], text=True).strip())
DRY_RUN = "--dry-run" in sys.argv

# Content that must not be rewritten (see the module docstring). Paths are repository-relative.
EXCLUDED_CONTENT = {
    "scripts/rebrand.py",
    "apps/desktop/src-tauri/src/migrate.rs",
    "apps/desktop/src/legacyNames.ts",
    "apps/desktop/src/legacyNames.test.ts",
}

# A line containing this marker keeps its old spelling (see transform).
KEEP_MARK = "rebrand-keep"

# Binary image and icon files keep both their names and their bytes.
IMAGE_EXTENSIONS = {".png", ".ico", ".icns", ".jpg", ".jpeg", ".webp"}

# Directories inside apps/native that are build output, never rewritten or collected.
SKIP_DIRS = {"target", "dist", "node_modules", ".git"}

# Spans that keep their old spelling.
PROTECTED = [
    # The GitHub repository has not been renamed yet, so its slug (and every URL built from it) stays.
    re.compile(r"sico-vibes/(?:bloblex|bitbots)", re.IGNORECASE),
    # The checkout folder on disk (also in Claude's project path); the owner renames it separately.
    re.compile(r"Coding[\\/\-]+Blolblex", re.IGNORECASE),
    # Binary image and icon file names referenced from configs and docs.
    re.compile(r"[\w.\-]*(?:bitbots|bloblex|blolblex|blobots)[\w.\-]*\.(?:png|ico|icns|jpe?g|webp)", re.IGNORECASE),
]

# Product names, case by case. Longest first where one contains another.
NAME_PAIRS = [
    ("BITBOTS", "RETINUE"), ("BitBots", "Retinue"), ("Bitbots", "Retinue"), ("bitbots", "retinue"),
    ("BLOBLEX", "RETINUE"), ("Bloblex", "Retinue"), ("bloblex", "retinue"),
    ("BLOLBLEX", "RETINUE"), ("Blolblex", "Retinue"), ("blolblex", "retinue"),
    ("BLOBOTS", "STONE"), ("Blobots", "Stone"), ("blobots", "stone"),
]
NAME_MAP = dict(NAME_PAIRS)
NAME_RX = re.compile("|".join(re.escape(old) for old, _ in sorted(NAME_PAIRS, key=lambda kv: -len(kv[0]))))

# Retired look: a quoted or backticked bitbots on a line about looks or styles is the Frost look id.
LOOK_LINE = re.compile(r"look|style|frost|retir|appearance|theme|stored", re.IGNORECASE)
FROST_ID = re.compile(r"(['\"`])bitbots\1")
FROST_NAME = re.compile(r"(['\"`])Bitbots\1")


def is_old(text):
    return NAME_RX.search(text) is not None or re.search(r"bitbots|bloblex|blolblex|blobots", text, re.IGNORECASE) is not None


def new_component(name, keep_name=False):
    if keep_name:
        return name
    return NAME_RX.sub(lambda m: NAME_MAP[m.group(0)], name)


def new_path(rel):
    parts = rel.split("/")
    out = []
    for index, part in enumerate(parts):
        is_file = index == len(parts) - 1
        keep = is_file and Path(part).suffix.lower() in IMAGE_EXTENSIONS
        out.append(new_component(part, keep))
    return "/".join(out)


def collect():
    tracked = subprocess.check_output(["git", "-C", str(ROOT), "ls-files", "-z"]).decode("utf-8", "surrogateescape").split("\0")
    files = {p for p in tracked if p}
    native = ROOT / "apps" / "native"
    if native.is_dir():
        for dirpath, dirnames, filenames in os.walk(native):
            dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]
            for name in filenames:
                files.add((Path(dirpath) / name).relative_to(ROOT).as_posix())
    return sorted(files)


def is_tracked(rel):
    return subprocess.run(["git", "-C", str(ROOT), "ls-files", "--error-unmatch", "--", rel],
                          capture_output=True).returncode == 0


def rename_files(files):
    renamed = {}
    for rel in files:
        target = new_path(rel)
        if target == rel:
            continue
        renamed[rel] = target
        if DRY_RUN:
            print(f"rename {rel} -> {target}")
            continue
        (ROOT / target).parent.mkdir(parents=True, exist_ok=True)
        if is_tracked(rel):
            subprocess.check_call(["git", "-C", str(ROOT), "mv", "--", rel, target])
        else:
            os.replace(ROOT / rel, ROOT / target)
    if not DRY_RUN:
        # Remove folders the renames emptied (bottom-up), so no old-named folder remains.
        for rel in renamed:
            folder = (ROOT / rel).parent
            while folder != ROOT and folder.is_dir() and not any(folder.iterdir()):
                folder.rmdir()
                folder = folder.parent
    return renamed


def transform(text):
    # A line marked `rebrand-keep` names an old product on purpose (a migration, or the value an
    # external service accepted), so it is left exactly as it is.
    return "".join(line if KEEP_MARK in line else transform_line(line) for line in text.splitlines(keepends=True))


def transform_line(text):
    holes = []

    def hide(match):
        holes.append(match.group(0))
        return f"\x00{len(holes) - 1}\x00"

    for rx in PROTECTED:
        text = rx.sub(hide, text)
    out = []
    for line in text.splitlines(keepends=True):
        if LOOK_LINE.search(line):
            line = FROST_ID.sub(lambda m: f"{m.group(1)}frost{m.group(1)}", line)
            line = FROST_NAME.sub(lambda m: f"{m.group(1)}Frost{m.group(1)}", line)
        out.append(line)
    text = "".join(out)
    text = NAME_RX.sub(lambda m: NAME_MAP[m.group(0)], text)
    # "Bitbots' own" (possessive) becomes "Retinue's own", not "Retinue' own".
    text = re.sub(r"Retinue' (?=[a-z])", "Retinue's ", text)
    text = re.sub(r"\x00(\d+)\x00", lambda m: holes[int(m.group(1))], text)
    return text


def rewrite_content(files):
    changed = []
    for rel in files:
        if rel in EXCLUDED_CONTENT:
            continue
        path = ROOT / rel
        if not path.is_file():
            continue
        raw = path.read_bytes()
        if b"\0" in raw[:8000]:
            continue
        text = raw.decode("utf-8", "surrogateescape")
        new = transform(text)
        if new == text:
            continue
        changed.append(rel)
        if not DRY_RUN:
            path.write_bytes(new.encode("utf-8", "surrogateescape"))
    return changed


def main():
    files = collect()
    renamed = rename_files(files)
    # Content is rewritten at the file's new location; exclusions are keyed by that location.
    after = [new_path(rel) if rel in renamed else rel for rel in files]
    if DRY_RUN:
        after = files
    changed = rewrite_content(after)
    print(f"{len(renamed)} path(s) renamed; {len(changed)} file(s) rewritten" + (" (dry run)" if DRY_RUN else ""))
    if DRY_RUN:
        return
    # Report what still mentions an old name outside the protected spans and the excluded modules.
    for rel in after:
        path = ROOT / rel
        if rel in EXCLUDED_CONTENT or not path.is_file():
            continue
        raw = path.read_bytes()
        if b"\0" in raw[:8000]:
            continue
        text = raw.decode("utf-8", "surrogateescape")
        for rx in PROTECTED:
            text = rx.sub("", text)
        if is_old(text):
            print(f"still mentions an old name: {rel}")


if __name__ == "__main__":
    main()
