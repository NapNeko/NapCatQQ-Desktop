"""Capture the real Desktop browser preview with its bundled demonstration IPC data.

Requires Python Playwright and Chromium. Build the main branch's UI and serve its
output before running; this script never changes the Desktop application source.
"""

import argparse
import hashlib
import json
import re
from pathlib import Path

from playwright.sync_api import sync_playwright


PANELS = [
    ("karin", "Karin · 本机", "权限"),
    ("nonebot2", "荒境修仙", "连接"),
    ("astrbot", "AstrBot · 本机", "模型"),
    ("maibot", "麦麦 · 本机", "模型"),
    ("koishi", "Koishi · 本机", "插件"),
    ("yunzai", "云崽 · 本机", "主人与权限"),
    ("neobot", "NeoBot · 本机", "模型"),
]


def capture(url, output, chromium):
    output.mkdir(parents=True, exist_ok=True)
    records = []
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(
            executable_path=chromium,
            headless=True,
            args=["--no-sandbox", "--disable-crash-reporter", "--disable-gpu", "--disable-dev-shm-usage"],
        )
        try:
            context = browser.new_context(
                viewport={"width": 1440, "height": 1000},
                device_scale_factor=1,
                locale="zh-CN",
                color_scheme="light",
                reduced_motion="reduce",
            )
            page = context.new_page()
            page.set_default_timeout(30000)
            preferences = {"theme": "auto", "sidebarStyle": "classic", "motionEnabled": False, "showMascot": True}
            page.add_init_script("localStorage.setItem('ncd:preferences:v1', " + json.dumps(json.dumps(preferences)) + ");")
            page.goto(url, wait_until="domcontentloaded")
            page.wait_for_timeout(4000)
            expand = page.get_by_role("button", name="展开侧栏", exact=True)
            if expand.count():
                expand.click()
            for framework, name, tab in PANELS:
                page.locator("aside").get_by_role("button", name="应用端", exact=True).click()
                back = page.get_by_role("button", name="返回列表", exact=True)
                if back.count():
                    back.click()
                page.get_by_text(name, exact=True).click()
                # Setup badges add text to the accessible tab name.
                exact_tab = page.get_by_role("tab", name=tab, exact=True)
                if exact_tab.count():
                    exact_tab.click()
                else:
                    page.get_by_role("tab", name=re.compile("^" + re.escape(tab))).first.click()
                page.wait_for_timeout(1300)
                if framework == "koishi":
                    page.get_by_text("help", exact=True).click()
                    page.wait_for_timeout(500)
                for close in page.locator('[role="alert"] button[aria-label="关闭"]').all():
                    if close.is_visible():
                        close.click()
                for theme in ("light", "dark"):
                    # Actual auto theme follows the system media query; same panel, no reload.
                    page.emulate_media(color_scheme=theme)
                    page.wait_for_timeout(500)
                    filename = f"framework-{framework}-{theme}.png"
                    page.screenshot(path=str(output / filename))
                    records.append({"file": filename, "framework": framework, "panel": tab, "theme": theme,
                                    "sha256": hashlib.sha256((output / filename).read_bytes()).hexdigest()})
                    print(filename, flush=True)
            context.close()
        finally:
            browser.close()
    return records


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", default="http://127.0.0.1:1422")
    parser.add_argument("--chromium", default="/usr/bin/chromium")
    parser.add_argument("--output", type=Path, default=Path(__file__).resolve().parents[1] / "public/docs/screenshots")
    parser.add_argument("--main-sha", required=True, help="Full commit SHA of the main UI being served")
    options = parser.parse_args()
    records = capture(options.url, options.output, options.chromium)
    manifest = {"source_commit": options.main_sha, "viewport": {"width": 1440, "height": 1000},
                "device_scale_factor": 1, "data_source": "main bundled browser mock IPC data",
                "runtime": "production browser preview; no application source modifications",
                "theme_capture": "actual auto preference; prefers-color-scheme light/dark on the same panel",
                "capture_script": "scripts/capture-framework-screenshots.py",
                "screenshots": records}
    (options.output / "frameworks-manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")
