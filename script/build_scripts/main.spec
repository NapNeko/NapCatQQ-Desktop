# -*- mode: python ; coding: utf-8 -*-

block_cipher = None

a = Analysis(
    ['../../main.py'],  # 主入口文件
    pathex=[],    # 可添加额外搜索路径
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=[],   # 可排除不需要的模块
    win_no_prefer_redirects=False,
    win_private_assemblies=False,
    cipher=block_cipher,
    noarchive=True,  # 关键：必须设置为 True（单文件模式）
)

# 单文件模式不需要 COLLECT
pyz = PYZ(a.pure, a.zipped_data, cipher=block_cipher)

exe = EXE(
    pyz,
    a.scripts,
    a.binaries,
    a.zipfiles,
    a.datas,
    name='NapCatQQ-Desktop',  # 输出文件名
    debug=False,
    strip=False,
    upx=True,          # 启用 UPX 压缩
    console=False,     # 不显示控制台
    icon='icon.ico',   # 图标
    uac_admin=True,    # 请求管理员权限
    runtime_tmpdir=None,  # 单文件运行时解压目录（None 为自动管理）
)
