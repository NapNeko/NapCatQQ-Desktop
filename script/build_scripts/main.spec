# -*- mode: python ; coding: utf-8 -*-

import sys
from pathlib import Path

spec_dir = Path.cwd() / "script" / "build_scripts"
if str(spec_dir) not in sys.path:
    sys.path.insert(0, str(spec_dir))

from collection_filters import filter_analysis_collections
from runtime_assets import prepare_runtime_assets

block_cipher = None
project_root = Path.cwd()
runtime_hooks = [str(spec_dir / "runtime_hook_qfluent_image_utils.py")]
datas = prepare_runtime_assets(project_root, project_root / "build")

a = Analysis(
    ['../../main.py'],
    pathex=[],
    hookspath=[],
    hooksconfig={},
    runtime_hooks=runtime_hooks,
    excludes=['numpy', 'scipy', 'pytest', 'matplotlib', 'pandas'],
    datas=datas,
    win_no_prefer_redirects=False,
    win_private_assemblies=False,
    cipher=block_cipher,
)
filter_analysis_collections(a, locales=("zh_CN",))

pyz = PYZ(a.pure, a.zipped_data, cipher=block_cipher)

exe = EXE(
    pyz,
    a.scripts,
    [],
    exclude_binaries=True,
    name='NapCatQQ-Desktop',
    debug=False,
    strip=False,
    upx=True,
    console=False,
    icon='icon.ico',
    uac_admin=True,
)

coll = COLLECT(
    exe,
    a.binaries,
    a.zipfiles,
    a.datas,
    strip=False,
    upx=True,
    upx_exclude=[],
    name='NapCatQQ-Desktop',
)
