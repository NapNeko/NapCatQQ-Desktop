# -*- coding: utf-8 -*-

from pathlib import Path

import pytest

import src.core.installation.install_type as install_type


def test_detect_install_type_matches_registry_path_case_insensitively(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> None:
    monkeypatch.setattr(install_type, "HAS_WINREG", True)
    monkeypatch.setattr(
        install_type,
        "_get_msi_install_dir_from_registry",
        lambda: str(tmp_path).upper().replace("/", "\\"),
    )

    assert install_type.detect_install_type(tmp_path) == install_type.InstallType.MSI


def test_get_msi_install_dir_from_registry_returns_none_on_permission_error(monkeypatch: pytest.MonkeyPatch) -> None:
    if not install_type.HAS_WINREG:
        pytest.skip("当前环境无 winreg")

    def raise_permission_error(*args, **kwargs):
        raise PermissionError(5, "Access is denied")

    monkeypatch.setattr(install_type.winreg, "OpenKey", raise_permission_error)

    assert install_type._get_msi_install_dir_from_registry() is None
