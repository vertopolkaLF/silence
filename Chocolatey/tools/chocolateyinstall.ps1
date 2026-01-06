$ErrorActionPreference = 'Stop'
$toolsDir   = "$(Split-Path -parent $MyInvocation.MyCommand.Definition)"

$packageArgs = @{
  packageName   = $env:ChocolateyPackageName
  fileType      = 'exe'
  url           = 'https://github.com/vertopolkaLF/silence/releases/download/v1.4.1/silence-v1.4.1-x86-setup.exe'
  url64bit      = 'https://github.com/vertopolkaLF/silence/releases/download/v1.4.1/silence-v1.4.1-x64-setup.exe'
  
  softwareName  = 'silence!*'
  
  checksum      = 'E241BD73443955A2338A37AFB63AF9DBC1515CEEB61A2A2C20648A7023DA1A00'
  checksumType  = 'sha256'
  checksum64    = '6D926E3C4AB5CC4F7A5F0988F673360A33654A24318349545EEFA184383EB2A5'
  checksumType64= 'sha256'
  
  silentArgs    = '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP-'
  validExitCodes= @(0)
}

Install-ChocolateyPackage @packageArgs
