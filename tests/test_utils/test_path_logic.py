# -*- coding: utf-8 -*-
"""
Test suite for path handling logic from src/core/utils/path_func.py

Tests path operations, validation, and migration logic without Windows registry dependency
"""

import pytest
from pathlib import Path
import shutil


class TestPathHandling:
    """Test path handling concepts"""
    
    @pytest.mark.unit
    def test_path_creation_and_existence(self, tmp_path):
        """Test creating and checking path existence"""
        test_dir = tmp_path / "test_directory"
        
        # Initially doesn't exist
        assert not test_dir.exists()
        
        # Create directory
        test_dir.mkdir(parents=True, exist_ok=True)
        
        # Now exists
        assert test_dir.exists()
        assert test_dir.is_dir()
    
    @pytest.mark.unit
    def test_nested_path_creation(self, tmp_path):
        """Test creating nested directories"""
        nested_dir = tmp_path / "level1" / "level2" / "level3"
        
        # Create all levels at once
        nested_dir.mkdir(parents=True, exist_ok=True)
        
        assert nested_dir.exists()
        assert (tmp_path / "level1").exists()
        assert (tmp_path / "level1" / "level2").exists()
    
    @pytest.mark.unit
    def test_path_exists_check_before_creation(self, tmp_path):
        """Test checking if path exists before creation"""
        test_dir = tmp_path / "existing_dir"
        
        # Create directory
        test_dir.mkdir()
        
        # Try to create again with exist_ok=True
        test_dir.mkdir(parents=True, exist_ok=True)
        
        # Should still exist without error
        assert test_dir.exists()
    
    @pytest.mark.unit
    def test_multiple_paths_validation(self, tmp_path):
        """Test validating multiple paths"""
        paths_to_validate = [
            (tmp_path / "tmp", "Tmp"),
            (tmp_path / "config", "config"),
            (tmp_path / "napcat", "NapCat")
        ]
        
        for path, name in paths_to_validate:
            if not path.exists():
                path.mkdir(parents=True, exist_ok=True)
                status = f"创建路径 {name.center(8)} 成功"
            else:
                status = f"路径 {name.center(8)} 已存在"
            
            assert path.exists()
            assert "成功" in status or "已存在" in status
    
    @pytest.mark.unit
    def test_path_migration_detection(self, tmp_path):
        """Test detecting if path migration is needed"""
        old_config_path = tmp_path / "config"
        new_config_path = tmp_path / "runtime" / "config"
        
        # Create old path
        old_config_path.mkdir()
        
        # Check if migration is needed
        needs_migration = old_config_path.exists() and not new_config_path.exists()
        
        assert needs_migration is True
    
    @pytest.mark.unit
    def test_path_migration_process(self, tmp_path):
        """Test path migration process"""
        # Old paths
        old_napcat = tmp_path / "NapCat"
        old_config = tmp_path / "config"
        old_tmp = tmp_path / "tmp"
        
        # New paths
        runtime = tmp_path / "runtime"
        new_napcat = runtime / "NapCatQQ"
        new_config = runtime / "config"
        new_tmp = runtime / "tmp"
        
        # Create old directories with some files
        old_napcat.mkdir()
        old_config.mkdir()
        (old_config / "test.json").write_text('{"test": true}')
        
        # Perform migration
        if old_config.exists():
            runtime.mkdir(exist_ok=True)
            
            # Copy old to new
            if old_napcat.exists():
                shutil.copytree(old_napcat, new_napcat, dirs_exist_ok=True)
            if old_config.exists():
                shutil.copytree(old_config, new_config, dirs_exist_ok=True)
        
        # Verify migration
        assert new_config.exists()
        assert (new_config / "test.json").exists()
        assert (new_config / "test.json").read_text() == '{"test": true}'
    
    @pytest.mark.unit
    def test_path_composition(self, tmp_path):
        """Test composing paths from base path"""
        base_path = tmp_path
        runtime_path = base_path / "runtime"
        
        # Compose various paths
        napcat_path = runtime_path / "NapCatQQ"
        config_dir = runtime_path / "config"
        tmp_dir = runtime_path / "tmp"
        
        config_file = config_dir / "config.json"
        bot_config = config_dir / "bot.json"
        
        # Verify path composition
        assert str(napcat_path).endswith("NapCatQQ")
        assert str(config_file).endswith("config.json")
        assert str(bot_config).endswith("bot.json")
    
    @pytest.mark.unit
    def test_cwd_path_handling(self, tmp_path):
        """Test current working directory path handling"""
        # Simulate base path as current directory
        base_path = tmp_path
        
        # Relative path handling
        relative_path = Path("runtime") / "config"
        absolute_path = base_path / relative_path
        
        assert absolute_path.is_absolute()
        assert base_path.is_absolute()
    
    @pytest.mark.unit
    def test_old_version_paths_structure(self):
        """Test old version paths structure"""
        # Simulate old version paths
        old_paths = {
            "napcat_path": Path.cwd() / "NapCat",
            "config_dir_path": Path.cwd() / "config",
            "tmp_path": Path.cwd() / "tmp",
        }
        
        assert "napcat_path" in old_paths
        assert "config_dir_path" in old_paths
        assert "tmp_path" in old_paths
        
        # Verify paths are Path objects
        assert isinstance(old_paths["napcat_path"], Path)
    
    @pytest.mark.unit
    def test_path_migration_skip_when_not_needed(self, tmp_path):
        """Test skipping migration when not needed"""
        old_config_path = tmp_path / "old_config"
        
        # Don't create old path
        
        # Check migration condition
        if not old_config_path.exists():
            migration_result = "无需进行路径迁移"
        else:
            migration_result = "需要迁移"
        
        assert migration_result == "无需进行路径迁移"
    
    @pytest.mark.unit
    def test_directory_move_with_shutil(self, tmp_path):
        """Test moving directories with shutil"""
        source = tmp_path / "source"
        destination = tmp_path / "destination"
        
        # Create source with content
        source.mkdir()
        (source / "file1.txt").write_text("content1")
        (source / "subdir").mkdir()
        (source / "subdir" / "file2.txt").write_text("content2")
        
        # Move directory
        shutil.copytree(source, destination)
        
        # Verify destination
        assert destination.exists()
        assert (destination / "file1.txt").read_text() == "content1"
        assert (destination / "subdir" / "file2.txt").read_text() == "content2"
    
    @pytest.mark.unit
    def test_path_string_representation(self, tmp_path):
        """Test converting paths to strings"""
        test_path = tmp_path / "runtime" / "config"
        
        # Convert to string
        path_str = str(test_path)
        
        assert isinstance(path_str, str)
        assert "runtime" in path_str
        assert "config" in path_str
    
    @pytest.mark.unit
    def test_parent_directory_access(self, tmp_path):
        """Test accessing parent directories"""
        nested_path = tmp_path / "a" / "b" / "c"
        nested_path.mkdir(parents=True)
        
        # Get parent directories
        parent = nested_path.parent
        grandparent = nested_path.parent.parent
        
        assert parent.name == "b"
        assert grandparent.name == "a"
    
    @pytest.mark.unit
    def test_file_path_within_directory(self, tmp_path):
        """Test creating file paths within directories"""
        config_dir = tmp_path / "config"
        config_dir.mkdir()
        
        # Create file paths
        config_file = config_dir / "config.json"
        bot_file = config_dir / "bot.json"
        
        # Write files
        config_file.write_text('{"config": true}')
        bot_file.write_text('{"bot": true}')
        
        # Verify files exist
        assert config_file.exists()
        assert bot_file.exists()
        assert config_file.is_file()
    
    @pytest.mark.unit
    def test_exist_ok_parameter(self, tmp_path):
        """Test exist_ok parameter behavior"""
        test_dir = tmp_path / "test"
        
        # First creation
        test_dir.mkdir()
        
        # Second creation with exist_ok=True
        test_dir.mkdir(exist_ok=True)
        
        # Should not raise error
        assert test_dir.exists()
        
        # Without exist_ok would raise error
        with pytest.raises(FileExistsError):
            test_dir.mkdir(exist_ok=False)
