# This runs bootstrap-ubi.ps1 with a fake `Invoke-WebRequest`, so we can test what happens when the
# download fails, and check what the script leaves behind in `$env:TEMP`. We run the script both as
# a file and through `Invoke-Expression`, which is how the README says to run it.
#
# A failed download must not install anything. In particular, it must not install a `ubi.exe` left
# in `$env:TEMP` by an earlier run.

$ErrorActionPreference = "Stop"
# The child process fails on purpose, and we want its exit code, not an error.
$PSNativeCommandUseErrorActionPreference = $false

$Script = (Resolve-Path (Join-Path $PSScriptRoot "../bootstrap-ubi.ps1")).Path
$Filename = "ubi-Windows-msvc-x86_64.zip"
$Failed = $false

# The bootstrap script runs in its own PowerShell process. Inside a `try` block, an error that
# should only stop one statement stops the whole script instead, and `$ErrorActionPreference` would
# leak in from this script. Either one would hide what happens in a user's shell.
$Child = @'
function Main {
    if ($env:BOOTSTRAP_MODE -eq "file") {
        & $env:BOOTSTRAP_SCRIPT
    }
    else {
        Get-Content -Raw $env:BOOTSTRAP_SCRIPT | Invoke-Expression
    }
}

# This takes the place of the real cmdlet while the script runs, because PowerShell looks up
# functions before cmdlets. Like the real one, it fails with an error that only stops the current
# statement unless the caller asks for more.
function Invoke-WebRequest {
    [CmdletBinding()]
    param($URI, $OutFile, [switch]$UseBasicParsing)

    if ($env:FAKE_DOWNLOAD -eq "fail") {
        $Err = [System.Management.Automation.ErrorRecord]::new(
            [System.Net.WebException]::new("Unable to connect to the remote server"),
            "FakeWebRequestFailed",
            [System.Management.Automation.ErrorCategory]::ConnectionError,
            $URI
        )
        $PSCmdlet.ThrowTerminatingError($Err)
    }

    $Src = Join-Path $env:FAKE_ROOT "new-release"
    New-Item -ItemType Directory $Src | Out-Null
    Set-Content -Path (Join-Path $Src "ubi.exe") -Value "new ubi"
    Set-Content -Path (Join-Path $Src "README.md") -Value "fake readme"
    Compress-Archive -Path (Join-Path $Src "*") -DestinationPath $OutFile -Force
    Remove-Item -Recurse -Force $Src
}

Main
'@

function Main {
    foreach ($Mode in "file", "expression") {
        Test-FailedDownload $Mode
        Test-GoodDownload $Mode
    }

    if ($script:Failed) {
        exit 1
    }
    Write-Host "bootstrap-ubi.ps1 handled a failed download and cleaned up after itself"
}

function Test-FailedDownload($Mode) {
    $Dirs = New-Dirs
    # These are what an earlier run of the old script left behind.
    $Src = Join-Path $Dirs.Root "old-release"
    New-Item -ItemType Directory $Src | Out-Null
    Set-Content -Path (Join-Path $Src "ubi.exe") -Value "old ubi"
    Compress-Archive -Path (Join-Path $Src "*") -DestinationPath (Join-Path $Dirs.Temp $Filename)
    Copy-Item -Path (Join-Path $Src "ubi.exe") -Destination $Dirs.Temp

    $Status = Invoke-Bootstrap $Mode "fail" $Dirs

    if ($Status -eq 0) {
        Write-Fail "With a failed download, running bootstrap-ubi.ps1 as $Mode exited with 0"
    }
    if (Test-Path (Join-Path $Dirs.Target "ubi.exe")) {
        Write-Fail "With a failed download, running bootstrap-ubi.ps1 as $Mode installed ubi.exe"
    }
    $Left = @(Get-ChildItem -Force -Name $Dirs.Temp)
    $New = @($Left | Where-Object { $_ -notin @($Filename, "ubi.exe") })
    if ($New.Count -ne 0) {
        $Names = $Left -join ", "
        Write-Fail ("With a failed download, running bootstrap-ubi.ps1 as $Mode left these in" +
            " `$env:TEMP: $Names")
    }

    Remove-Item -Recurse -Force $Dirs.Root
}

function Test-GoodDownload($Mode) {
    $Dirs = New-Dirs

    $Status = Invoke-Bootstrap $Mode "good" $Dirs

    if ($Status -ne 0) {
        Write-Fail "With a good download, running bootstrap-ubi.ps1 as $Mode exited with $Status"
    }
    $Installed = Join-Path $Dirs.Target "ubi.exe"
    if (-not (Test-Path $Installed) -or (Get-Content -Raw $Installed).Trim() -ne "new ubi") {
        Write-Fail ("With a good download, running bootstrap-ubi.ps1 as $Mode did not install" +
            " ubi.exe")
    }
    $Left = @(Get-ChildItem -Force -Name $Dirs.Temp)
    if ($Left.Count -ne 0) {
        $Names = $Left -join ", "
        Write-Fail "Running bootstrap-ubi.ps1 as $Mode left these in `$env:TEMP: $Names"
    }

    Remove-Item -Recurse -Force $Dirs.Root
}

# Returns the exit code of the PowerShell process that ran the script.
function Invoke-Bootstrap($Mode, $FakeDownload, $Dirs) {
    $ChildFile = Join-Path $Dirs.Root "child.ps1"
    Set-Content -Path $ChildFile -Value $script:Child

    $Saved = @{}
    $Vars = @{
        TEMP             = $Dirs.Temp
        TMP              = $Dirs.Temp
        BOOTSTRAP_MODE   = $Mode
        BOOTSTRAP_SCRIPT = $script:Script
        FAKE_DOWNLOAD    = $FakeDownload
        FAKE_ROOT        = $Dirs.Root
    }
    foreach ($Name in $Vars.Keys) {
        $Saved[$Name] = [Environment]::GetEnvironmentVariable($Name)
        [Environment]::SetEnvironmentVariable($Name, $Vars[$Name])
    }

    Push-Location $Dirs.Target
    try {
        $Exe = (Get-Process -Id $PID).Path
        # Without `Out-Host`, any output from the child would end up in our return value.
        & $Exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $ChildFile | Out-Host
        return $LASTEXITCODE
    }
    finally {
        Pop-Location
        foreach ($Name in $Saved.Keys) {
            [Environment]::SetEnvironmentVariable($Name, $Saved[$Name])
        }
    }
}

function New-Dirs {
    $Root = Join-Path ([System.IO.Path]::GetTempPath()) ([System.IO.Path]::GetRandomFileName())
    $Dirs = @{
        Root   = $Root
        Temp   = Join-Path $Root "temp"
        Target = Join-Path $Root "target"
    }
    New-Item -ItemType Directory $Dirs.Temp, $Dirs.Target | Out-Null
    return $Dirs
}

function Write-Fail($Message) {
    Write-Host $Message
    $script:Failed = $true
}

Main
