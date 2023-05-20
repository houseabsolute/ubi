$Filename = "ubi-Windows-x86_64.zip"

$URL = "https://github.com/houseabsolute/ubi/releases/latest/download/$($Filename)"

$TempFile = "$env:TEMP/$Filename"

Invoke-WebRequest -URI "$URL" -OutFile "$TempFile"

Expand-Archive -Path "$TempFile" -DestinationPath "$env:TEMP" -Force

Copy-Item -Path "$env:TEMP/ubi.exe" -Destination "."
