# vapx installer for Windows (PowerShell 5.1+).
#
#   irm https://raw.githubusercontent.com/Hex29A/vapx/main/install.ps1 | iex
#
# Read it first if you like:
#   irm https://raw.githubusercontent.com/Hex29A/vapx/main/install.ps1
#
# Downloads vapx.exe and SHA256SUMS from the latest GitHub release, verifies the
# checksum, and installs to %LOCALAPPDATA%\Programs\vapx (no admin). That folder
# is added to the user PATH. Running it again updates to the latest release.
#
#   $env:VAPX_VERSION = "0.27.0"; irm ... | iex     install a specific version
#   $env:VAPX_UNINSTALL = "1"; irm ... | iex         remove vapx and its PATH entry

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'   # Invoke-WebRequest is very slow with the progress bar

$Repo = 'Hex29A/vapx'
$Dir = if ($env:VAPX_INSTALL_DIR) { $env:VAPX_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'Programs\vapx' }
$Exe = Join-Path $Dir 'vapx.exe'

function Get-UserPath { [Environment]::GetEnvironmentVariable('Path', 'User') }
function Set-UserPath($p) { [Environment]::SetEnvironmentVariable('Path', $p, 'User') }

if ($env:VAPX_UNINSTALL) {
    if (Test-Path $Exe) { Remove-Item $Exe -Force; Write-Host "Removed $Exe" }
    else { Write-Host "No vapx in $Dir, nothing to remove." }
    if ((Test-Path $Dir) -and -not (Get-ChildItem $Dir)) { Remove-Item $Dir }
    $parts = (Get-UserPath) -split ';' | Where-Object { $_ -and $_ -ne $Dir }
    Set-UserPath ($parts -join ';')
    Write-Host 'Your cameras.yaml is left in place.'
    return
}

$arch = $env:PROCESSOR_ARCHITECTURE
if ($arch -ne 'AMD64') {
    # ARM64 Windows runs x64 binaries under emulation, so that is fine too.
    if ($arch -ne 'ARM64') { throw "No prebuilt vapx for Windows $arch" }
}
$Asset = 'vapx-x86_64-pc-windows-msvc.exe'

if ($env:VAPX_VERSION) {
    $v = $env:VAPX_VERSION.TrimStart('v')
    $Base = "https://github.com/$Repo/releases/download/v$v"
} else {
    $Base = "https://github.com/$Repo/releases/latest/download"
}

$Tmp = Join-Path ([IO.Path]::GetTempPath()) ("vapx-" + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $Tmp | Out-Null
try {
    Write-Host "Downloading $Asset..."
    Invoke-WebRequest -UseBasicParsing "$Base/$Asset" -OutFile "$Tmp\vapx.exe"
    Invoke-WebRequest -UseBasicParsing "$Base/SHA256SUMS" -OutFile "$Tmp\SHA256SUMS"

    $line = Get-Content "$Tmp\SHA256SUMS" | Where-Object { ($_ -split '\s+')[1].TrimStart('*') -eq $Asset }
    if (-not $line) { throw "$Asset is not listed in SHA256SUMS" }
    $want = ($line -split '\s+')[0].ToLower()
    $got = (Get-FileHash "$Tmp\vapx.exe" -Algorithm SHA256).Hash.ToLower()
    if ($want -ne $got) { throw "Checksum mismatch for $Asset (expected $want, got $got)" }

    New-Item -ItemType Directory -Force -Path $Dir | Out-Null
    $old = if (Test-Path $Exe) { & $Exe --version 2>$null } else { $null }
    Move-Item -Force "$Tmp\vapx.exe" $Exe
    $new = & $Exe --version
    if ($old -and $old -ne $new) { Write-Host "Updated $old -> $new in $Dir" }
    else { Write-Host "Installed $new in $Dir (checksum verified)" }
} finally {
    Remove-Item -Recurse -Force $Tmp -ErrorAction SilentlyContinue
}

$userPath = "$(Get-UserPath)"
if (-not (($userPath -split ';') -contains $Dir)) {
    Set-UserPath (($userPath.TrimEnd(';') + ";$Dir").TrimStart(';'))
    Write-Host "Added $Dir to your user PATH. Open a new terminal to use it."
}
Write-Host ''
Write-Host 'Next: vapx config add <name> <ip>   (or vapx --help)'
