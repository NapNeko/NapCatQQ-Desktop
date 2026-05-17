# -*- coding: utf-8 -*-
"""根据环境变量生成 ``src/core/network/_build_constants.py``.

打包前调用. 从环境变量读取真实 secret, 写入 _build_constants.py 替换占位值.

环境变量:
    NCD_PROXY_BASE_URL      中转站根地址 (默认: https://napcat-desktop.aqiaoyo.top)
    NCD_PROXY_SHARED_SECRET 与 Worker SHARED_SECRET 一致的 HMAC 密钥 (必填)

CI 用法:
    set NCD_PROXY_SHARED_SECRET=xxxxxxxx
    python script/build_scripts/inject_proxy_constants.py
    pyinstaller script/build_scripts/main.spec

退出码:
    0: 成功生成
    1: 缺少必填环境变量
"""

from __future__ import annotations

import os
import sys
from pathlib import Path

DEFAULT_BASE_URL = "https://napcat-desktop.aqiaoyo.top"
TARGET_REL = "src/core/network/_build_constants.py"


def main() -> int:
    # GitHub Actions 把未配置的 vars.X 渲染成空字符串而非 unset, 所以 .get(key, default)
    # 拿不到 default. 这里手动检查空串.
    base_url = (os.environ.get("NCD_PROXY_BASE_URL") or DEFAULT_BASE_URL).rstrip("/")
    secret = os.environ.get("NCD_PROXY_SHARED_SECRET")

    if not secret:
        print(
            "[ERROR] 环境变量 NCD_PROXY_SHARED_SECRET 未设置. "
            "请先 export 真实 secret 再打包.",
            file=sys.stderr,
        )
        return 1

    if "PLACEHOLDER" in secret or len(secret) < 16:
        print(
            f"[ERROR] NCD_PROXY_SHARED_SECRET 看起来不是真实密钥 (len={len(secret)}). "
            "请用 ``openssl rand -hex 32`` 生成并配置.",
            file=sys.stderr,
        )
        return 1

    project_root = Path(__file__).resolve().parents[2]
    target = project_root / TARGET_REL

    content = (
        "# -*- coding: utf-8 -*-\n"
        '"""构建期注入的常量 (本文件由 inject_proxy_constants.py 生成, 不要手改)."""\n\n'
        f'PROXY_BASE_URL = {base_url!r}\n'
        f'PROXY_SHARED_SECRET = {secret!r}\n'
    )

    target.write_text(content, encoding="utf-8")
    print(f"[OK] 写入 {target} (base_url={base_url}, secret 长度={len(secret)})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
