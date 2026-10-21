"""Capture Bot tutorial steps from the real main-branch browser preview.

Only bundled demonstration data is read; the new Bot form is never saved.
Requires Python Playwright and Chromium.
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
            for state in ['components', 'bot-create', 'bot-connections', 'bot-logs']:
                page = context.new_page()
                page.set_default_timeout(15000)
                page.goto(args.url, wait_until='domcontentloaded')
                page.locator('aside').get_by_role('button', name='组件' if state == 'components' else '机器人', exact=True).click()
                if state == 'bot-create':
                    page.get_by_role('button', name='新增 Bot', exact=True).click()
                    page.get_by_placeholder('例如：10001', exact=True).fill('10003')
                    page.get_by_label('实例名称', exact=True).fill('我的第一个 Bot')
                elif state == 'bot-connections':
                    page.get_by_role('button', name='配置参数', exact=True).first.click()
                    page.get_by_role('tab', name=re.compile('^连接')).click()
                    page.get_by_text('HTTP-API', exact=True).click()
                    page.get_by_role('dialog').wait_for()
                elif state == 'bot-logs':
                    page.get_by_role('button', name='查看日志', exact=True).first.click()
                    page.get_by_role('heading', name=re.compile('实例.*运行日志')).wait_for()
                page.wait_for_timeout(1000)
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
                'data_source': 'main bundled browser mock IPC; unsaved example account 10003',
                'theme_capture': 'actual auto preference with prefers-color-scheme light/dark on the same screen',
                'capture_script': 'scripts/capture-bot-screenshots.py', 'screenshots': records}
    (args.output / 'bot-manifest.json').write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + '\n')


if __name__ == '__main__':
    main()
