#Requires -Version 5.1
param(
  [Parameter(Mandatory = $true)]
  [string]$MsiPath
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path -LiteralPath $MsiPath)) {
  throw "MSI not found: $MsiPath"
}

$ExpectedUpgradeCode = "{5EBD6A58-1B0A-4700-B16F-7E3D7D62949D}"
$ExpectedProductName = "NapCatQQ Desktop"
$ExpectedManufacturer = "Qiao"
# zh-CN LCID; see tauri-bundler languages.json + bundle.windows.wix.language
$ExpectedProductLanguage = "2052"
$MainExeName = "NapCatQQ-Desktop.exe"
$MaxExtractedFiles = 40

function Get-MsiProperties {
  param([string]$Path)
  $full = (Resolve-Path -LiteralPath $Path).Path
  $installer = New-Object -ComObject WindowsInstaller.Installer
  $db = $installer.OpenDatabase($full, 0)
  $view = $db.OpenView("SELECT Property, Value FROM Property")
  $view.Execute() | Out-Null
  $map = @{}
  while ($true) {
    $rec = $view.Fetch()
    if ($null -eq $rec) { break }
    $map[$rec.StringData(1)] = $rec.StringData(2)
  }
  return $map
}

Write-Host ("Assert MSI: " + $MsiPath)
$props = Get-MsiProperties -Path $MsiPath

$upgrade = $props["UpgradeCode"]
$product = $props["ProductName"]
$productLanguage = $props["ProductLanguage"]

Write-Host ("  ProductName     = " + $product)
Write-Host ("  Manufacturer    = " + $manufacturer)
Write-Host ("  ProductVersion  = " + $version)
Write-Host ("  ProductLanguage = " + $productLanguage)
Write-Host ("  UpgradeCode     = " + $upgrade)
Write-Host ("  ProductCode     = " + $props["ProductCode"])

if ($upgrade -ne $ExpectedUpgradeCode) {
  throw ("UpgradeCode mismatch: got '" + $upgrade + "', expected '" + $ExpectedUpgradeCode + "'")
}
if ($product -ne $ExpectedProductName) {
  throw ("ProductName mismatch: got '" + $product + "', expected '" + $ExpectedProductName + "'")
}
if ($manufacturer -ne $ExpectedManufacturer) {
  throw ("Manufacturer mismatch: got '" + $manufacturer + "', expected '" + $ExpectedManufacturer + "'")
}
if ($productLanguage -ne $ExpectedProductLanguage) {
  throw ("ProductLanguage mismatch: got '" + $productLanguage + "', expected '" + $ExpectedProductLanguage + "' (zh-CN)
if ($manufacturer -ne $ExpectedManufacturer) {
  throw ("Manufacturer mismatch: got '" + $manufacturer + "', expected '" + $ExpectedManufacturer + "'")
}

$work = Join-Path ([System.IO.Path]::GetTempPath()) ("ncd-msi-assert-" + [guid]::NewGuid().ToString("n"))
New-Item -ItemType Directory -Path $work -Force | Out-Null
try {
  $extractRoot = Join-Path $work "extract"
  New-Item -ItemType Directory -Path $extractRoot -Force | Out-Null
  $log = Join-Path $work "msiexec.log"
  $msiFull = (Resolve-Path -LiteralPath $MsiPath).Path
  $args = @(
    "/a",
    ('"' + $msiFull + '"'),
    "/qn",
    ("TARGETDIR=" + '"' + $extractRoot + '"'),
    "/l*v",
    ('"' + $log + '"')
  )
  $p = Start-Process -FilePath "msiexec.exe" -ArgumentList $args -Wait -PassThru
  if ($p.ExitCode -ne 0) {
    throw ("msiexec /a failed exit=" + $p.ExitCode + "; log=" + $log)
  }

  $appDir = Get-ChildItem -Path $extractRoot -Recurse -Directory -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -eq $ExpectedProductName } |
    Select-Object -First 1

  if (-not $appDir) {
    $candidates = Get-ChildItem -Path $extractRoot -Recurse -File -Filter $MainExeName -ErrorAction SilentlyContinue
    if ($candidates) {
      $appDir = $candidates[0].Directory
    }
  }
  if (-not $appDir) {
    throw "Could not locate install app directory under admin extract"
  }

  Write-Host ("  Extract app dir = " + $appDir.FullName)
  $files = @(Get-ChildItem -Path $appDir.FullName -Recurse -File -ErrorAction SilentlyContinue)
  $count = $files.Count
  Write-Host ("  Extracted files = " + $count)

  $top = $files | Sort-Object Length -Descending | Select-Object -First 10
  foreach ($f in $top) {
    $rel = $f.FullName.Substring($appDir.FullName.Length + 1)
    $kb = [math]::Round($f.Length / 1KB, 1)
    Write-Host ("    " + $kb + " KB  " + $rel)
  }

  $exe = Join-Path $appDir.FullName $MainExeName
  if (-not (Test-Path -LiteralPath $exe)) {
    throw ("Missing main exe: " + $exe)
  }
  $internal = Join-Path $appDir.FullName "_internal"
  if (Test-Path -LiteralPath $internal) {
    throw ("V3 MSI must not ship PyInstaller _internal directory: " + $internal)
  }
  $icons = Join-Path $appDir.FullName "icons"
  if (Test-Path -LiteralPath $icons) {
    throw ("V3 MSI must not ship icons resource directory (tray/window icons are embedded): " + $icons)
  }
  if ($count -gt $MaxExtractedFiles) {
    throw ("Extracted file count " + $count + " exceeds V3 budget " + $MaxExtractedFiles + " (looks like onedir regression)")
  }

  Write-Host "MSI layout assertions passed."
}
finally {
  Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
}
