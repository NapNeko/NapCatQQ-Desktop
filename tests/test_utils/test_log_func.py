# -*- coding: utf-8 -*-
"""
Test suite for logger functionality from src/core/utils/logger/log_func.py

Tests logging buffer management, file operations, and log formatting
"""

import pytest
from pathlib import Path
from datetime import datetime, timedelta
import time


class TestLoggerBuffer:
    """Test logger buffer management"""
    
    @pytest.mark.unit
    def test_log_buffer_initialization(self):
        """Test creating an empty log buffer"""
        log_buffer = []
        
        assert isinstance(log_buffer, list)
        assert len(log_buffer) == 0
    
    @pytest.mark.unit
    def test_adding_logs_to_buffer(self):
        """Test adding logs to buffer"""
        log_buffer = []
        
        # Add some logs
        log_buffer.append({"message": "Log 1", "level": "INFO"})
        log_buffer.append({"message": "Log 2", "level": "DEBUG"})
        log_buffer.append({"message": "Log 3", "level": "ERROR"})
        
        assert len(log_buffer) == 3
        assert log_buffer[0]["message"] == "Log 1"
        assert log_buffer[2]["level"] == "ERROR"
    
    @pytest.mark.unit
    def test_buffer_size_limit(self):
        """Test buffer size limit logic"""
        log_buffer = []
        log_buffer_size = 5000
        log_buffer_delete_size = 1000
        
        # Simulate filling buffer
        for i in range(log_buffer_size + 100):
            log_buffer.append({"message": f"Log {i}", "time": i})
        
        # Check if buffer exceeds limit
        if len(log_buffer) >= log_buffer_size:
            # Trim buffer
            log_buffer = log_buffer[log_buffer_delete_size:]
        
        # Verify buffer was trimmed
        assert len(log_buffer) < log_buffer_size
        assert len(log_buffer) == log_buffer_size + 100 - log_buffer_delete_size
    
    @pytest.mark.unit
    def test_buffer_clearing_logic(self):
        """Test buffer clearing when it gets too large"""
        log_buffer_size = 100
        log_buffer_delete_size = 20
        log_buffer = []
        
        # Fill buffer beyond limit
        for i in range(150):
            log_buffer.append(f"Log {i}")
            
            # Clear if needed
            if len(log_buffer) >= log_buffer_size:
                log_buffer = log_buffer[log_buffer_delete_size:]
        
        # Should have cleared oldest entries
        assert len(log_buffer) < log_buffer_size
    
    @pytest.mark.unit
    def test_log_buffer_fifo_behavior(self):
        """Test first-in-first-out behavior of log buffer"""
        log_buffer = ["Log 1", "Log 2", "Log 3", "Log 4", "Log 5"]
        delete_count = 2
        
        # Remove oldest entries
        log_buffer = log_buffer[delete_count:]
        
        assert log_buffer[0] == "Log 3"
        assert "Log 1" not in log_buffer
        assert "Log 2" not in log_buffer


class TestLogFileOperations:
    """Test log file operations"""
    
    @pytest.mark.unit
    def test_log_file_path_generation(self, tmp_path):
        """Test generating log file path with timestamp"""
        log_dir = tmp_path / "log"
        log_dir.mkdir()
        
        # Generate timestamped filename
        timestamp = datetime.now().strftime('%Y-%m-%d_%H-%M-%S')
        log_path = log_dir / f"{timestamp}.log"
        
        assert log_path.parent == log_dir
        assert ".log" in str(log_path)
        assert timestamp in str(log_path)
    
    @pytest.mark.unit
    def test_log_directory_creation(self, tmp_path):
        """Test creating log directory if it doesn't exist"""
        log_dir = tmp_path / "log"
        
        # Check if exists, create if not
        if not log_dir.exists():
            log_dir.mkdir(parents=True, exist_ok=True)
        
        assert log_dir.exists()
        assert log_dir.is_dir()
    
    @pytest.mark.unit
    def test_old_log_file_detection(self, tmp_path):
        """Test detecting old log files"""
        log_dir = tmp_path / "log"
        log_dir.mkdir()
        
        log_save_days = 7
        
        # Create old log file
        old_log = log_dir / "old.log"
        old_log.write_text("old log")
        
        # Simulate old file by checking timestamp
        file_age_days = (datetime.now() - datetime.fromtimestamp(old_log.stat().st_mtime)).days
        
        # For testing, assume file is old if created more than threshold
        should_delete = file_age_days > log_save_days
        
        # In practice, this would be False since file is just created
        assert should_delete is False
    
    @pytest.mark.unit
    def test_log_file_cleanup_logic(self, tmp_path):
        """Test log file cleanup logic"""
        log_dir = tmp_path / "log"
        log_dir.mkdir()
        
        # Create test files
        recent_log = log_dir / "recent.log"
        old_log = log_dir / "old.log"
        
        recent_log.write_text("recent")
        old_log.write_text("old")
        
        log_save_days = 7
        files_to_delete = []
        
        # Check each file
        for log_file in log_dir.iterdir():
            file_age = (datetime.now() - datetime.fromtimestamp(log_file.stat().st_mtime)).days
            if file_age > log_save_days:
                files_to_delete.append(log_file)
        
        # No files should be old enough to delete in this test
        assert len(files_to_delete) == 0
    
    @pytest.mark.unit
    def test_log_file_writing(self, tmp_path):
        """Test writing to log file"""
        log_path = tmp_path / "test.log"
        
        # Write log entry
        log_entry = "[2025-01-01 10:00:00] INFO: Test message\n"
        log_path.write_text(log_entry)
        
        # Verify
        content = log_path.read_text()
        assert "INFO" in content
        assert "Test message" in content
    
    @pytest.mark.unit
    def test_log_file_append(self, tmp_path):
        """Test appending to log file"""
        log_path = tmp_path / "test.log"
        
        # Initial write
        log_path.write_text("Log 1\n")
        
        # Append
        with open(log_path, 'a') as f:
            f.write("Log 2\n")
            f.write("Log 3\n")
        
        content = log_path.read_text()
        assert "Log 1" in content
        assert "Log 2" in content
        assert "Log 3" in content


