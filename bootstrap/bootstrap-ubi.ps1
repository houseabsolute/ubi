$Filename = "ubi-Windows-msvc-x86_64.zip"
$URL = "https://github.com/houseabsolute/ubi/releases/latest/download/$($Filename)"
# Each run gets its own directory, so we never install a ubi.exe left behind by an earlier run.
$TempDir = Join-Path ([System.IO.Path]::GetTempPath()) ([System.IO.Path]::GetRandomFileName())
New-Item -ItemType Directory -Path "$TempDir" -ErrorAction Stop | Out-Null
# We use `-ErrorAction Stop` and not `$ErrorActionPreference`, because the README runs this script
# with `Invoke-Expression`, which would change the setting in the user's own shell. We use `throw`
# and not `exit` for the same reason, since `exit` would close the user's shell.
try {
    $TempFile = Join-Path "$TempDir" "$Filename"
    Invoke-WebRequest -URI "$URL" -OutFile "$TempFile" -ErrorAction Stop
    Expand-Archive -Path "$TempFile" -DestinationPath "$TempDir" -Force -ErrorAction Stop
    Copy-Item -Path (Join-Path "$TempDir" "ubi.exe") -Destination "." -ErrorAction Stop
}
catch {
    # Some of these errors would only stop the one command that failed. A `throw` stops the script.
    throw $_
}
finally {
    Remove-Item -Recurse -Force -Path "$TempDir" -ErrorAction SilentlyContinue
}
