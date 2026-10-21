"""Capture real application UI steps using main's bundled browser IPC fixtures.

Requires Python Playwright and Chromium. Installation and link forms are not submitted.
"""
import argparse
import hashlib
import json
import re
from pathlib import Path
from playwright.sync_api import sync_playwright


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--url', default='http://127.0.0.1:1422')
    parser.add_argument('--chromium', default='/usr/bin/chromium')
    parser.add_argument('--main-sha', required=True)
    parser.add_argument('--output', type=Path, default=Path(__file__).resolve().parents[1] / 'public/docs/screenshots')
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    records = []
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(executable_path=args.chromium, headless=True,
                                            args=['--no-sandbox', '--disable-gpu', '--disable-dev-shm-usage'])
        context = browser.new_context(viewport={'width': 1440, 'height': 1000}, reduced_motion='reduce', locale='zh-CN', color_scheme='light')
        preferences = json.dumps({'theme': 'auto', 'sidebarStyle': 'classic', 'motionEnabled': False})
        context.add_init_script('localStorage.setItem("ncd:preferences:v1",' + json.dumps(preferences) + ');')
        try:
            for state in ['app-install', 'apps', 'app-link', 'app-config']:
                page = context.new_page()
                page.set_default_timeout(15000)
                page.goto(args.url, wait_until='domcontentloaded')
                page.locator('aside').get_by_role('button', name='组件' if state == 'app-install' else '应用端', exact=True).click()
                if state == 'app-install':
                    page.locator('[data-tour-id="comp-row-astrbot"]').get_by_role('button', name='新建实例', exact=True).click()
                    page.get_by_role('dialog').wait_for()
                    page.get_by_label('实例名', exact=True).fill('测试用 AstrBot')
                    page.get_by_text('监听端口', exact=True).click()
                elif state == 'app-link':
                    page.get_by_role('button', name='改绑到别的 Bot', exact=True).first.click()
                    page.get_by_role('dialog').get_by_role('combobox').first.click()
                    page.get_by_role('option', name=re.compile('10001')).first.click()
                elif state == 'app-config':
                    page.get_by_text('Karin · 本机', exact=True).click()
                    page.get_by_role('tab', name=re.compile('^基础')).first.click()
                page.wait_for_timeout(1200)
                for close in page.locator('[role="alert"] button[aria-label="关闭"]').all():
                    if close.is_visible():
                        close.click()
                for theme in ['light', 'dark']:
                    page.emulate_media(color_scheme=theme)
                    page.wait_for_timeout(500)
                    filename = f'{state}-{theme}.png'
                    page.screenshot(path=str(args.output / filename))
                    records.append({'file': filename, 'screen': state, 'theme': theme,
                                    'sha256': hashlib.sha256((args.output / filename).read_bytes()).hexdigest()})
                    print(filename, flush=True)
                page.close()
        finally:
            browser.close()
    manifest = {'source_commit': args.main_sha, 'viewport': {'width': 1440, 'height': 1000},
                'data_source': 'main bundled browser mock IPC; install and link are not submitted',
                'theme_capture': 'actual auto preference with prefers-color-scheme light/dark on the same screen',
                'capture_script': 'scripts/capture-app-screenshots.py', 'screenshots': records}
    (args.output / 'apps-manifest.json').write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + '\n')


if __name__ == '__main__':
    main()
