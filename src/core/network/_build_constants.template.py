# -*- coding: utf-8 -*-
"""构建期注入的常量 (本地 fallback).

⚠️ 此文件不入 git, 仅供本地开发使用. 正式发布的二进制由构建脚本根据环境
变量重新生成本文件再打包.

正式发布时构建脚本会读取以下环境变量:
    - NCD_PROXY_BASE_URL: 中转站根地址, 例: ``https://napcat-desktop.aqiaoyo.top``
    - NCD_PROXY_SHARED_SECRET: HMAC 密钥, 与 Worker 的 ``SHARED_SECRET`` 一致

本地开发时:
    1. 复制 ``_build_constants.template.py`` 到 ``_build_constants.py``
    2. 填入开发用的中转地址和密钥
    3. 运行项目即可

也可以直接 export 环境变量, 但 Python 启动时不会读环境变量, 必须有这份文件.
"""

# 中转站根地址, 不带尾斜杠
PROXY_BASE_URL = "https://napcat-desktop.aqiaoyo.top"

# HMAC-SHA256 密钥. 必须与 Worker 端 SHARED_SECRET 一致.
# 生成: openssl rand -hex 32
PROXY_SHARED_SECRET = "DEV_SECRET_PLACEHOLDER_REPLACE_BEFORE_RUNNING"
