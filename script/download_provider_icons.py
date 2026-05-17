"""批量下载 @lobehub/icons-static-svg 仓库中的彩色 SVG 图标。

用法:
    python scripts/download_provider_icons.py

功能:
    - 从 unpkg CDN 获取 @lobehub/icons-static-svg 包的文件列表
    - 筛选所有 *-color.svg 文件并下载到 src/ui/resources/provider_icons/
    - 支持增量更新：跳过本地已存在的文件
    - 支持通过 --force 参数强制重新下载所有文件
"""

from __future__ import annotations

import argparse
import json
import logging
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

# ---------------------------------------------------------------------------
# 配置
# ---------------------------------------------------------------------------

# npm 包名和 CDN 基础 URL
PACKAGE_NAME = "@lobehub/icons-static-svg"
UNPKG_BASE = "https://unpkg.com"

# 获取包文件列表的 API（unpkg ?meta 返回 JSON 目录结构）
PACKAGE_META_URL = f"{UNPKG_BASE}/{PACKAGE_NAME}@latest/icons/?meta"

# 单个文件下载 URL 模板
ICON_DOWNLOAD_URL = f"{UNPKG_BASE}/{PACKAGE_NAME}@latest/icons/{{filename}}"

# 本地存储目录（相对于项目根目录）
OUTPUT_DIR = Path(__file__).resolve().parent.parent / "src" / "ui" / "resources" / "provider_icons"

# 下载超时（秒）
TIMEOUT = 30

# 请求间隔（秒），避免触发 CDN 限流
REQUEST_DELAY = 0.1

# 最大重试次数
MAX_RETRIES = 3

# ---------------------------------------------------------------------------
# 日志
# ---------------------------------------------------------------------------

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(message)s",
    datefmt="%H:%M:%S",
)
logger = logging.getLogger(__name__)

# ---------------------------------------------------------------------------
# 核心逻辑
# ---------------------------------------------------------------------------


def fetch_json(url: str) -> dict | list:
    """从 URL 获取 JSON 数据。"""
    req = urllib.request.Request(url, headers={"User-Agent": "NapCatQQ-Desktop-IconDownloader/1.0"})
    for attempt in range(1, MAX_RETRIES + 1):
        try:
            with urllib.request.urlopen(req, timeout=TIMEOUT) as resp:
                return json.loads(resp.read().decode("utf-8"))
        except (urllib.error.URLError, urllib.error.HTTPError, TimeoutError) as exc:
            if attempt < MAX_RETRIES:
                logger.warning("请求失败 (尝试 %d/%d): %s - %s", attempt, MAX_RETRIES, url, exc)
                time.sleep(1 * attempt)
            else:
                raise


def fetch_color_icon_list() -> list[str]:
    """从 unpkg meta API 获取所有 *-color.svg 文件名列表。"""
    logger.info("正在获取图标文件列表...")
    try:
        meta = fetch_json(PACKAGE_META_URL)
    except Exception as exc:
        logger.error("无法获取包元数据: %s", exc)
        logger.info("回退到备用方案：使用已知图标列表...")
        return _get_fallback_icon_list()

    # unpkg ?meta 返回格式: {"path": "/icons", "type": "directory", "files": [...]}
    files: list[dict] = meta.get("files", [])
    color_icons = [
        f["path"].rsplit("/", 1)[-1]  # 提取文件名部分，如 "/icons/openai-color.svg" → "openai-color.svg"
        for f in files
        if f.get("type") == "file" and f.get("path", "").endswith("-color.svg")
    ]
    logger.info("找到 %d 个彩色 SVG 图标", len(color_icons))
    if not color_icons:
        logger.info("API 返回空列表，回退到备用方案...")
        return _get_fallback_icon_list()
    return color_icons


