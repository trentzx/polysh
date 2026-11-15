# Install polysh on Windows:
#   irm https://raw.githubusercontent.com/trentzx/polysh/main/install.ps1 | iex
#
# Set $env:POLYSH_INSTALL_DIR to choose where it goes
# (default: %LOCALAPPDATA%\polysh\bin, added to your user PATH).
# Set $env:POLYSH_NO_MODIFY_PATH = 1 to leave PATH alone.

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

$repo = 'trentzx/polysh'
$dir = if ($env:POLYSH_INSTALL_DIR) { $env:POLYSH_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'polysh\bin' }

$tag = (Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest").tag_name
$name = "polysh-$tag-x86_64-pc-windows-msvc"
$url = "https://github.com/$repo/releases/download/$tag/$name.zip"
$tmp = Join-Path ([IO.Path]::GetTempPath()) ("polysh-" + [guid]::NewGuid())
New-Item -ItemType Directory $tmp | Out-Null

try {
    Write-Host "Downloading polysh $tag..."
    $zip = Join-Path $tmp "$name.zip"
    Invoke-WebRequest $url -OutFile $zip -UseBasicParsing
    Invoke-WebRequest "$url.sha256" -OutFile "$zip.sha256" -UseBasicParsing

    $expected = ((Get-Content "$zip.sha256" -Raw).Trim() -split '\s+')[0]
    $actual = (Get-FileHash $zip -Algorithm SHA256).Hash
    if ($actual -ne $expected) { throw "Checksum mismatch; the download may be corrupted." }

    Expand-Archive $zip -DestinationPath $tmp -Force
    New-Item -ItemType Directory -Force $dir | Out-Null
    Copy-Item (Join-Path $tmp "$name\polysh.exe") $dir -Force
}
finally {
    Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
}

$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if ($env:POLYSH_NO_MODIFY_PATH) {
    Write-Host "Skipped PATH changes; polysh.exe is in $dir"
}
elseif (-not (($userPath -split ';') -contains $dir)) {
    $newPath = if ($userPath) { "$userPath;$dir" } else { $dir }
    [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
    $env:Path += ";$dir"
    Write-Host "Added $dir to your PATH. Open a new terminal to use it everywhere."
}

Write-Host "Installed polysh $tag. Run: polysh"
