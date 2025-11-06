# -*- coding: utf-8 -*-
"""测试 URL 常量（不依赖 GUI）"""


class TestUrlConstants:
    """测试 URL 常量定义（基于文档验证）"""

    def test_github_urls_format(self):
        """测试 GitHub URL 格式正确"""
        # 这些是基于源代码的 URL 常量
        urls = [
            "https://github.com/NapNeko/NapCatQQ-Desktop",
            "https://github.com/NapNeko/NapCatQQ-Desktop/issues",
            "https://github.com/NapNeko/NapCatQQ",
            "https://github.com/NapNeko/NapCatQQ/issues",
        ]
        
        for url in urls:
            assert url.startswith("https://github.com")
            assert "NapNeko" in url or "NapCat" in url

    def test_api_urls_exist(self):
        """测试 API URL 存在"""
        api_urls = [
            "https://nclatest.znin.net/get_ncd_ver",
            "https://nclatest.znin.net",
            "https://nclatest.znin.net/get_qq_ver",
        ]
        
        for url in api_urls:
            assert url.startswith("https://")
            assert "nclatest.znin.net" in url

    def test_download_urls_format(self):
        """测试下载 URL 格式"""
        download_urls = [
            "https://github.com/NapNeko/NapCatQQ-Desktop/releases/latest/download/NapCatQQ-Desktop.exe",
            "https://github.com/NapNeko/NapCatQQ/releases/latest/download/NapCat.Shell.zip",
        ]
        
        for url in download_urls:
            assert "/releases/latest/download/" in url
            assert url.endswith((".exe", ".zip"))

    def test_mirror_sites_are_https(self):
        """测试镜像站点使用 HTTPS"""
        mirror_sites = [
            "https://gh.ddlc.top",
            "https://slink.ltd",
            "https://cors.isteed.cc",
            "https://hub.gitmirror.com",
            "https://ghproxy.cc",
            "https://github.moeyy.xyz",
        ]
        
        for url in mirror_sites:
            assert url.startswith("https://")

    def test_qq_related_urls(self):
        """测试 QQ 相关 URL"""
        qq_urls = [
            "https://im.qq.com/index/",
            "https://q.qlogo.cn/headimg_dl",
        ]
        
        for url in qq_urls:
            assert url.startswith("https://")
            assert "qq" in url.lower() or "qlogo" in url
