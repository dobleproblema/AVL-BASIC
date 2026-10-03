#requires -Version 7.0
<#
Offline experimental PMC reader; never starts a recording or requests elevation.
Requires the version/hash-pinned TraceEvent 3.0.6 DLL from Visual Studio.

TargetMetadata accepts the unchanged cross_machine runner's raw.json, or:
  {"targets":[{"pid":123,"exactImage":"C:\\bench\\avl-basic.exe",
                "started_utc":"2026-10-01T20:00:00Z","wall_s":2.0}]}
Without metadata/ProcessId, discovers lifetimes named exactly avl-basic.exe.

CounterOrder is optional. Only independently verified ordering may be labelled:
  {"verified":true,"evidence":"Description/path of verified live PMC session order",
   "counters":[{"index":0,"name":"InstructionRetired","sourceId":26}]}
Do not infer this order from the WPRP XML. Without verification counters remain
indexed decimal strings. No IPC/GHz calculations are emitted in either case.

Outputs pmc-report.json, switches.ndjson and intervals.ndjson. Partial sums are
only accepted complete scheduled intervals, never promised whole-process totals.
Any reported event/buffer loss disables sums. Missing PMC returns a nonzero exit
after writing diagnostics. OutputDirectory must be new.
#>
param(
    [Parameter(Mandatory=$true)][string]$Etl,
    [Parameter(Mandatory=$true)][string]$OutputDirectory,
    [string]$TargetMetadata,
    [int]$ProcessId = 0,
    [string]$ExactImage,
    [string]$CounterOrder,
    [string]$PrivateAssemblies = 'C:\Program Files\Microsoft Visual Studio\2022\Community\Common7\IDE\PrivateAssemblies'
)
$ErrorActionPreference = 'Stop'
[Threading.Thread]::CurrentThread.CurrentCulture = [Globalization.CultureInfo]::InvariantCulture
if (Test-Path -LiteralPath $OutputDirectory) { throw 'OutputDirectory must not exist; preserve earlier evidence by choosing a new directory.' }
$taskEtl = (Resolve-Path -LiteralPath $Etl).Path
$taskOutput = [IO.Path]::GetFullPath($OutputDirectory)
$taskDependencies = @('Microsoft.Diagnostics.FastSerialization.dll','Dia2Lib.dll','TraceReloggerLib.dll','OSExtensions.dll','Microsoft.Diagnostics.Tracing.TraceEvent.dll')
foreach ($taskDependency in $taskDependencies) { [void][Reflection.Assembly]::LoadFrom((Join-Path $PrivateAssemblies $taskDependency)) }
$taskReferences = @((Join-Path $PrivateAssemblies 'Microsoft.Diagnostics.Tracing.TraceEvent.dll'), (Join-Path $PrivateAssemblies 'Microsoft.Diagnostics.FastSerialization.dll'))
$taskReferences += Get-ChildItem -LiteralPath (Join-Path $PSHOME 'ref') -Filter '*.dll' | ForEach-Object FullName
Add-Type -Path (Join-Path $PSScriptRoot 'ExtractPmcTrace.cs') -ReferencedAssemblies $taskReferences -CompilerOptions '/unsafe'

