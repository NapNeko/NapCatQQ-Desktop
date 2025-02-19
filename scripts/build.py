# -*- coding: utf-8 -*-


if __name__ == "__main__":
    # 标准库导入
    import subprocess
    from pathlib import Path

    # 获取需要编译的文件的路径
    path = Path().cwd() / "main.py"

    # 定义要传递给 Nuitka 的命令行选项
    nuitka_command = [
        "nuitka",
        "--standalone",
        "--no-pyi-file",
        "--output-dir=dist",
        "--output-filename=NapCatQQ Desktop",
        f"--windows-icon-from-ico={path.parent / "src" / "ui" / "resource" / "icons" / "logo.ico"}",
        "--enable-plugin=pyside6",
        "--jobs=12",
        "--show-progress",
        "--show-memory",
        str(path),
    ]

    # 使用 subprocess 运行 Nuitka
    subprocess.run(nuitka_command)