def _get_fallback_icon_list() -> list[str]:
    """备用图标列表 - 包含常见 AI/LLM 供应商的彩色图标。

    当无法从 CDN 获取完整列表时使用此备用列表。
    """
    # 常见供应商图标 ID（仅 -color.svg 变体）
    known_providers = [
        "adobe", "adobefirefly", "agentvoice", "ai2", "ai21", "ai302", "ai360",
        "aihubmix", "aimass", "aionlabs", "akashchat", "alibaba", "alibabacloud",
        "amp", "antgroup", "antigravity", "anyscale", "apertis", "arcee",
        "askverdict", "assemblyai", "automatic", "aws", "aya", "azure", "azureai",
        "baichuan", "baidu", "baiducloud", "bailian", "bedrock", "bilibili",
        "bing", "briaai", "burncloud", "bytedance", "centml", "cerebras",
        "chatglm", "cherrystudio", "civitai", "claude", "claudecode", "cloudflare",
        "codeflicker", "codegeex", "codex", "cogvideo", "cogview", "cohere",
        "colab", "cometapi", "comfyui", "commanda", "copilot", "copilotkit",
        "coqui", "crewai", "crusoe", "dalle", "dbrx", "deepcogito", "deepinfra",
        "deepl", "deepmind", "deepseek", "dify", "doc2x", "docsearch", "doubao",
        "essentialai", "exa", "fal", "fastgpt", "featherless", "figma",
        "fireworks", "gemini", "geminicli", "gemma", "glmv", "google",
        "googlecloud", "gradio", "greptile", "hailuo", "higress", "huawei",
        "huaweicloud", "huggingface", "hunyuan", "hyperbolic", "iflytekcloud",
        "infermatic", "infinigence", "internlm", "jimeng", "junie", "kimi",
        "kling", "kluster", "kolors", "kwaipilot", "langchain", "langfuse",
        "langgraph", "langsmith", "leptonai", "lg", "livekit", "llamaindex",
        "llava", "llmapi", "lobehub", "longcat", "lovable", "luma", "make",
        "mcpso", "meta", "metaai", "microsoft", "minimax", "mistral",
        "modelscope", "monica", "morph", "myshell", "n8n", "nanobanana",
        "newapi", "nova", "novita", "nplcloud", "nvidia", "obsidian", "openchat",
        "openclaw", "openhands", "palm", "perplexity", "phidata", "pixverse",
        "player2", "poe", "ppio", "prunaai", "pydanticai", "qingyan", "qiniu",
        "qoder", "qwen", "replit", "rsshub", "rwkv", "sambanova", "search1api",
        "sensenova", "siliconcloud", "skywork", "smithery", "snowflake",
        "sophnet", "sora", "spark", "stability", "statecloud", "stepfun",
        "straico", "streamlake", "submodel", "targon", "tavily", "tencent",
        "tencentcloud", "tiangong", "tii", "together", "trae", "tripo",
        "udio", "unstructured", "upstage", "vertexai", "vidu", "vllm",
        "volcengine", "voyage", "wenxin", "workersai", "xinference", "xpay",
        "xuanyuan", "yi", "yuanbao", "zapier", "zeabur", "zencoder", "zhipu",
    ]
    return [f"{pid}-color.svg" for pid in known_providers]


def download_icon(filename: str, output_dir: Path, *, force: bool = False) -> bool:
    """下载单个图标文件。

    Args:
        filename: 图标文件名，如 "openai-color.svg"
        output_dir: 本地存储目录
        force: 是否强制覆盖已存在的文件

    Returns:
        True 表示下载成功或已跳过，False 表示下载失败
    """
    dest = output_dir / filename

    # 增量更新：跳过已存在的文件
    if dest.exists() and not force:
        return True

    url = ICON_DOWNLOAD_URL.format(filename=filename)
    req = urllib.request.Request(url, headers={"User-Agent": "NapCatQQ-Desktop-IconDownloader/1.0"})

    for attempt in range(1, MAX_RETRIES + 1):
        try:
            with urllib.request.urlopen(req, timeout=TIMEOUT) as resp:
                content = resp.read()
                # 基本验证：确保内容是 SVG
                if b"<svg" not in content.lower():
                    logger.warning("文件内容不是有效的 SVG: %s", filename)
                    return False
                dest.write_bytes(content)
                return True
        except urllib.error.HTTPError as exc:
            if exc.code == 404:
                logger.debug("图标不存在 (404): %s", filename)
                return False
            if attempt < MAX_RETRIES:
                logger.warning("下载失败 (尝试 %d/%d): %s - HTTP %d", attempt, MAX_RETRIES, filename, exc.code)
                time.sleep(1 * attempt)
            else:
                logger.error("下载失败（已达最大重试次数）: %s - HTTP %d", filename, exc.code)
                return False
        except (urllib.error.URLError, TimeoutError, OSError) as exc:
            if attempt < MAX_RETRIES:
                logger.warning("下载失败 (尝试 %d/%d): %s - %s", attempt, MAX_RETRIES, filename, exc)
                time.sleep(1 * attempt)
            else:
                logger.error("下载失败（已达最大重试次数）: %s - %s", filename, exc)
                return False

    return False


def main() -> int:
    """主入口。"""
    parser = argparse.ArgumentParser(
        description="从 @lobehub/icons-static-svg 批量下载彩色 SVG 图标"
    )
    parser.add_argument(
        "--force",
        action="store_true",
        help="强制重新下载所有文件（忽略已存在的文件）",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=OUTPUT_DIR,
        help=f"输出目录（默认: {OUTPUT_DIR}）",
    )
    parser.add_argument(
        "--delay",
        type=float,
        default=REQUEST_DELAY,
        help=f"请求间隔秒数（默认: {REQUEST_DELAY}）",
    )
    args = parser.parse_args()

    output_dir: Path = args.output
    output_dir.mkdir(parents=True, exist_ok=True)
    logger.info("输出目录: %s", output_dir)

    # 获取图标列表
    icon_files = fetch_color_icon_list()
    if not icon_files:
        logger.error("未找到任何彩色 SVG 图标")
        return 1

    # 统计
    total = len(icon_files)
    downloaded = 0
    skipped = 0
    failed = 0

    for i, filename in enumerate(icon_files, 1):
        dest = output_dir / filename
        if dest.exists() and not args.force:
            skipped += 1
            continue

        if download_icon(filename, output_dir, force=args.force):
            downloaded += 1
            if downloaded % 20 == 0:
                logger.info("进度: %d/%d (已下载 %d, 跳过 %d, 失败 %d)", i, total, downloaded, skipped, failed)
        else:
            failed += 1

        # 请求间隔
        if args.delay > 0:
            time.sleep(args.delay)

    logger.info(
        "完成! 总计 %d 个图标: 新下载 %d, 跳过 %d, 失败 %d",
        total, downloaded, skipped, failed,
    )
    return 0 if failed == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
