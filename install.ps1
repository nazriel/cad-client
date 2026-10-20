# Install cad-cli on Windows from a GitHub release.
#
#   irm https://raw.githubusercontent.com/nazriel/cad-client/main/install.ps1 | iex
#   & ([scriptblock]::Create((irm https://raw.githubusercontent.com/nazriel/cad-client/main/install.ps1))) -Version v0.1.0
#
# Parameters (or the matching environment variables):
#   -Version TAG       release to install (CAD_CLI_VERSION, default: latest)
#   -BinDir DIR        where cad-cli.exe goes (CAD_CLI_BIN_DIR, default: ~\.local\bin)
#   -NoModifyPath      do not add BinDir to the user PATH
#
# CAD_CLI_DOWNLOAD_URL replaces the release download URL (a mirror, or a local
# server in CI); it must serve the archive and SHA256SUMS for the version.
param(
    [string]$Version = $(if ($env:CAD_CLI_VERSION) { $env:CAD_CLI_VERSION } else { 'latest' }),
    [string]$BinDir = $(if ($env:CAD_CLI_BIN_DIR) { $env:CAD_CLI_BIN_DIR } else { Join-Path $HOME '.local\bin' }),
    [switch]$NoModifyPath
)

$ErrorActionPreference = 'Stop'
# Windows PowerShell renders download progress slowly enough to dominate the install.
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

$repo = 'nazriel/cad-client'

function Say([string]$Message) { Write-Host "cad-cli-install: $Message" }
function Fail([string]$Message) { throw "cad-cli-install: error: $Message" }

if ($PSVersionTable.PSEdition -eq 'Core' -and -not $IsWindows) {
    Fail 'install.ps1 is for Windows; use install.sh on macOS and Linux'
}

$arch = $null
try {
    $arch = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
} catch {
    $arch = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
}
switch -Regex ($arch) {
    '^(X64|AMD64)$' { $target = 'x86_64-pc-windows-msvc' }
    '^(Arm64|ARM64)$' { $target = 'aarch64-pc-windows-msvc' }
    default { Fail "unsupported architecture: $arch" }
}

if ($Version -eq 'latest') {
    try {
        $Version = (Invoke-RestMethod -UseBasicParsing "https://api.github.com/repos/$repo/releases/latest").tag_name
    } catch {
        Fail "could not resolve the latest release: $($_.Exception.Message)"
    }
}

$archive = "cad-cli-$Version-$target"
$base = if ($env:CAD_CLI_DOWNLOAD_URL) { $env:CAD_CLI_DOWNLOAD_URL } else { "https://github.com/$repo/releases/download/$Version" }
$tmp = Join-Path ([IO.Path]::GetTempPath()) ("cad-cli-install-" + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $tmp | Out-Null

try {
    $zip = Join-Path $tmp "$archive.zip"
    $sums = Join-Path $tmp 'SHA256SUMS'
    Say "downloading $archive.zip"
    try {
        Invoke-WebRequest -UseBasicParsing -Uri "$base/$archive.zip" -OutFile $zip
        Invoke-WebRequest -UseBasicParsing -Uri "$base/SHA256SUMS" -OutFile $sums
    } catch {
        Fail "download failed from ${base}: $($_.Exception.Message)"
    }

    $expected = $null
    foreach ($line in Get-Content -LiteralPath $sums) {
        $fields = $line -split '\s+', 2
        if ($fields.Count -eq 2 -and $fields[1].TrimStart('*') -eq "$archive.zip") { $expected = $fields[0] }
    }
    if (-not $expected) { Fail "$archive.zip is not listed in SHA256SUMS" }
    $actual = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash
    if ($actual -ne $expected) { Fail "checksum mismatch for $archive.zip" }

    Expand-Archive -LiteralPath $zip -DestinationPath $tmp -Force
    $src = Join-Path $tmp $archive

    New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
    $exe = Join-Path $BinDir 'cad-cli.exe'
    # A running cad-cli.exe cannot be overwritten, but it can be renamed.
    if (Test-Path -LiteralPath $exe) { Move-Item -LiteralPath $exe -Destination "$exe.old" -Force }
    Copy-Item -LiteralPath (Join-Path $src 'cad-cli.exe') -Destination $exe
    Remove-Item -LiteralPath "$exe.old" -Force -ErrorAction SilentlyContinue
    Say "installed $exe ($Version)"

    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $entries = @($userPath -split ';' | Where-Object { $_ })
    if ($entries -notcontains $BinDir) {
        if ($NoModifyPath) {
            Say "note: $BinDir is not on PATH"
        } else {
            [Environment]::SetEnvironmentVariable('Path', (($entries + $BinDir) -join ';'), 'User')
            $env:Path = "$env:Path;$BinDir"
            Say "added $BinDir to the user PATH; open a new terminal to use it"
        }
    }

    Say 'next: set CAD_API_KEY and CAD_API_SECRET, then run `cad-cli completions powershell` for shell completion'
} finally {
    Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
}
