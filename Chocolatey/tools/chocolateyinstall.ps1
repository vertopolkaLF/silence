$ErrorActionPreference = 'Stop'
$toolsDir   = "$(Split-Path -parent $MyInvocation.MyCommand.Definition)"

$packageArgs = @{
  packageName   = $env:ChocolateyPackageName
  fileType      = 'exe'
  url           = 'https://github.com/vertopolkaLF/silence/releases/download/v1.5/silence-v1.5-x86-setup.exe'
  url64bit      = 'https://github.com/vertopolkaLF/silence/releases/download/v1.5/silence-v1.5-x64-setup.exe'
  
  softwareName  = 'silence!*'
  
  checksum      = 'CB67DB2F5FF0B7988E4ADA5FA64CA9820C8A99D284A49FA90EA3E752325178FC'
  checksumType  = 'sha256'
  checksum64    = 'AC9825D7C56CADF40E770D1C104522F70B79D099363A5B324EE9FC690E7A96B1'
  checksumType64= 'sha256'
  
  silentArgs    = '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP-'
  validExitCodes= @(0)
}

Install-ChocolateyPackage @packageArgs
