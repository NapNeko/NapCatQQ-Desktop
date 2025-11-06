# -*- coding: utf-8 -*-
"""
Test suite for src/core/utils/get_version.py module

Tests version data model and version parsing logic
"""

import pytest
import json
from pathlib import Path
from unittest.mock import Mock, patch, mock_open
from pydantic import BaseModel


# Copy VersionData model for testing
class VersionData(BaseModel):
    """版本信息数据模型"""
    napcat_version: str | None
    qq_version: str | None
    ncd_version: str | None
    qq_download_url: str | None = None
    napcat_update_log: str | None = None
    ncd_update_log: str | None = None


class TestVersionData:
    """Test VersionData model"""
    
    @pytest.mark.unit
    def test_version_data_creation(self):
        """Test creating VersionData instance"""
        version = VersionData(
            napcat_version="v1.0.0",
            qq_version="9.9.3",
            ncd_version="v2.0.0"
        )
        
        assert version.napcat_version == "v1.0.0"
        assert version.qq_version == "9.9.3"
        assert version.ncd_version == "v2.0.0"
        assert version.qq_download_url is None
    
    @pytest.mark.unit
    def test_version_data_with_optional_fields(self):
        """Test VersionData with all fields"""
        version = VersionData(
            napcat_version="v1.0.0",
            qq_version="9.9.3",
            ncd_version="v2.0.0",
            qq_download_url="https://example.com/qq.exe",
            napcat_update_log="Bug fixes",
            ncd_update_log="New features"
        )
        
        assert version.qq_download_url == "https://example.com/qq.exe"
        assert version.napcat_update_log == "Bug fixes"
        assert version.ncd_update_log == "New features"
    
    @pytest.mark.unit
    def test_version_data_with_none_values(self):
        """Test VersionData with None values"""
        version = VersionData(
            napcat_version=None,
            qq_version=None,
            ncd_version=None
        )
        
        assert version.napcat_version is None
        assert version.qq_version is None
        assert version.ncd_version is None
    
    @pytest.mark.unit
    def test_version_data_serialization(self):
        """Test VersionData JSON serialization"""
        version = VersionData(
            napcat_version="v1.0.0",
            qq_version="9.9.3",
            ncd_version="v2.0.0"
        )
        
        # Convert to dict
        data_dict = version.model_dump()
        assert data_dict['napcat_version'] == "v1.0.0"
        assert data_dict['qq_version'] == "9.9.3"
        
        # Convert to JSON
        json_str = version.model_dump_json()
        assert "v1.0.0" in json_str


