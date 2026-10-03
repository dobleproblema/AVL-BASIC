#requires -version 5.1
# User launcher: stage locally, measure away from OneDrive, then export results.
[CmdletBinding()]
param([int]$Runs=7, [int]$Warmups=2, [int]$StartDelaySeconds=45, [switch]$Smoke)
$ErrorActionPreference='Stop'
$taskSource=$PSScriptRoot
$taskStamp='{0}-{1}-{2}' -f $env:COMPUTERNAME,(Get-Date -Format 'yyyyMMdd-HHmmss'),([Guid]::NewGuid().ToString('N').Substring(0,8))
$taskLocal=Join-Path $env:LOCALAPPDATA ('AVL-BASIC-CpuComparison\'+$taskStamp)
$taskPackage=Join-Path $taskLocal 'package'
$taskResults=Join-Path $taskLocal 'results'
$null=New-Item -ItemType Directory -Path $taskPackage
foreach($taskName in @('avl-basic.exe','workloads.json','cross_machine.ps1','programs')) {
    Copy-Item -LiteralPath (Join-Path $taskSource $taskName) -Destination $taskPackage -Recurse
}
Write-Host 'AVL-BASIC CPU comparison: no administrator rights required.'
Write-Host 'Close busy applications and disconnect Moonlight during the start delay.'
Write-Host 'Do not lock the desktop: some tests open a graphics window.'
Write-Host 'The full run takes approximately 6-8 minutes.'
$taskRunner=Join-Path $taskPackage 'cross_machine.ps1'
$taskFailed=$false
try {
    & $taskRunner -PackageRoot $taskPackage -MachineLabel $env:COMPUTERNAME -Runs $Runs -Warmups $Warmups -StartDelaySeconds $StartDelaySeconds -Output $taskResults -Smoke:$Smoke
} catch {
    $taskFailed=$true
    Write-Warning $_.Exception.Message
} finally {
    if(Test-Path -LiteralPath $taskResults) {
        $taskExport=Join-Path $taskSource 'RESULTADOS'
        $null=[IO.Directory]::CreateDirectory($taskExport)
        $taskZip=Join-Path $taskExport ($taskStamp+'.zip')
        Compress-Archive -LiteralPath $taskResults -DestinationPath $taskZip -CompressionLevel Optimal
        Write-Host ('Results exported to: '+$taskZip)
        Write-Host ('SHA256: '+(Get-FileHash -LiteralPath $taskZip -Algorithm SHA256).Hash)
    }
}
if($taskFailed) { throw 'Comparison failed. The exported results include the error.' }
