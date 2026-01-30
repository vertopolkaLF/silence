$ErrorActionPreference = 'Stop'
$toolsDir   = "$(Split-Path -parent $MyInvocation.MyCommand.Definition)"

$packageArgs = @{
  packageName   = $env:ChocolateyPackageName
  fileType      = 'exe'
  url           = 'https://github.com/vertopolkaLF/silence/releases/download/v1.6/silence-v1.5-x86-setup.exe'
  url64bit      = 'https://github.com/vertopolkaLF/silence/releases/download/v1.6/silence-v1.5-x64-setup.exe'
  
  softwareName  = 'silence!*'
  
  checksum      = 'E3D67EEA6B32DA317925129CD7790B703A2C0A6AD86824DE0C33F3EB929B8BCB'
  checksumType  = 'sha256'
  checksum64    = '0C14425A224094FEE941BEFE4E9A8A84621314B793B9B967A85EC2C1C5D2BFA5'
  checksumType64= 'sha256'
  
  silentArgs    = '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP-'
  validExitCodes= @(0)
}

Install-ChocolateyPackage @packageArgs
