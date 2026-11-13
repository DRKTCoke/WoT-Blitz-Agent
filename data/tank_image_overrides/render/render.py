"""Render reviewed Tier XI covers from a local, unpacked dataset using the XM69 framing."""
import argparse
import base64
import io
import json
from pathlib import Path
from urllib.parse import urlsplit

from PIL import Image, ImageFilter
from playwright.sync_api import sync_playwright


def fit_cover(data):
    with Image.open(io.BytesIO(data)) as source:
        alpha = source.getchannel("A")
        bounds = alpha.getbbox()
        if not bounds or min(bounds[:2]) < 2 or bounds[2] >= source.width - 1 or bounds[3] >= source.height - 1:
            raise ValueError("Empty or clipped render")
        subject = source.crop(bounds)
    # Frame the main body like XM69; keep antennas and thin details in the reserved margin.
    body = subject.getchannel("A").point(lambda a: 255 if a > 160 else 0)
    left, top, right, bottom = body.filter(ImageFilter.MedianFilter(13)).getbbox()
    scale = min(146 / (right - left), 101 / (bottom - top), 124 / subject.height)
    size = round(subject.width * scale), round(subject.height * scale)
    if size[0] + 4 > 256:
        raise ValueError("Silhouette exceeds the icon canvas")
    canvas = Image.new("RGBA", (256, 128))
    canvas.paste(subject.resize(size, Image.Resampling.LANCZOS), (2, 2))
    return canvas


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pack", type=Path, required=True)
    parser.add_argument("--three", type=Path, required=True, help="Installed three@0.185.1 package")
    parser.add_argument("--browser", type=Path, help="Path to the Chromium or Chrome executable")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("ids", nargs="+", type=int)
    args = parser.parse_args()
    package = json.loads((args.three / "package.json").read_bytes())
    if package["version"] != "0.185.1":
        parser.error("This render recipe uses three@0.185.1")
    args.out.mkdir(parents=True, exist_ok=True)
    page_html = Path(__file__).with_name("render.html")

    def serve(route):
        # Fulfil same-origin requests from disk, without opening a socket or using a CDN.
        url = urlsplit(route.request.url)
        if url.netloc != "127.0.0.1":
            return route.abort()
        if url.path == "/render.html":
            return route.fulfill(path=page_html, content_type="text/html")
        parts = url.path.lstrip("/").split("/")
        roots = {"pack": args.pack, "three": args.three}
        if parts[0] not in roots:
            return route.abort()
        root = roots[parts[0]].resolve()
        file = root.joinpath(*parts[1:]).resolve()
        if not file.is_relative_to(root) or not file.is_file():
            return route.fulfill(status=404)
        types = {".js": "text/javascript", ".json": "application/json", ".glb": "model/gltf-binary"}
        return route.fulfill(path=file, content_type=types.get(file.suffix, "application/octet-stream"))

    with sync_playwright() as p:
        browser = p.chromium.launch(executable_path=args.browser, args=["--use-angle=swiftshader", "--enable-unsafe-swiftshader"])
        for tank_id in args.ids:
            page = browser.new_page()
            page.route("http://**/*", serve)
            page.goto("http://127.0.0.1/render.html")
            page.wait_for_function("typeof window.renderTank === 'function'")
            png = page.evaluate("id => window.renderTank(id)", tank_id)
            canvas = fit_cover(base64.b64decode(png))
            canvas.save(args.out / f"{tank_id}.webp", "WEBP", lossless=True, method=6)
            print(f"{tank_id}: {canvas.getbbox()}", flush=True)
            page.close()
        browser.close()


if __name__ == "__main__":
    main()
