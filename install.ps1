# Install yp, the yourprompt command line, from a GitHub release.
#
#   irm https://raw.githubusercontent.com/LiU-SKY/yourprompt/main/install.ps1 | iex
#
# Settings come from the environment:
#
#   $env:YP_VERSION = 'v0.1.1'; irm .../install.ps1 | iex
#
#   YP_VERSION         the release to install (default: the latest)
#   YP_INSTALL_DIR     where to put yp.exe (default: %LOCALAPPDATA%\Programs\yourprompt)
#   YP_NO_MODIFY_PATH  set to 1 to leave the user PATH alone
#   YP_DOWNLOAD_BASE   where the releases are (default: this repository's on
#                      GitHub). For a mirror, and for testing this script.
#
# Windows PowerShell 5.1 and PowerShell 7. On Linux and macOS, use install.sh.
#
# Everything happens inside one script block, so nothing set here leaks into
# the session that ran it. Failures throw rather than exit, because under iex
# an exit would close that session's window.

& {
    $ErrorActionPreference = 'Stop'
    # Windows PowerShell redraws its progress bar for every chunk it
    # downloads, which can make a download many times slower than the network.
    $ProgressPreference = 'SilentlyContinue'
    # Older Windows PowerShell does not offer TLS 1.2 unless asked to, and
    # GitHub accepts nothing older.
    [Net.ServicePointManager]::SecurityProtocol =
        [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

    $repo = 'LiU-SKY/yourprompt'

    function Fail([string] $message) {
        throw "yp installer: $message"
    }

    function Save-Url([string] $url, [string] $file) {
        try {
            Invoke-WebRequest -Uri $url -OutFile $file -UseBasicParsing
        } catch {
            Fail "could not download ${url}: $($_.Exception.Message)"
        }
    }

    # True if $dir is an entry of the PATH-style $list, however it is spelled
    # there: through a %VARIABLE%, with a trailing backslash, in any case.
    function Test-PathEntry([string] $list, [string] $dir) {
        foreach ($entry in $list -split ';') {
            if ($entry -and [Environment]::ExpandEnvironmentVariables($entry).TrimEnd('\') -eq $dir.TrimEnd('\')) {
                return $true
            }
        }
        $false
    }

    # Appends $dir to the user PATH, unless it is there already, and says
    # whether it did. The value is read and written raw: going through
    # [Environment] would expand every %VARIABLE% in the user's PATH and store
    # the result as a plain string, quietly flattening entries that were
    # written to follow a variable.
    function Add-UserPath([string] $dir) {
        $key = 'HKCU:\Environment'
        $path = (Get-Item -LiteralPath $key).GetValue('Path', '', 'DoNotExpandEnvironmentNames')
        if (Test-PathEntry $path $dir) {
            return $false
        }
        if ($path) {
            $path = $path.TrimEnd(';') + ';' + $dir
        } else {
            $path = $dir
        }
        Set-ItemProperty -LiteralPath $key -Name Path -Value $path -Type ExpandString
        $true
    }

    if ($PSVersionTable.PSVersion.Major -ge 6 -and -not $IsWindows) {
        Fail "this script is for Windows. On Linux and macOS, run: curl -fsSL https://raw.githubusercontent.com/$repo/main/install.sh | sh"
    }
    # A 32-bit PowerShell on 64-bit Windows sees x86 in PROCESSOR_ARCHITECTURE,
    # and the machine's own architecture in PROCESSOR_ARCHITEW6432.
    $arch = $env:PROCESSOR_ARCHITEW6432
    if (-not $arch) {
        $arch = $env:PROCESSOR_ARCHITECTURE
    }
    switch ($arch) {
        'AMD64' { $target = 'x86_64-pc-windows-msvc' }
        'ARM64' { $target = 'aarch64-pc-windows-msvc' }
        default {
            Fail "there is no prebuilt yp for $arch Windows. Build it from source instead: cargo install --git https://github.com/$repo yp-cli"
        }
    }

    $base = $env:YP_DOWNLOAD_BASE
    if (-not $base) {
        $base = "https://github.com/$repo/releases"
    }
    $base = $base.TrimEnd('/')
    $tag = $env:YP_VERSION
    if (-not $tag -or $tag -eq 'latest') {
        $url = "$base/latest/download"
        $release = 'the latest release'
    } else {
        if (-not $tag.StartsWith('v')) {
            $tag = "v$tag"
        }
        $url = "$base/download/$tag"
        $release = $tag
    }
    $asset = "yp-$target.zip"

    $dir = $env:YP_INSTALL_DIR
    if (-not $dir) {
        $dir = Join-Path $env:LOCALAPPDATA 'Programs\yourprompt'
    }
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    $dir = (Get-Item -LiteralPath $dir).FullName.TrimEnd('\')
    $exe = Join-Path $dir 'yp.exe'

    $tmp = Join-Path ([IO.Path]::GetTempPath()) ('yp-install-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $tmp | Out-Null
    try {
        Write-Host "downloading $asset from $release"
        $zip = Join-Path $tmp $asset
        Save-Url "$url/$asset" $zip
        Save-Url "$url/$asset.sha256" "$zip.sha256"

        $expected = ([IO.File]::ReadAllText("$zip.sha256").Trim() -split '\s+')[0].ToLowerInvariant()
        if ($expected -notmatch '^[0-9a-f]{64}$') {
            Fail "$asset.sha256 does not hold a SHA-256 checksum. Nothing was installed."
        }
        $actual = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($actual -ne $expected) {
            Fail "checksum mismatch for ${asset}: the release says $expected, the download is $actual. Nothing was installed."
        }
        Write-Host 'checksum verified'

        $unpacked = Join-Path $tmp 'unpacked'
        Expand-Archive -LiteralPath $zip -DestinationPath $unpacked
        $new = Join-Path $unpacked 'yp.exe'
        if (-not (Test-Path -LiteralPath $new -PathType Leaf)) {
            Fail "$asset does not contain yp.exe. Nothing was installed."
        }

        # Windows will not write over a program while it runs, and Claude Code
        # may be running yp for the status line at any moment. It will rename
        # one, though, so the old binary steps aside for the new one, and is
        # deleted if it has stopped or left for the next install if not.
        Copy-Item -LiteralPath $new -Destination "$exe.new" -Force
        if (Test-Path -LiteralPath $exe) {
            Move-Item -LiteralPath $exe -Destination "$exe.old" -Force
        }
        Move-Item -LiteralPath "$exe.new" -Destination $exe
        Remove-Item -LiteralPath "$exe.old" -Force -ErrorAction SilentlyContinue
    } finally {
        Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
    }

    try {
        $installed = & $exe --version
    } catch {
        Fail "installed $exe, but it does not run on this machine: $($_.Exception.Message)"
    }
    Write-Host "installed $installed to $exe"
    Write-Host ''

    $run = 'yp'
    if (Test-PathEntry $env:Path $dir) {
        Write-Host "$dir is on your PATH."
    } elseif ($env:YP_NO_MODIFY_PATH) {
        Write-Host "$dir is not on your PATH, and YP_NO_MODIFY_PATH is set, so it stays that way."
        $run = "& `"$exe`""
    } else {
        if (Add-UserPath $dir) {
            # Add-UserPath writes the registry directly, which tells no one.
            # Setting any user variable through [Environment] broadcasts the
            # change, so terminals opened from now on start with the new PATH.
            $flag = 'YP_INSTALL_' + [guid]::NewGuid().ToString('N')
            [Environment]::SetEnvironmentVariable($flag, '1', 'User')
            [Environment]::SetEnvironmentVariable($flag, [NullString]::Value, 'User')
            Write-Host "added $dir to your user PATH."
        } else {
            Write-Host "$dir is on your user PATH."
        }
        Write-Host 'This terminal can use it now; terminals that were already open need restarting.'
        $env:Path = "$env:Path;$dir"
    }
    $found = Get-Command yp -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($run -eq 'yp' -and $found -and $found.Path -ne $exe) {
        Write-Host "But $($found.Path) comes before it, so 'yp' runs that one. Remove it, or move $dir ahead of it."
        $run = "& `"$exe`""
    }

    Write-Host ''
    Write-Host 'Next, register the hook and the status line with Claude Code:'
    Write-Host ''
    Write-Host "    $run install"
}
