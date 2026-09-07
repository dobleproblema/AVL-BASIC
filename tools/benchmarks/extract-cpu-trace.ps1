#requires -Version 7.0
param(
    [Parameter(Mandatory=$true)][string]$Etl,
    [Parameter(Mandatory=$true)][string]$ExactImage,
    [Parameter(Mandatory=$true)][string]$SymbolDirectory,
    [Parameter(Mandatory=$true)][string]$OutputDirectory,
    [int]$ProcessId = 0,
    [double]$FromMs = 0,
    [double]$ToMs = [double]::MaxValue,
    [string]$PrivateAssemblies = 'C:\Program Files\Microsoft Visual Studio\2022\Community\Common7\IDE\PrivateAssemblies'
)
$ErrorActionPreference = 'Stop'
[Threading.Thread]::CurrentThread.CurrentCulture = [Globalization.CultureInfo]::InvariantCulture
if (Test-Path -LiteralPath $OutputDirectory) { throw 'OutputDirectory must not exist; use a new directory to preserve prior evidence.' }
$taskEtl = (Resolve-Path -LiteralPath $Etl).Path
$taskSymbols = (Resolve-Path -LiteralPath $SymbolDirectory).Path
$taskOutput = [IO.Path]::GetFullPath($OutputDirectory)
[void](New-Item -ItemType Directory -Path $taskOutput)
$taskTraceDll = Join-Path $PrivateAssemblies 'Microsoft.Diagnostics.Tracing.TraceEvent.dll'
$taskDependencies = @('Microsoft.Diagnostics.FastSerialization.dll','Dia2Lib.dll','TraceReloggerLib.dll','OSExtensions.dll','Microsoft.Diagnostics.Tracing.TraceEvent.dll')
foreach($taskDependency in $taskDependencies) { [void][Reflection.Assembly]::LoadFrom((Join-Path $PrivateAssemblies $taskDependency)) }
$taskReferences = @($taskTraceDll, (Join-Path $PrivateAssemblies 'Microsoft.Diagnostics.FastSerialization.dll'))
# PowerShell 7 Add-Type's platform references are supplied explicitly along
# with TraceEvent so the source compiles against this host's own .NET runtime.
if ($PSVersionTable.PSEdition -eq 'Core') { $taskReferences += Get-ChildItem -LiteralPath (Join-Path $PSHOME 'ref') -Filter '*.dll' | ForEach-Object FullName }
else { $taskReferences += 'System.dll','System.Core.dll','System.Net.Http.dll' }
Add-Type -Path (Join-Path $PSScriptRoot 'ExtractCpuTrace.cs') -ReferencedAssemblies $taskReferences
$taskResult = [AvlCpuTrace]::Read($taskEtl, $taskOutput, $ProcessId, $ExactImage, $taskSymbols, $FromMs, $ToMs)
$taskResult | ConvertTo-Json -Depth 15 | Set-Content -LiteralPath (Join-Path $taskOutput 'profile.json') -Encoding utf8
$taskResult.Functions | Export-Csv -LiteralPath (Join-Path $taskOutput 'functions.csv') -NoTypeInformation -Encoding utf8
$taskResult.Lines | Export-Csv -LiteralPath (Join-Path $taskOutput 'lines.csv') -NoTypeInformation -Encoding utf8
$taskResult | Select-Object Process,Samples,SamplesWithStack,UnknownLeafSamples,SamplesWithUnknownFrame,EventsLost,Truncated,Warnings | ConvertTo-Json -Depth 5
