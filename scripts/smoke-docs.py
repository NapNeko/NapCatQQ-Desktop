#!/usr/bin/env python3
"""Browser smoke checks for a running documentation site.

Requires Python Playwright and a Chromium executable. Example:
  python scripts/smoke-docs.py --url http://127.0.0.1:4321/NapCatQQ-Desktop
Reports and screenshots are written outside the repository by default.
"""

import argparse
import json
import sys
import tempfile
from pathlib import Path
from urllib.parse import unquote, urljoin, urlparse


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", default="http://127.0.0.1:4321", help="Site URL, including any SITE_BASE prefix")
    parser.add_argument("--output", type=Path, default=Path(tempfile.gettempdir()) / "ncd-docs-review")
    parser.add_argument("--chromium", default="/usr/bin/chromium")
    args = parser.parse_args()
    if urlparse(args.url).scheme not in {"http", "https"} or urlparse(args.url).query or urlparse(args.url).fragment:
        parser.error("--url must be an HTTP(S) site root without a query or fragment")
    try:
        from playwright.sync_api import expect, sync_playwright
    except ImportError:
        parser.error("Python Playwright is required: python -m pip install playwright")

    base = args.url.rstrip("/") + "/"
    output = args.output.expanduser().resolve()
    output.mkdir(parents=True, exist_ok=True)
    report = {"url": base, "checks": [], "page_errors": [], "console_errors": [], "resource_errors": []}
    routes, images = [], set()

    def site(path=""):
        return urljoin(base, path)

    def require(condition, message):
        if not condition:
            raise AssertionError(message)

    def record(name, operation):
        try:
            detail = operation()
            report["checks"].append({"name": name, "passed": True, "detail": detail})
            print(f"PASS {name}", flush=True)
        except Exception as error:
            report["checks"].append({"name": name, "passed": False, "error": str(error)})
            print(f"FAIL {name}: {error}", file=sys.stderr, flush=True)

    def listen(page):
        page.on("pageerror", lambda error: report["page_errors"].append(str(error)))
        page.on("console", lambda message: report["console_errors"].append(message.text) if message.type == "error" else None)
        page.on("response", lambda response: report["resource_errors"].append(f"HTTP {response.status}: {response.url}") if response.status >= 400 else None)
        # Navigating away from a page legitimately aborts pending lazy images.
        page.on("requestfailed", lambda request: report["resource_errors"].append(f"{request.failure}: {request.url}") if "ERR_ABORTED" not in (request.failure or "") else None)

    def goto(page, path):
        response = page.goto(site(path), wait_until="domcontentloaded")
        require(response is not None and response.status == 200, f"Page did not return 200: {page.url}")
        expect(page.locator(".doc-title h1")).to_be_visible()
        images.update(page.locator(".doc-prose img").evaluate_all("xs => xs.map(x => x.src)"))

    def no_overflow(page):
        require(page.evaluate("document.documentElement.scrollWidth <= innerWidth"), f"Horizontal page overflow at {page.url}")

    def set_theme(page, theme):
        if page.locator("html").get_attribute("data-theme") != theme:
            page.locator("[data-theme-toggle]").click()
        expect(page.locator("html")).to_have_attribute("data-theme", theme)

    def find(page, query):
        page.keyboard.press("Control+k")
        expect(page.locator("[data-search-dialog]")).to_be_visible()
        page.locator("#docs-search-input").fill(query)
        expect(page.locator(".search-result").first).to_be_visible()

    def decoded_image(page, selector):
        image = page.locator(selector).first
        image.evaluate("img => { img.loading = 'eager'; }")
        page.wait_for_function("img => img.complete && img.naturalWidth > 0", arg=image.element_handle())
        return image

    with sync_playwright() as playwright:
        with playwright.chromium.launch(executable_path=args.chromium, headless=True, args=["--no-sandbox"]) as browser:
            desktop = browser.new_context(viewport={"width": 1440, "height": 1000}, color_scheme="light", reduced_motion="reduce", permissions=["clipboard-read", "clipboard-write"])
            desktop.set_default_timeout(10000)
            page = desktop.new_page()
            listen(page)

            def inventory():
                goto(page, "docs/")
                routes.extend(page.locator(".sidebar-group a").evaluate_all("xs => xs.map(x => x.href)"))
                require(routes and len(routes) == len(set(routes)), "Sidebar routes must be nonempty and unique")
                response = desktop.request.get(site("docs/search.json"))
                require(response.ok, f"Search index returned {response.status}")
                indexed = {urljoin(base, entry["url"]) for entry in response.json()}
                require(set(routes) == indexed, "Sidebar and search index contain different page sets")
                content = Path(__file__).resolve().parent.parent / "src/content/docs"
                if content.is_dir():
                    require(len(routes) == len(list(content.glob("*.md"))), "Navigation count differs from current Markdown source count")
                no_overflow(page)
                return {"pages": len(routes)}

            record("page inventory and search index", inventory)

            def themes():
                goto(page, "docs/")
                set_theme(page, "light")
                page.screenshot(path=str(output / "desktop-light.png"))
                set_theme(page, "dark")
                page.screenshot(path=str(output / "desktop-dark.png"))
                page.reload(wait_until="domcontentloaded")
                expect(page.locator("html")).to_have_attribute("data-theme", "dark")
                goto(page, "docs/installation/")
                expect(page.locator("html")).to_have_attribute("data-theme", "dark")

            record("desktop theme switch and persistence", themes)

            def search():
                goto(page, "docs/")
                find(page, "备份")
                expect(page.locator(".search-result.is-selected strong")).to_have_text("备份、迁移与升级")
                page.screenshot(path=str(output / "search-backup.png"))
                page.keyboard.press("Enter")
                page.wait_for_url(site("docs/backup/"))
                expect(page.locator(".doc-title h1")).to_have_text("备份、迁移与升级")
                find(page, "反向 WebSocket")
                expect(page.locator(".search-result.is-selected strong")).to_have_text("连接应用端与 OneBot")
                first = page.locator(".search-result.is-selected").get_attribute("href")
                page.keyboard.press("ArrowDown")
                expect(page.locator(".search-result.is-selected")).not_to_have_attribute("href", first)
                page.keyboard.press("ArrowUp")
                expect(page.locator(".search-result.is-selected")).to_have_attribute("href", first)
                page.keyboard.press("Escape")
                expect(page.locator("[data-search-dialog]")).not_to_be_visible()

            record("search keyboard, navigation and Escape with a query", search)

            def outline():
                goto(page, "docs/connections/")
                link = page.locator(".docs-outline nav a").filter(has_text="手动配置正向 WebSocket").first
                fragment = unquote(link.get_attribute("href"))
                link.click()
                page.wait_for_function("hash => decodeURIComponent(location.hash) === hash", arg=fragment)
                expect(link).to_have_class("is-active")
                heading = page.locator(f'[id="{fragment[1:]}"]')
                page.wait_for_function("el => el.getBoundingClientRect().top >= 72 && el.getBoundingClientRect().top < innerHeight", arg=heading.element_handle())

            record("desktop outline anchors and active heading", outline)

            def screenshot_controls():
                goto(page, "docs/connections/")
                set_theme(page, "dark")
                shot = page.locator("[data-shot]").first
                expect(shot).to_have_attribute("data-mode", "auto")
                expect(shot.locator(".shot-dark")).to_be_visible()
                decoded_image(page, "[data-shot] .shot-dark")
                for theme in ["light", "dark"]:
                    shot.locator(f"[data-shot-theme={theme}]").click()
                    expect(shot.locator(f"[data-shot-theme={theme}]")).to_have_attribute("aria-pressed", "true")
                    image = decoded_image(page, f"[data-shot] .shot-{theme}")
                    expect(image).to_be_visible()
                    shot.locator("[data-shot-open]").click()
                    expect(page.locator("[data-lightbox]")).to_be_visible()
                    expect(page.locator("[data-lightbox] img")).to_have_attribute("src", image.evaluate("img => img.src"))
                    decoded_image(page, "[data-lightbox] img")
                    page.keyboard.press("Escape")
                    expect(page.locator("[data-lightbox]")).not_to_be_visible()

            record("real screenshot pairs, theme controls and lightbox", screenshot_controls)

            def clipboard():
                goto(page, "docs/connections/")
                block = page.locator(".doc-prose pre").first
                expected = block.locator("code").inner_text()
                block.locator(".copy-code").click()
                expect(block.locator(".copy-code")).to_have_text("已复制")
                actual = page.evaluate("navigator.clipboard.readText()")
                require(actual == expected, "Copied code differs from the displayed code block")

            record("copy code to clipboard", clipboard)
            for url in routes:
                record("desktop route " + urlparse(url).path, lambda url=url: (goto(page, url), no_overflow(page)))

            mobile = browser.new_context(viewport={"width": 390, "height": 844}, is_mobile=True, has_touch=True, color_scheme="light", reduced_motion="reduce")
            mobile.set_default_timeout(10000)
            phone = mobile.new_page()
            listen(phone)

            def mobile_navigation():
                goto(phone, "docs/first-bot/")
                no_overflow(phone)
                expect(phone.locator("#docs-sidebar")).not_to_be_visible()
                phone.screenshot(path=str(output / "mobile-light.png"))
                toggle = phone.locator("[data-sidebar-toggle]")
                toggle.focus()
                phone.keyboard.press("Space")
                expect(toggle).to_have_attribute("aria-expanded", "true")
                expect(phone.locator("#docs-sidebar")).to_be_visible()
                phone.screenshot(path=str(output / "mobile-sidebar.png"))
                phone.keyboard.press("Escape")
                expect(toggle).to_have_attribute("aria-expanded", "false")
                toggle.click()
                phone.locator(".sidebar-group a").filter(has_text="备份、迁移与升级").click()
                phone.wait_for_url(site("docs/backup/"))
                expect(phone.locator("[data-sidebar-toggle]")).to_have_attribute("aria-expanded", "false")
                phone.locator(".mobile-outline summary").click()
                link = phone.locator(".mobile-outline a").filter(has_text="清理下载缓存").first
                fragment = unquote(link.get_attribute("href"))
                link.click()
                phone.wait_for_function("hash => decodeURIComponent(location.hash) === hash", arg=fragment)
                heading = phone.locator(f'[id="{fragment[1:]}"]')
                phone.wait_for_function("el => el.getBoundingClientRect().top >= 108 && el.getBoundingClientRect().top < innerHeight", arg=heading.element_handle())
                set_theme(phone, "dark")
                goto(phone, "docs/first-bot/")
                expect(phone.locator("html")).to_have_attribute("data-theme", "dark")
                phone.screenshot(path=str(output / "mobile-dark.png"))
                phone.locator("[data-search-open]").click()
                phone.locator("#docs-search-input").fill("备份")
                expect(phone.locator(".search-result").first).to_be_visible()
                phone.screenshot(path=str(output / "mobile-search.png"))
                phone.keyboard.press("Escape")
                expect(phone.locator("[data-search-dialog]")).not_to_be_visible()

            record("mobile keyboard, sidebar, outline, theme and search", mobile_navigation)
            for url in routes:
                record("mobile route " + urlparse(url).path, lambda url=url: (goto(phone, url), no_overflow(phone)))

            # Check hidden and lazy theme variants too, not only images loaded above the fold.
            for url in sorted(images):
                def resource(url=url):
                    response = desktop.request.get(url)
                    require(response.ok, f"Image returned HTTP {response.status}: {url}")
                    require(response.headers.get("content-type", "").startswith("image/"), f"Not an image response: {url}")
                record("image " + urlparse(url).path, resource)

    for key in ["page_errors", "console_errors", "resource_errors"]:
        report[key] = sorted(set(report[key]))
    failed = sum(not check["passed"] for check in report["checks"]) + sum(len(report[key]) for key in ["page_errors", "console_errors", "resource_errors"])
    report["summary"] = {"checks": len(report["checks"]), "failed_checks": sum(not check["passed"] for check in report["checks"]), "errors": failed, "pages": len(routes), "images": len(images)}
    (output / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    print(f"{len(report['checks'])} checks; {failed} failures/errors. Report: {output / 'report.json'}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
