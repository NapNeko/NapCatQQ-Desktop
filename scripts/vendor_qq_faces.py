"""Vendor only fixed-revision QQ face thumbnails; default invocation is a dry run."""
from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor
from hashlib import sha1, sha256
from io import BytesIO
import json
from pathlib import Path
import re
from urllib.request import Request, urlopen

REVISION = "c96fa14ff894147ac0442dda0fce6a4523e5fcbd"
ROOT = Path(__file__).resolve().parents[1]
DESTINATION = ROOT / "src-ui" / "public" / "qq-faces"
INDEX = ROOT / "src-ui" / "core" / "domain" / "chat" / "qqFaceAssets.json"
MAX_SOURCE_BYTES = 8 * 1024 * 1024
MAX_THUMB_BYTES = 2 * 1024 * 1024
MAX_EDGE = 64


def fetch(url: str, limit: int) -> bytes:
    request = Request(url, headers={"User-Agent": "NapCatQQ-Desktop-QQFace-vendor"})
    with urlopen(request, timeout=30) as response:
        body = response.read(limit + 1)
    if len(body) > limit:
        raise ValueError("source exceeds its byte budget")
    return body


def source_files() -> list[dict]:
    tree = json.loads(fetch(
        "https://api.github.com/repos/koishijs/QFace/git/trees/" + REVISION + "?recursive=1",
        16 * 1024 * 1024,
    ))
    if tree.get("truncated") or tree.get("sha") != REVISION:
        raise ValueError("incomplete or unexpected upstream revision")
    entries = []
    for item in tree["tree"]:
        match = re.fullmatch(r"public/assets/qq_emoji/(\d+)/png/\1\.png", item["path"])
        if match and item["type"] == "blob":
            entries.append({**item, "id": match[1]})
    entries.sort(key=lambda item: int(item["id"]))
    if len(entries) != 327 or sum(item["size"] for item in entries) > MAX_SOURCE_BYTES:
        raise ValueError("upstream thumbnail scope changed")
    return entries


def thumbnail(item: dict) -> tuple[str, bytes, dict]:
    from PIL import Image

    body = None
    for prefix in [
        "https://cdn.jsdelivr.net/gh/koishijs/QFace@" + REVISION + "/",
        "https://raw.githubusercontent.com/koishijs/QFace/" + REVISION + "/",
    ]:
        try:
            candidate = fetch(prefix + item["path"], 1024 * 1024)
            if len(candidate) != item["size"]:
                raise ValueError("source size differs from pinned tree")
            digest = sha1(b"blob " + str(len(candidate)).encode() + b"\0" + candidate).hexdigest()
            if digest != item["sha"]:
                raise ValueError("source hash differs from pinned tree")
            body = candidate
            break
        except Exception:
            continue
    if body is None:
        raise ValueError("cannot fetch verified resource " + item["id"])
    with Image.open(BytesIO(body)) as original:
        original.seek(0)
        image = original.convert("RGBA")
        image.thumbnail((MAX_EDGE, MAX_EDGE), Image.Resampling.LANCZOS)
        image = image.quantize(colors=128, method=Image.Quantize.FASTOCTREE)
        output = BytesIO()
        image.save(output, format="PNG", optimize=True)
    encoded = output.getvalue()
    return item["id"], encoded, {
        "sourceBlob": item["sha"],
        "sha256": sha256(encoded).hexdigest(),
        "bytes": len(encoded),
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--dry-run", action="store_true")
    mode.add_argument("--write", action="store_true")
    args = parser.parse_args()
    destination = DESTINATION.resolve()
    index = INDEX.resolve()
    if not destination.is_relative_to(ROOT.resolve()) or not index.is_relative_to(ROOT.resolve()):
        raise ValueError("asset destination escaped this checkout")
    files = source_files()
    print("revision=" + REVISION)
    print("scope=" + str(destination / "<numeric-id>.png"))
    print("module_index=" + str(index))
    print("files=" + str(len(files)) + ", source_bytes=" + str(sum(item["size"] for item in files)))
    print("transform=first frame, transparent 128-color PNG, preserve aspect, max edge 64px; output budget 2MiB")
    if not args.write:
        for item in files[:5] + files[-5:]:
            print(item["id"] + ".png <- " + item["path"])
        print("remaining " + str(len(files) - 10) + " files follow the verified numeric-id path pattern")
        print("DRY RUN: no files written; --write additionally writes manifest.json and module index")
        return
    results = []
    with ThreadPoolExecutor(max_workers=8) as workers:
        for result in workers.map(thumbnail, files):
            results.append(result)
            if len(results) % 64 == 0:
                print("verified " + str(len(results)) + "/" + str(len(files)), flush=True)
    total = sum(len(body) for _, body, _ in results)
    print("derived_thumbnail_bytes=" + str(total), flush=True)
    if total > MAX_THUMB_BYTES:
        raise ValueError("derived thumbnails exceed their total byte budget")
    destination.mkdir(parents=True, exist_ok=True)
    manifest = {"source": "https://github.com/koishijs/QFace", "revision": REVISION, "maxEdge": MAX_EDGE, "files": {}}
    for face_id, body, metadata in results:
        path = (destination / (face_id + ".png")).resolve()
        if path.parent != destination:
            raise ValueError("asset filename escaped the output directory")
        path.write_bytes(body)
        manifest["files"][face_id] = metadata
    (destination / "manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    index.write_text(json.dumps({"revision": REVISION, "ids": [face_id for face_id, _, _ in results]}, indent=4) + "\n", encoding="utf-8")
    print("WROTE " + str(len(results)) + " PNG thumbnails + manifest + module index, " + str(total) + " bytes", flush=True)


if __name__ == "__main__":
    main()
