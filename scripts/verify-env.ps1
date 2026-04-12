# NapCatQQ Desktop 环境验证脚本
# 用途: 验证开发/生产环境是否配置正确

param(
    [switch]$Detailed,
    [switch]$Fix
)

$ErrorActionPreference = "Continue"
$global:Errors = 0
$global:Warnings = 0

function Write-Check($name, $status, $message = "") {
    $symbol = if ($status -eq "OK") { "✅" } elseif ($status -eq "WARN") { "⚠️" } else { "❌" }
    $color = if ($status -eq "OK") { "Green" } elseif ($status -eq "WARN") { "Yellow" } else { "Red" }
    Write-Host "$symbol $name" -ForegroundColor $color -NoNewline
    if ($message) {
        Write-Host " - $message" -ForegroundColor Gray
    } else {
        Write-Host ""
    }
    if ($status -eq "ERROR") { $global:Errors++ }
    if ($status -eq "WARN") { $global:Warnings++ }
}

function Test-Command($cmd) {
    return [bool](Get-Command -Name $cmd -ErrorAction SilentlyContinue)
}

Write-Host "========================================"
Write-Host "  NapCatQQ Desktop 环境验证"
Write-Host "========================================"
Write-Host ""

$projectRoot = Split-Path -Parent $PSScriptRoot | Split-Path -Parent
Set-Location $projectRoot

# ==================== 基础工具 ====================
Write-Host "【基础工具检查】" -ForegroundColor Cyan

if (Test-Command "python") {
    $version = python --version 2>&1
    Write-Check "Python" "OK" $version
} else {
    Write-Check "Python" "ERROR" "未安装"
}

if (Test-Command "go") {
    $version = go version
    Write-Check "Go" "OK" $version
} else {
    Write-Check "Go" "ERROR" "未安装"
}

if (Test-Command "git") {
    Write-Check "Git" "OK"
} else {
    Write-Check "Git" "WARN" "建议安装以便版本管理"
}

# ==================== Python 环境 ====================
Write-Host ""
Write-Host "【Python 环境检查】" -ForegroundColor Cyan

if (Test-Path ".venv") {
    Write-Check "虚拟环境" "OK" ".venv 存在"
} else {
    Write-Check "虚拟环境" "WARN" "未创建，运行 scripts/dev-setup.ps1"
}

$pythonDeps = @{
    "PySide6" = "GUI 框架"
    "pydantic" = "数据验证"
    "paramiko" = "SSH 客户端"
    "httpx" = "HTTP 客户端"
    "keyring" = "密钥存储"
}

foreach ($dep in $pythonDeps.GetEnumerator()) {
    try {
        python -c "import $($dep.Key)" 2>$null
        Write-Check $dep.Key "OK" $dep.Value
    } catch {
        Write-Check $dep.Key "ERROR" "$($dep.Value) - 未安装"
    }
}

# ==================== Go 环境 ====================
Write-Host ""
Write-Host "【Go 环境检查】" -ForegroundColor Cyan

Push-Location src\daemon

# 检查 go.mod
try {
    $modContent = Get-Content "go.mod" -Raw
    if ($modContent -match "gorilla/websocket" -and $modContent -match "yaml.v3") {
        Write-Check "go.mod" "OK" "依赖配置完整"
    } else {
        Write-Check "go.mod" "WARN" "依赖可能不完整"
    }
} catch {
    Write-Check "go.mod" "ERROR" "文件不存在"
}

# 检查能否下载依赖
$env:GOPROXY = "https://proxy.golang.org,direct"
$result = go mod verify 2>&1
if ($LASTEXITCODE -eq 0) {
    Write-Check "Go 依赖" "OK" "已下载"
} else {
    Write-Check "Go 依赖" "WARN" "需要下载: go mod download"
}

Pop-Location

# ==================== 项目结构 ====================
Write-Host ""
Write-Host "【项目结构检查】" -ForegroundColor Cyan

$requiredDirs = @(
    @("src\desktop\core", "Desktop 核心代码"),
    @("src\daemon\cmd", "Daemon 入口"),
    @("src\daemon\pkg\jsonrpc", "JSON-RPC 协议"),
    @("src\daemon\pkg\security", "安全库"),
    @("config", "配置文件"),
    @("docs\general", "文档")
)

foreach ($dirInfo in $requiredDirs) {
    $dir = $dirInfo[0]
    $desc = $dirInfo[1]
    if (Test-Path $dir) {
        Write-Check $desc "OK"
    } else {
        Write-Check $desc "ERROR" "目录缺失: $dir"
    }
}

# ==================== 关键文件 ====================
Write-Host ""
Write-Host "【关键文件检查】" -ForegroundColor Cyan

$requiredFiles = @(
    @("main.py", "程序入口"),
    @("pyproject.toml", "Python 项目配置"),
    @("requirements.txt", "依赖列表"),
    @("Makefile", "构建脚本"),
    @("src\daemon\go.mod", "Go 模块配置"),
    @("config\daemon.yaml", "Daemon 配置模板")
)

foreach ($fileInfo in $requiredFiles) {
    $file = $fileInfo[0]
    $desc = $fileInfo[1]
    if (Test-Path $file) {
        Write-Check $desc "OK"
    } else {
        Write-Check $desc "ERROR" "文件缺失: $file"
    }
}

# ==================== 语法检查 ====================
Write-Host ""
Write-Host "【语法检查】" -ForegroundColor Cyan

# Python 语法
$pyFiles = Get-ChildItem -Path "src\desktop" -Filter "*.py" -Recurse | Select-Object -First 20
$pyErrors = 0
foreach ($file in $pyFiles) {
    $result = python -m py_compile $file.FullName 2>&1
    if ($LASTEXITCODE -ne 0) {
        $pyErrors++
        if ($Detailed) {
            Write-Check "$($file.Name)" "ERROR" "语法错误"
        }
    }
}
if ($pyErrors -eq 0) {
    Write-Check "Python 语法" "OK" "已检查 $($pyFiles.Count) 个文件"
} else {
    Write-Check "Python 语法" "ERROR" "$pyErrors 个文件有语法错误"
}

# Go 语法
Push-Location src\daemon
$result = go build -o ..\..\bin\syntax-check.exe ./cmd/daemon 2>&1
if ($LASTEXITCODE -eq 0) {
    Write-Check "Go 语法" "OK" "可正常构建"
    Remove-Item ..\..\bin\syntax-check.exe -ErrorAction SilentlyContinue
} else {
    Write-Check "Go 语法" "ERROR" "构建失败"
}
Pop-Location

# ==================== 总结 ====================
Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "验证完成" -ForegroundColor Cyan
Write-Host "========================================"
Write-Host ""

if ($global:Errors -eq 0 -and $global:Warnings -eq 0) {
    Write-Host "✅ 环境验证通过！所有检查项正常。" -ForegroundColor Green
    exit 0
} elseif ($global:Errors -eq 0) {
    Write-Host "⚠️ 环境基本可用，但有 $($global:Warnings) 个警告。" -ForegroundColor Yellow
    exit 0
} else {
    Write-Host "❌ 环境检查失败，发现 $($global:Errors) 个错误，$($global:Warnings) 个警告。" -ForegroundColor Red
    Write-Host ""
    Write-Host "建议操作:" -ForegroundColor Yellow
    Write-Host "  1. 运行: .\scripts\dev-setup.ps1" -ForegroundColor Yellow
    Write-Host "  2. 检查: make health-check" -ForegroundColor Yellow
    exit 1
}