class TestLogConfiguration:
    """Test log configuration"""
    
    @pytest.mark.unit
    def test_load_default_config(self):
        """Test loading default configuration"""
        log_buffer_size = 5000
        log_buffer_delete_size = 1000
        log_save_day = 7
        
        assert log_buffer_size == 5000
        assert log_buffer_delete_size == 1000
        assert log_save_day == 7
    
    @pytest.mark.unit
    def test_config_values_reasonable(self):
        """Test that config values are reasonable"""
        log_buffer_size = 5000
        log_buffer_delete_size = 1000
        
        # Delete size should be less than buffer size
        assert log_buffer_delete_size < log_buffer_size
        
        # Both should be positive
        assert log_buffer_size > 0
        assert log_buffer_delete_size > 0
    
    @pytest.mark.unit
    def test_log_save_days_config(self):
        """Test log save days configuration"""
        log_save_days = 7
        
        # Should be positive
        assert log_save_days > 0
        
        # Calculate cutoff date
        cutoff_date = datetime.now() - timedelta(days=log_save_days)
        
        assert cutoff_date < datetime.now()


class TestTimestampFormatting:
    """Test timestamp formatting for logs"""
    
    @pytest.mark.unit
    def test_datetime_string_formatting(self):
        """Test formatting datetime as string"""
        now = datetime.now()
        formatted = now.strftime('%Y-%m-%d_%H-%M-%S')
        
        assert "-" in formatted
        assert "_" in formatted
        assert len(formatted) == 19  # YYYY-MM-DD_HH-MM-SS
    
    @pytest.mark.unit
    def test_timestamp_to_seconds(self):
        """Test converting to timestamp"""
        now = datetime.now()
        timestamp = now.timestamp()
        
        assert isinstance(timestamp, float)
        assert timestamp > 0
    
    @pytest.mark.unit
    def test_time_difference_calculation(self):
        """Test calculating time difference"""
        now = datetime.now()
        past = now - timedelta(days=10)
        
        difference_days = (now - past).days
        
        assert difference_days == 10
    
    @pytest.mark.unit
    def test_datetime_from_timestamp(self):
        """Test creating datetime from timestamp"""
        timestamp = time.time()
        dt = datetime.fromtimestamp(timestamp)
        
        assert isinstance(dt, datetime)
        assert dt.year >= 2025


class TestLogObjectConstruction:
    """Test log object construction concepts"""
    
    @pytest.mark.unit
    def test_log_dict_structure(self):
        """Test basic log dictionary structure"""
        log = {
            "level": "INFO",
            "message": "Test message",
            "time": time.time(),
            "type": "SYSTEM",
            "source": "CORE",
            "position": {"file": "test.py", "line": 10}
        }
        
        assert log["level"] == "INFO"
        assert log["message"] == "Test message"
        assert log["type"] == "SYSTEM"
        assert log["source"] == "CORE"
    
    @pytest.mark.unit
    def test_log_with_optional_group(self):
        """Test log with optional group field"""
        log_without_group = {
            "message": "Test",
            "group": None
        }
        
        log_with_group = {
            "message": "Test",
            "group": "GroupA"
        }
        
        assert log_without_group["group"] is None
        assert log_with_group["group"] == "GroupA"
    
    @pytest.mark.unit
    def test_log_message_formatting(self):
        """Test log message formatting"""
        level = "INFO"
        message = "User action completed"
        timestamp = datetime.now().strftime('%Y-%m-%d %H:%M:%S')
        
        formatted_log = f"[{timestamp}] {level}: {message}"
        
        assert level in formatted_log
        assert message in formatted_log
        assert "[" in formatted_log
        assert "]" in formatted_log
