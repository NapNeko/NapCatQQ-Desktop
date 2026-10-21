"""Capture core routes using the real main UI and its bundled browser IPC fixtures.

Requires Python Playwright and Chromium. Native Windows dialogs, the hidden
Docker route without a remote host, and real SnowLuma QR login are excluded.
"""
import argparse
import hashlib
import json
import re
from pathlib import Path
from playwright.sync_api import sync_playwright

SCREENS = [('overview', '概览', None), ('bots', '机器人', None), ('remote', '远端', None),
           ('debug', '调试台', None), ('tasks', '任务队列', None),
           ('settings-appearance', '设置', '外观'), ('settings-features', '设置', '功能'),
           ('settings-network', '设置', '网络'), ('settings-data', '设置', '数据'),
           ('settings-notifications', '设置', '通知'), ('settings-runtime', '设置', '运行'),
           ('settings-monitoring', '设置', '监控')]


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
            for name, route, tab in SCREENS:
                page = context.new_page()
                page.set_default_timeout(15000)
                page.goto(args.url, wait_until='domcontentloaded')
                page.locator('aside').get_by_role('button', name=route, exact=True).click()
                if tab:
                    page.get_by_role('tab', name=tab, exact=True).click()
                if name == 'debug':
                    page.get_by_role('button', name='搜索接口（Ctrl+K）', exact=True).click()
                    page.get_by_role('combobox', name='搜索接口', exact=True).fill('get_login_info')
                    page.get_by_role('option', name=re.compile('^get_login_info')).click()
                    page.get_by_role('heading', name='get_login_info', exact=True).wait_for()
                    page.keyboard.press('Control+Enter')
                page.wait_for_timeout(1200)
                for close in page.locator('[role="alert"] button[aria-label="关闭"]').all():
                    if close.is_visible():
                        close.click()
                for theme in ['light', 'dark']:
                    page.emulate_media(color_scheme=theme)
                    page.wait_for_timeout(500)
                    filename = f'{name}-{theme}.png'
                    page.screenshot(path=str(args.output / filename))
                    records.append({'file': filename, 'screen': f'{route}{" / " + tab if tab else ""}', 'theme': theme,
                                    'sha256': hashlib.sha256((args.output / filename).read_bytes()).hexdigest()})
                    print(filename, flush=True)
                page.close()
        finally:
            browser.close()
    manifest = {'source_commit': args.main_sha, 'viewport': {'width': 1440, 'height': 1000},
                'data_source': 'main bundled browser mock IPC; no real accounts or private hosts',
                'theme_capture': 'actual auto preference with prefers-color-scheme light/dark on the same screen',
                'capture_script': 'scripts/capture-core-screenshots.py', 'screenshots': records}
    (args.output / 'core-manifest.json').write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + '\n')


if __name__ == '__main__':
    main()
