# -*- coding: utf-8 -*-

# 标准库导入
import zipfile
from pathlib import Path
from types import SimpleNamespace

# 项目内模块导入
import src.core.utils.file as file_module
import src.core.utils.install_func as install_func


def test_qfluent_file_supports_context_manager(tmp_path: Path) -> None:
    """QFluentFile 应支持 with 打开并在退出后关闭。"""
    file_path = tmp_path / "sample.txt"
    file_path.write_text("hello", encoding="utf-8")

    with file_module.QFluentFile(file_path, file_module.QFile.OpenModeFlag.ReadOnly) as handle:
        assert bytes(handle.readAll().data()).decode("utf-8") == "hello"
        assert handle.isOpen() is True

    assert handle.isOpen() is False


def test_qfluent_file_raises_for_missing_file(tmp_path: Path) -> None:
    """打开不存在的文件时应抛出 IOError。"""
    missing = tmp_path / "missing.txt"

    try:
        with file_module.QFluentFile(missing, file_module.QFile.OpenModeFlag.ReadOnly):
            raise AssertionError("should not reach here")
    except IOError:
        pass


def mute_install_logger(monkeypatch) -> None:
    """屏蔽安装模块的日志副作用。"""
    monkeypatch.setattr(install_func.logger, "info", lambda *args, **kwargs: None)
    monkeypatch.setattr(install_func.logger, "error", lambda *args, **kwargs: None)
    monkeypatch.setattr(install_func.logger, "exception", lambda *args, **kwargs: None)


def test_napcat_install_remove_old_file_preserves_config_and_log(monkeypatch, tmp_path: Path) -> None:
    """删除旧文件时应保留 config/log 目录。"""
    mute_install_logger(monkeypatch)
    install_root = tmp_path / "NapCatQQ"
    tmp_root = tmp_path / "tmp"
    install_root.mkdir()
    tmp_root.mkdir()
    (install_root / "config").mkdir()
    (install_root / "log").mkdir()
    (install_root / "plugins").mkdir()
    (install_root / "old.txt").write_text("remove", encoding="utf-8")
    (install_root / "plugins" / "plugin.js").write_text("remove", encoding="utf-8")

    monkeypatch.setattr(
        install_func,
        "it",
        lambda cls: SimpleNamespace(tmp_path=tmp_root, napcat_path=install_root),
    )

    task = install_func.NapCatInstall()
    task.remove_old_file()

    assert (install_root / "config").exists()
    assert (install_root / "log").exists()
    assert not (install_root / "plugins").exists()
    assert not (install_root / "old.txt").exists()


def test_napcat_install_unzip_file_extracts_package_and_removes_zip(monkeypatch, tmp_path: Path) -> None:
    """解压 NapCat 安装包后应写入目标目录并删除 zip 文件。"""
    mute_install_logger(monkeypatch)
    install_root = tmp_path / "NapCatQQ"
    tmp_root = tmp_path / "tmp"
    install_root.mkdir()
    tmp_root.mkdir()
    zip_path = tmp_root / "NapCat.Shell.zip"

    with zipfile.ZipFile(zip_path, "w") as archive:
        archive.writestr("module/file.txt", "payload")

    monkeypatch.setattr(
        install_func,
        "it",
        lambda cls: SimpleNamespace(tmp_path=tmp_root, napcat_path=install_root),
    )

    task = install_func.NapCatInstall()
    finished: list[bool] = []
    task.install_finish_signal.connect(lambda: finished.append(True))
    task.unzip_file()

    assert (install_root / "module" / "file.txt").read_text(encoding="utf-8") == "payload"
    assert not zip_path.exists()
    assert finished == [True]


def test_qq_install_nonzero_exit_emits_error_and_deletes_installer(monkeypatch, tmp_path: Path) -> None:
    """QQ 安装程序返回非零退出码时应发出失败信号并删除安装包。"""
    mute_install_logger(monkeypatch)
    installer = tmp_path / "QQ.exe"
    installer.write_text("binary", encoding="utf-8")

    class Result:
        returncode = 1

    monkeypatch.setattr(install_func.subprocess, "run", lambda args: Result())

    task = install_func.QQInstall(installer)
    errors: list[bool] = []
    task.error_finish_signal.connect(lambda: errors.append(True))
    task.execute()

    assert errors == [True]
    assert not installer.exists()
