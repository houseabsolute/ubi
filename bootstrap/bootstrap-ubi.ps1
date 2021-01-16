## Without -UseBasicParsing I had an error on my test VM about IE not being
## fully installed, and this seems to work fine with this flag.
$DlFileURL = (Invoke-WebRequest -URI "https://github.com/houseabsolute/ubi/releases/latest/" -UseBasicParsing).BaseResponse.ResponseUri.OriginalString

$DlFileName = "ubi-Windows-x86_64.zip"

$DlFileURL = "$DlFileURL" -replace '/tag/','/download/'
$DlFileURL = "$DlFileURL/$DlFileName" 

$TempFile = "$env:TEMP/$DlFileName"

Invoke-WebRequest -URI "$DlFileURL" -OutFile "$TempFile"

Expand-Archive -Path "$TempFile" -DestinationPath "$env:TEMP" -Force

Copy-Item -Path "$env:TEMP/ubi.exe" -Destination "."