$taskTargets = [Collections.Generic.List[AvlPmcTrace+Target]]::new()
$taskInputHashes = @{}
function ConvertTo-TaskUtcText($Value) {
    if ($Value -is [DateTime]) { return $Value.ToUniversalTime().ToString('o') }
    if ($Value -is [DateTimeOffset]) { return $Value.ToUniversalTime().ToString('o') }
    return [string]$Value
}
if ($TargetMetadata) {
    $taskMetadataPath = (Resolve-Path -LiteralPath $TargetMetadata).Path
    $taskMetadata = Get-Content -LiteralPath $taskMetadataPath -Raw | ConvertFrom-Json
    $taskInputHashes.target_metadata = @{ path=$taskMetadataPath; sha256=(Get-FileHash -LiteralPath $taskMetadataPath -Algorithm SHA256).Hash.ToLowerInvariant() }
    if ($taskMetadata.targets) {
        foreach ($taskEntry in $taskMetadata.targets) {
            if ([int]$taskEntry.pid -le 0) { throw 'Each targets entry needs a positive pid.' }
            $taskTarget = [AvlPmcTrace+Target]::new()
            $taskTarget.Pid = [int]$taskEntry.pid
            $taskTarget.ExactImage = [string]$taskEntry.exactImage
            $taskTarget.StartedUtc = ConvertTo-TaskUtcText $taskEntry.started_utc
            $taskTarget.WallSeconds = [double]$taskEntry.wall_s
            $taskTarget.EffectiveAffinity = [string]$taskEntry.effectiveAffinity
            $taskTarget.Label = [string]$taskEntry.label
            $taskTargets.Add($taskTarget)
        }
    } elseif ($taskMetadata.records) {
        $taskRunnerImage = $ExactImage
        if (-not $taskRunnerImage -and $taskMetadata.metadata.package_root) {
            $taskRunnerImage = [IO.Path]::Combine([string]$taskMetadata.metadata.package_root, 'avl-basic.exe')
        }
        foreach ($taskEntry in $taskMetadata.records) {
            if ([int]$taskEntry.native.ProcessId -le 0) { continue }
            $taskTarget = [AvlPmcTrace+Target]::new()
            $taskTarget.Pid = [int]$taskEntry.native.ProcessId
            $taskTarget.ExactImage = $taskRunnerImage
            $taskTarget.StartedUtc = ConvertTo-TaskUtcText $taskEntry.started_utc
            $taskTarget.WallSeconds = [double]$taskEntry.wall_s
            $taskTarget.EffectiveAffinity = [string]$taskEntry.native.EffectiveAffinity
            $taskTarget.Label = "sequence=$($taskEntry.sequence); workload=$($taskEntry.workload); mode=$($taskEntry.mode)"
            $taskTargets.Add($taskTarget)
        }
    } else { throw 'TargetMetadata must contain targets[] or runner records[].native.ProcessId.' }
    if ($taskTargets.Count -eq 0) { throw 'TargetMetadata contains no positive process IDs.' }
}
if ($ProcessId -gt 0) {
    $taskTarget = [AvlPmcTrace+Target]::new()
    $taskTarget.Pid = $ProcessId; $taskTarget.ExactImage = $ExactImage
    $taskTargets.Add($taskTarget)
}
$taskNames = [Collections.Generic.List[AvlPmcTrace+CounterName]]::new()
$taskOrderVerified = $false
$taskOrderEvidence = ''
if ($CounterOrder) {
    $taskOrderPath = (Resolve-Path -LiteralPath $CounterOrder).Path
    $taskOrder = Get-Content -LiteralPath $taskOrderPath -Raw | ConvertFrom-Json
    $taskInputHashes.counter_order = @{ path=$taskOrderPath; sha256=(Get-FileHash -LiteralPath $taskOrderPath -Algorithm SHA256).Hash.ToLowerInvariant() }
    $taskOrderVerified = $taskOrder.verified -is [bool] -and $taskOrder.verified
    $taskOrderEvidence = [string]$taskOrder.evidence
    foreach ($taskEntry in $taskOrder.counters) {
        $taskName = [AvlPmcTrace+CounterName]::new()
        $taskName.Index = [int]$taskEntry.index; $taskName.Name = [string]$taskEntry.name
        if ($null -ne $taskEntry.sourceId) { $taskName.SourceId = [int]$taskEntry.sourceId }
        $taskNames.Add($taskName)
    }
}
[void](New-Item -ItemType Directory -Path $taskOutput)
$taskInputHashes | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $taskOutput 'input-manifests.json') -Encoding utf8
$taskResult = [AvlPmcTrace]::Read($taskEtl, $taskOutput, $taskTargets.ToArray(), $taskNames.ToArray(), $taskOrderVerified, $taskOrderEvidence)
$taskResult | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $taskOutput 'pmc-report.json') -Encoding utf8
$taskResult | Select-Object Status,AllSwitchRows,AllPmcRows,ExportedSwitchRows,TargetIntervals,IncludedIntervals,EventsLost,BuffersLost,CounterOrderVerified,Warnings | ConvertTo-Json -Depth 5
if ($taskResult.Status.StartsWith('rejected_')) { throw "PMC extraction rejected: $($taskResult.Status). See pmc-report.json; no usable labelled metrics have been produced." }
