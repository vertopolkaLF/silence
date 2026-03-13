$ErrorActionPreference = 'Stop'
$toolsDir   = "$(Split-Path -parent $MyInvocation.MyCommand.Definition)"

$packageArgs = @{
  packageName   = $env:ChocolateyPackageName
  fileType      = 'exe'
  url           = '__URL32__'
  url64bit      = '__URL64__'
  
  softwareName  = 'silence!*'
  
  checksum      = '__CHECKSUM32__'
  checksumType  = 'sha256'
  checksum64    = '__CHECKSUM64__'
  checksumType64= 'sha256'
  
  silentArgs    = '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP-'
  validExitCodes= @(0)
}

Install-ChocolateyPackage @packageArgs