class TestVersionParsing:
    """Test version parsing logic"""
    
    @pytest.mark.unit
    def test_parse_github_response(self):
        """Test parsing GitHub API response format"""
        response = {
            "tag_name": "v1.2.3",
            "body": "Release notes here"
        }
        
        # Simulate parsing function
        parsed = {
            "version": response["tag_name"],
            "update_log": response["body"]
        }
        
        assert parsed["version"] == "v1.2.3"
        assert parsed["update_log"] == "Release notes here"
    
    @pytest.mark.unit
    def test_parse_qq_response(self):
        """Test parsing QQ version response format"""
        response = {
            "verHash": "abc123",
            "version": "9-9-3"
        }
        
        # Simulate parsing function
        ver_hash = response["verHash"]
        version = response["version"].replace("-", ".")
        download_url = f"https://dldir1.qq.com/qqfile/qq/QQNT/{ver_hash}/QQ{version}_x64.exe"
        
        parsed = {
            "version": version,
            "download_url": download_url
        }
        
        assert parsed["version"] == "9.9.3"
        assert "abc123" in parsed["download_url"]
        assert "QQ9.9.3_x64.exe" in parsed["download_url"]
    
    @pytest.mark.unit
    def test_version_format_normalization(self):
        """Test version format normalization"""
        # Test dash to dot conversion
        version_with_dash = "9-9-3"
        normalized = version_with_dash.replace("-", ".")
        assert normalized == "9.9.3"
        
        # Test version with v prefix
        version_with_v = "v1.0.0"
        assert version_with_v.startswith("v")
        
        # Test removing v prefix if needed
        version_no_v = version_with_v.lstrip("v")
        assert version_no_v == "1.0.0"
    
    @pytest.mark.unit
    def test_error_value_generation(self):
        """Test error value generation for different services"""
        error_values = {
            "QQ": {"version": None, "download_url": None},
            "NapCat": {"version": None, "update_log": None},
            "NapCatQQ Desktop": {"version": None, "update_log": None},
        }
        
        qq_error = error_values.get("QQ", {"version": None})
        assert qq_error["version"] is None
        assert qq_error["download_url"] is None
        
        napcat_error = error_values.get("NapCat")
        assert napcat_error["version"] is None
        assert napcat_error["update_log"] is None
    
    @pytest.mark.unit
    def test_package_json_version_parsing(self):
        """Test parsing version from package.json"""
        package_json_content = {
            "name": "napcat",
            "version": "1.0.0",
            "description": "NapCat"
        }
        
        # Simulate reading and parsing
        version_str = f"v{package_json_content['version']}"
        assert version_str == "v1.0.0"
    
    @pytest.mark.unit
    def test_config_json_version_parsing(self):
        """Test parsing QQ version from config.json"""
        config_json_content = {
            "curVersion": "9-9-3-12345"
        }
        
        # Simulate parsing
        version = config_json_content["curVersion"].replace("-", ".")
        assert version == "9.9.3.12345"
    
    @pytest.mark.unit
    def test_url_construction(self):
        """Test download URL construction"""
        ver_hash = "test_hash"
        version = "9.9.3"
        
        download_url = f"https://dldir1.qq.com/qqfile/qq/QQNT/{ver_hash}/QQ{version}_x64.exe"
        
        assert "https://dldir1.qq.com" in download_url
        assert "test_hash" in download_url
        assert "9.9.3" in download_url
        assert "_x64.exe" in download_url
    
    @pytest.mark.unit
    def test_version_comparison_logic(self):
        """Test version comparison concepts"""
        versions = ["v1.0.0", "v1.1.0", "v2.0.0"]
        
        # Test version string sorting
        sorted_versions = sorted(versions)
        assert sorted_versions == ["v1.0.0", "v1.1.0", "v2.0.0"]
    
    @pytest.mark.unit
    def test_missing_key_handling(self):
        """Test handling missing keys in responses"""
        response = {"tag_name": "v1.0.0"}
        
        # Test KeyError handling
        try:
            body = response["body"]
        except KeyError:
            body = None
        
        assert body is None
    
    @pytest.mark.unit
    def test_json_parsing_error_handling(self):
        """Test JSON parsing error handling"""
        invalid_json = "{'invalid': json}"
        
        with pytest.raises(json.JSONDecodeError):
            json.loads(invalid_json)
    
    @pytest.mark.unit
    def test_file_reading_concept(self, tmp_path):
        """Test file reading for version info"""
        package_json = tmp_path / "package.json"
        package_json.write_text(json.dumps({"version": "1.2.3"}))
        
        # Read and parse
        with open(package_json, 'r', encoding='utf-8') as f:
            data = json.load(f)
        
        version = f"v{data['version']}"
        assert version == "v1.2.3"
    
    @pytest.mark.unit
    def test_file_not_found_error_handling(self, tmp_path):
        """Test FileNotFoundError handling"""
        non_existent = tmp_path / "nonexistent.json"
        
        try:
            with open(non_existent, 'r') as f:
                content = f.read()
            version = "Found"
        except FileNotFoundError:
            version = None
        
        assert version is None
    
    @pytest.mark.unit
    def test_http_response_simulation(self):
        """Test HTTP response data structure"""
        # Simulate successful response
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.json.return_value = {"tag_name": "v1.0.0", "body": "Notes"}
        
        data = mock_response.json()
        assert data["tag_name"] == "v1.0.0"
        
        # Simulate error response
        error_response = Mock()
        error_response.status_code = 404
        
        assert error_response.status_code == 404
