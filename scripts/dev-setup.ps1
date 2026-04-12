# NapCatQQ Desktop 开发环境设置脚本 (PowerShell)
# 用途: 一键设置开发环境

param(
    [switch]$SkipGo,
    [switch]$SkipPython,
    [switch]$Verbose
)

$ErrorActionPreference = "Stop"

function Write-Info($msg) { Write-Host "[INFO] $msg" -ForegroundColor Blue }
function Write-Success($msg) { Write-Host "[SUCCESS] $msg" -ForegroundColor Green }
function Write-Warning($msg) { Write-Host "[WARN] $msg" -ForegroundColor Yellow }
function Write-Error($msg) { Write-Host "[ERROR] $msg" -ForegroundColor Red }

function Test-Command($cmd) {
    return [bool](Get-Command -Name $cmd -ErrorAction SilentlyContinue)
}

Write-Host "========================================"
Write-Host "  NapCatQQ Desktop 开发环境设置"
Write-Host "========================================"
Write-Host ""

# 检查管理员权限
$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    Write-Warning "建议以管理员权限运行此脚本 (用于创建目录等操作)"
}

# 设置项目根目录
$projectRoot = Split-Path -Parent $PSScriptRoot | Split-Path -Parent
Set-Location $projectRoot

Write-Info "项目目录: $projectRoot"

# ==================== Python 环境 ====================
if (-not $SkipPython) {
    Write-Host ""
    Write-Info "设置 Python 环境..."

    # 检查 Python
    if (-not (Test-Command "python")) {
        Write-Error "未找到 Python，请安装 Python 3.12+"
        exit 1
    }

    $pythonVersion = python --version 2>&1
    Write-Info "Python 版本: $pythonVersion"

    # 检查是否为 3.12
    if (-not ($pythonVersion -match "3\.12")) {
        Write-Warning "建议使用 Python 3.12.x 版本"
    }

    # 创建虚拟环境 (如果不存在)
    if (-not (Test-Path ".venv")) {
        Write-Info "创建虚拟环境..."
        python -m venv .venv
        Write-Success "虚拟环境创建完成"
    } else {
        Write-Info "虚拟环境已存在"
    }

    # 激活虚拟环境
    Write-Info "激活虚拟环境..."
    & .\.venv\Scripts\Activate.ps1

    # 升级 pip
    Write-Info "升级 pip..."
    python -m pip install --upgrade pip -q

    # 安装依赖
    if (Test-Path "requirements.txt") {
        Write-Info "安装 Python 依赖..."
        pip install -r requirements.txt -q
        if ($Verbose) { pip install -r requirements.txt }
        Write-Success "Python 依赖安装完成"
    }

    # 安装项目
    Write-Info "安装项目..."
    pip install -e . -q
    Write-Success "项目安装完成"

    # 检查关键依赖
    Write-Info "检查关键依赖..."
    $deps = @("PySide6", "pydantic", "paramiko", "keyring")
    foreach ($dep in $deps) {
        try {
            python -c "import $dep" 2>$null
            Write-Success "$dep 已安装"
        } catch {
            Write-Warning "$dep 可能未正确安装"
        }
    }
}

# ==================== Go 环境 ====================
if (-not $SkipGo) {
    Write-Host ""
    Write-Info "设置 Go 环境..."

    # 检查 Go
    if (-not (Test-Command "go")) {
        Write-Error "未找到 Go，请安装 Go 1.21+"
        Write-Info "下载地址: https://go.dev/dl/"
        exit 1
    }

    $goVersion = go version
    Write-Info "Go 版本: $goVersion"

    # 下载依赖
    Write-Info "下载 Go 依赖..."
    Push-Location src\daemon
    go mod download
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Go 依赖下载失败"
        Pop-Location
        exit 1
    }
    Write-Success "Go 依赖下载完成"

    # 验证构建
    Write-Info "验证 Daemon 构建..."
    go build -o ..\..\bin\napcat-daemon-test.exe .\cmd\daemon 2>$null
    if ($LASTEXITCODE -eq 0) {
        Write-Success "Daemon 构建验证通过"
        Remove-Item ..\..\bin\napcat-daemon-test.exe -ErrorAction SilentlyContinue
    } else {
        Write-Warning "Daemon 构建验证失败，可能需要修复代码"
    }

    Pop-Location
}

# ==================== 目录结构 ====================
Write-Host ""
Write-Info "创建目录结构..."

$dirs = @(
    "bin",
    "logs",
    "data\napcat",
    "test-artifacts"
)

foreach ($dir in $dirs) {
    $path = Join-Path $projectRoot $dir
    if (-not (Test-Path $path)) {
        New-Item -ItemType Directory -Path $path -Force | Out-Null
        Write-Success "创建目录: $dir"
    }
}

# ==================== 配置文件 ====================
Write-Host ""
Write-Info "检查配置文件..."

if (-not (Test-Path "config\daemon.yaml")) {
    Copy-Item "config\daemon.yaml" "config\daemon.yaml" -ErrorAction SilentlyContinue
    Write-Success "创建默认配置文件"
}

# ==================== 完成 ====================
Write-Host ""
Write-Host "========================================"
Write-Success "开发环境设置完成!"
Write-Host "========================================"
Write-Host ""
Write-Host "常用命令:"
Write-Host "  make build          - 构建所有组件"
Write-Host "  make run-desktop    - 运行 Desktop"
Write-Host "  make run-daemon     - 运行 Daemon"
Write-Host "  make test           - 运行测试"
Write-Host ""
Write-Host "IDE 配置:"
Write-Host "  - 设置 Python 解释器: $projectRoot\.venv\Scripts\python.exe"
Write-Host "  - 设置 Go SDK: $(go env GOROOT)"
Write-Host ""
