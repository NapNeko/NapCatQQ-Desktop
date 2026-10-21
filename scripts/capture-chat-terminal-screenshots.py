"""Capture chat, group files and terminal from the main branch's real browser preview.

The preview must serve the UI with its bundled mock IPC. No real accounts or
remote commands are used. Requires Python Playwright and a Chromium executable.
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
        try:
            page = browser.new_page(viewport={'width': 1440, 'height': 1000}, reduced_motion='reduce', locale='zh-CN', color_scheme='light')
            page.set_default_timeout(15000)
            preferences = json.dumps({'theme': 'auto', 'sidebarStyle': 'classic', 'motionEnabled': False})
            page.add_init_script('localStorage.setItem("ncd:preferences:v1",' + json.dumps(preferences) + ');')

            def dismiss_notices():
                for close in page.locator('[role="alert"] button[aria-label="关闭"]').all():
                    if close.is_visible():
                        close.click()

            def pair(name, description):
                dismiss_notices()
                for theme in ['light', 'dark']:
                    page.emulate_media(color_scheme=theme)
                    page.wait_for_timeout(500)  # Let the native theme observers repaint canvas controls.
                    filename = f'{name}-{theme}.png'
                    page.screenshot(path=str(args.output / filename))
                    records.append({'file': filename, 'screen': description, 'theme': theme,
                                    'sha256': hashlib.sha256((args.output / filename).read_bytes()).hexdigest()})
                    print(filename, flush=True)

            page.goto(args.url, wait_until='domcontentloaded')
            page.locator('aside').get_by_role('button', name='机器人', exact=True).click()
            page.get_by_role('button', name='终端', exact=True).first.click()
            page.locator('.xterm-screen').first.wait_for()
            page.wait_for_timeout(1000)
            pair('terminal', '机器人页打开本机终端；命令输出为项目自带的浏览器演示')

            # A fresh page keeps the terminal collapsed while showing the chat workflow.
            page.goto(args.url, wait_until='domcontentloaded')
            page.locator('aside').get_by_role('button', name='聊天', exact=True).click()
            page.get_by_role('button', name=re.compile('群消息盒子')).first.click()
            page.get_by_text('NapCat 开发交流', exact=True).click()
            page.get_by_role('button', name='群文件', exact=True).wait_for()
            page.wait_for_timeout(1000)
            pair('chat', '选择演示群会话，查看消息、媒体和输入区')
            page.get_by_role('button', name='群文件', exact=True).click()
            page.get_by_role('textbox', name='搜索群文件').wait_for()
            page.wait_for_timeout(800)
            pair('chat-group-files', '群文件面板：目录、上传下载、搜索与容量')
        finally:
            browser.close()
    manifest = {'source_commit': args.main_sha, 'viewport': {'width': 1440, 'height': 1000},
                'data_source': 'main bundled browser mock IPC; no real QQ or terminal acceptance test',
                'theme_capture': 'actual auto preference with prefers-color-scheme light/dark on the same screen',
                'capture_script': 'scripts/capture-chat-terminal-screenshots.py', 'screenshots': records}
    (args.output / 'chat-terminal-manifest.json').write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + '\n')


if __name__ == '__main__':
    main()
