# -*- coding: utf-8 -*-
"""
此脚本用于自动化发布流程

使用方法:
1. 运行脚本：`python release.py`
2. 输入要发布的版本号 (格式: x.y.z, 例如: 1.0.0)
3. 脚本将执行 pdm bump to x.y.z  命令来更新版本号(pyproject.toml)
4. 修改 src\core\config\__init__.py 中的 __version__ 变量
5. 修改 docs\CHANGELOG.md 中的版本号
6. 提交更改并推送到远程仓库
    - 主题为 "🌟 releases: 发布 vx.y.z 版本"
"""

# 标准库导入
import re
import subprocess
import sys
from pathlib import Path
from typing import Optional


def run_command(command: str) -> None:
    """运行系统命令并处理错误"""
    result = subprocess.run(command, shell=True)
    if result.returncode != 0:
        print(f"命令 '{command}' 执行失败，退出码: {result.returncode}")
        sys.exit(result.returncode)
    else:
        print(f"命令 '{command}' 执行成功")


def update_version_in_file(file_path: Path, new_version: str) -> None:
    """更新指定文件中的版本号

    文件中的版本号格式应为: __version__ = "vx.y.z" 例如: __version__ = "v1.0.0"

    """
    version_pattern = re.compile(r'(__version__\s*=\s*")[^"]+(")')
    with file_path.open("r", encoding="utf-8") as file:
        content = file.read()

    new_content, count = version_pattern.subn(rf"\1{new_version}\2", content)
    if count == 0:
        print(f"未找到版本号模式，文件未修改: {file_path}")
        sys.exit(1)

    with file_path.open("w", encoding="utf-8") as file:
        file.write(new_content)
    print(f"已更新文件中的版本号: {file_path}")


def update_changelog_version(file_path: Path, new_version: str) -> None:
    """更新CHANGELOG.md文件中的版本号

    更新格式如: # 🚀 v1.6.6 - 累积更新！ 改为新的版本号
    """
    version_pattern = re.compile(r"(# 🚀 v)\d+\.\d+\.\d+(- 累积更新！)")
    with file_path.open("r", encoding="utf-8") as file:
        content = file.read()

    new_content, count = version_pattern.subn(rf"\g<1>{new_version}\g<2>", content)
    if count == 0:
        print(f"⚠️  CHANGELOG.md中未找到版本号标题，文件未修改: {file_path}")
        return

    with file_path.open("w", encoding="utf-8") as file:
        file.write(new_content)
    print(f"已更新CHANGELOG.md中的版本号: {file_path}")


def main() -> None:
    """主函数"""
    print("🚀 开始发布流程...")

    # 获取版本号
    version = input("请输入要发布的版本号 (格式: x.y.z, 例如: 1.0.0): ").strip()

    # 简单验证版本号格式
    if not re.match(r"^\d+\.\d+\.\d+$", version):
        print("❌ 版本号格式不正确，请使用 x.y.z 格式")
        sys.exit(1)

    version_with_v = f"v{version}"
    print(f"准备发布版本: {version_with_v}")

    # 确认发布
    confirm = input(f"确认发布版本 {version_with_v}? (y/N): ")
    if confirm.lower() != "y":
        print("发布已取消")
        sys.exit(0)

    try:
        # 1. 使用pdm更新版本号
        print(f"\n1. 使用pdm更新版本号到 {version}...")
        run_command(f"pdm bump to {version}")

        # 2. 更新src/core/config/__init__.py中的版本号
        print(f"\n2. 更新源码中的版本号...")
        version_file = Path("src/core/config/__init__.py")
        if version_file.exists():
            update_version_in_file(version_file, version_with_v)
        else:
            print(f"⚠️  版本文件不存在: {version_file}，跳过")

        # 3. 更新docs/CHANGELOG.md中的版本号
        print(f"\n3. 更新CHANGELOG.md中的版本号...")
        changelog_file = Path("docs/CHANGELOG.md")
        if changelog_file.exists():
            update_changelog_version(changelog_file, version)
        else:
            print(f"⚠️  CHANGELOG.md文件不存在: {changelog_file}，跳过")

        # 4. 提交更改
        print(f"\n4. 提交版本更改...")
        run_command("git add .")
        run_command(f'git commit -m "🌟 releases: 发布 {version_with_v} 版本"')

        # 5. 创建标签
        print(f"\n5. 创建版本标签 {version_with_v}...")
        run_command(f'git tag -a {version_with_v} -m "发布 {version_with_v} 版本"')

        print(f"\n🎉 版本 {version_with_v} 发布完成!")
        print("\n请手动执行以下命令推送更改:")
        print(f"git push origin main")
        print(f"git push origin {version_with_v}")

    except Exception as e:
        print(f"❌ 发布过程中出现错误: {e}")
        sys.exit(1)


if __name__ == "__main__":
    main()
