$ErrorActionPreference = 'Stop'
$toolsDir   = "$(Split-Path -parent $MyInvocation.MyCommand.Definition)"

$packageArgs = @{
  packageName   = $env:ChocolateyPackageName
  fileType      = 'exe'
  url           = 'https://github.com/vertopolkaLF/silence/releases/download/v1.5/silence-v1.5-x86-setup.exe'
  url64bit      = 'https://github.com/vertopolkaLF/silence/releases/download/v1.5/silence-v1.5-x64-setup.exe'
  
  softwareName  = 'silence!*'
  
  checksum      = 'PLACEHOLDER_X86_CHECKSUM'
  checksumType  = 'sha256'
  checksum64    = 'PLACEHOLDER_X64_CHECKSUM'
  checksumType64= 'sha256'
  
  silentArgs    = '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP-'
  validExitCodes= @(0)
}

Install-ChocolateyPackage @packageArgs
