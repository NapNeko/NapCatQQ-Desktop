# -*- coding: utf-8 -*-

# 标准库导入
import sys
import subprocess


def run_command(command):
    """运行命令并打印输出"""
    try:
        result = subprocess.run(command, check=True, text=True, capture_output=True, encoding="utf-8")
        print(result.stdout)
    except subprocess.CalledProcessError as e:
        print(f"运行命令 {command} 时出错")
        print(e.stderr)
        sys.exit(1)


def format_code():
    """自动格式化代码，运行 black, isort 和 autoflake"""
    print("🌟 运行 autoflake 以删除未使用的导入...")
    run_command(["autoflake", "src"])

    print("🌟 运行 isort 对导入进行排序...")
    run_command(["isort", "src"])

    print("🌟 运行 black 来格式化代码...")
    run_command(["black", "src"])


if __name__ == "__main__":
    format_code()
