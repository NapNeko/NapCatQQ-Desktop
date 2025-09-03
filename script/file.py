# 标准库导入
import os

# 项目根目录
PROJECT_ROOT = r"D:\NapCat-Project\NapCatQQ-Desktop-V1\src\ui\common"  # 可改成你的项目路径

# 需要排除的文件夹名称
EXCLUDE_DIRS = {".venv", ".vscode", ".github", "docs", "tmp", ".git", "__pycache__"}  # 可根据需要修改


def list_files(root_dir, exclude_dirs):
    all_files = []
    for dirpath, dir_names, filenames in os.walk(root_dir):
        # 排除指定目录
        dir_names[:] = [d for d in dir_names if d not in exclude_dirs]
        for file in filenames:
            # 获取相对路径
            rel_path = os.path.relpath(os.path.join(dirpath, file), root_dir)
            all_files.append(rel_path)
    return all_files


if __name__ == "__main__":
    files = list_files(PROJECT_ROOT, EXCLUDE_DIRS)
    # 格式化输出
    print("项目中所有文件列表（已排除指定文件夹）:")
    for f in files:
        print(f"- {f}")
