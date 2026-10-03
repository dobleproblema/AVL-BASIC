#requires -version 5.1
<#
Phase 2, to be launched explicitly by the user in 64-bit elevated PowerShell.
This script does not elevate itself, install anything, or change power/security
settings. It uses a unique WPR instance and never cancels an existing recording.
Only prepare/read this script when reviewing it: invoking it starts real traces.

Expected package: avl-basic.exe, workloads.json, cross_machine.ps1, programs/,
and cross_cpu_pmu.wprp beside this script. All measurement and tracing happen
under LOCALAPPDATA. Export to the delivery folder starts after tracing has ended.
#>
[CmdletBinding()]
param(
    [string]$PackageRoot = $PSScriptRoot,
    [string]$MachineLabel = $env:COMPUTERNAME,
    [ValidateRange(1,20)][int]$Runs = 2,
    [ValidateRange(0,10)][int]$Warmups = 1,
    [ValidateRange(0,3600)][int]$StartDelaySeconds = 45,
    [ValidateSet('IPC','Branches','LLC')][string[]]$Profiles = @('IPC'),
    [switch]$Pilot,
    [string]$ExportDirectory,
    [switch]$SkipExport
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2
$pmuUtf8 = New-Object Text.UTF8Encoding($false)
$pmuNativeEncoding = [Text.Encoding]::GetEncoding([Globalization.CultureInfo]::CurrentCulture.TextInfo.OEMCodePage)
$pmuRoot = $null
$pmuState = $null
$pmuFailed = $false
$pmuStopUncertain = $false
$pmuCalls = New-Object 'System.Collections.Generic.List[object]'
$pmuCallNumber = 0
if ($Pilot) { $Runs=1; $Warmups=1 }

function Save-PmuJson([string]$Path, $Value) {
    [IO.File]::WriteAllText($Path,($Value | ConvertTo-Json -Depth 30),$pmuUtf8)
}
function Pmu-Hash([string]$Path) { (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
function Check-PmuHash([string]$Path,[string]$Expected) {
    if ($Expected -notmatch '^[a-fA-F0-9]{64}$') { throw "Missing/invalid SHA256 for $Path" }
    $actual=Pmu-Hash $Path
    if ($actual -ne $Expected.ToLowerInvariant()) { throw "SHA256 mismatch: $Path; expected $Expected; actual $actual" }
    return $actual
}
function Pmu-Child([string]$Root,[string]$Relative) {
    if ([IO.Path]::IsPathRooted($Relative) -or [string]::IsNullOrWhiteSpace($Relative)) { throw "Expected relative path: $Relative" }
    $base=[IO.Path]::GetFullPath($Root).TrimEnd('\')+'\'
    $path=[IO.Path]::GetFullPath((Join-Path $base $Relative))
    if (-not $path.StartsWith($base,[StringComparison]::OrdinalIgnoreCase)) { throw "Path escapes package: $Relative" }
    return $path
}
function Pmu-Property($Object,[string]$Name,$Default) {
    $p=$Object.PSObject.Properties[$Name]
    if ($null -eq $p -or $null -eq $p.Value) { return $Default }
    return $p.Value
}
function Quote-PmuArgument([string]$Value) {
    # Windows CommandLineToArgvW/CRT quoting, including trailing backslashes.
    return '"'+[regex]::Replace([regex]::Replace($Value,'(\\*)"','$1$1\"'),'(\\+)$','$1$1')+'"'
}
function Save-PmuState {
    if ($null -ne $pmuState) {
        $pmuState.native_commands=@($pmuCalls.ToArray())
        Save-PmuJson (Join-Path $pmuRoot 'capture.json') $pmuState
    }
}
function Invoke-PmuNative([string]$Executable,[string[]]$Arguments,[string]$Label,[int]$TimeoutSeconds=300) {
    $script:pmuCallNumber++
    $stem='{0:D3}-{1}' -f $pmuCallNumber,$Label
    $prefix=Join-Path (Join-Path $pmuRoot 'diagnostics') $stem
    $outFile=[IO.File]::Create($prefix+'.stdout.bin')
    $errFile=[IO.File]::Create($prefix+'.stderr.bin')
    $proc=New-Object Diagnostics.Process
    $proc.StartInfo.FileName=$Executable
    $proc.StartInfo.Arguments=(@($Arguments | ForEach-Object { Quote-PmuArgument $_ }) -join ' ')
    $proc.StartInfo.UseShellExecute=$false
    $proc.StartInfo.CreateNoWindow=$true
    $proc.StartInfo.RedirectStandardOutput=$true
    $proc.StartInfo.RedirectStandardError=$true
    $proc.StartInfo.RedirectStandardInput=$true
    $call=[ordered]@{label=$Label; executable=$Executable; arguments=$Arguments; started_utc=[DateTime]::UtcNow.ToString('o'); exit_code=$null; timeout=$false; diagnostic_prefix=('diagnostics/'+$stem); display_decode_codepage=$pmuNativeEncoding.CodePage; error=$null}
    try {
        if (-not $proc.Start()) { throw "Could not start $Executable" }
        $proc.StandardInput.Close()
        $outTask=$proc.StandardOutput.BaseStream.CopyToAsync($outFile)
        $errTask=$proc.StandardError.BaseStream.CopyToAsync($errFile)
        if (-not $proc.WaitForExit($TimeoutSeconds*1000)) {
            $call.timeout=$true
            $proc.Kill() # Only this command-line client, never an unrelated trace/session.
            $proc.WaitForExit()
        }
        $outTask.GetAwaiter().GetResult(); $errTask.GetAwaiter().GetResult()
        $call.exit_code=$proc.ExitCode
    } catch {
        $call.error=$_.Exception.Message
        throw
    } finally {
        $outFile.Dispose(); $errFile.Dispose(); $proc.Dispose()
        $call.completed_utc=[DateTime]::UtcNow.ToString('o')
        $pmuCalls.Add([pscustomobject]$call)
        Save-PmuState
    }
    # Original bytes are retained; the UTF-8 views record their decoding choice.
    $stdout=$pmuNativeEncoding.GetString([IO.File]::ReadAllBytes($prefix+'.stdout.bin'))
    $stderr=$pmuNativeEncoding.GetString([IO.File]::ReadAllBytes($prefix+'.stderr.bin'))
    [IO.File]::WriteAllText($prefix+'.stdout.txt',$stdout,$pmuUtf8)
    [IO.File]::WriteAllText($prefix+'.stderr.txt',$stderr,$pmuUtf8)
    return [pscustomobject]@{exit_code=$call.exit_code; timeout=$call.timeout; stdout=$stdout; stderr=$stderr; text=($stdout+"`n"+$stderr); diagnostic_prefix=$call.diagnostic_prefix}
}
function Assert-PmuCommand($Result,[string]$Purpose) {
    if ($Result.timeout -or $Result.exit_code -ne 0) { throw "$Purpose failed (exit $($Result.exit_code), timeout $($Result.timeout)); see $($Result.diagnostic_prefix)." }
}
function Test-NoPmuSession([string]$Text) {
    return $Text.Trim() -eq 'No PMC profile sources are being used.'
}
function Test-NoWprRecording([string]$Text) {
    # WPR emits English on the tested Spanish Windows installations. Unknown
    # localized output is rejected, never guessed to mean that tracing is idle.
    return [regex]::IsMatch($Text,'(?m)^\s*WPR is not recording\s*$')
}
function Get-PmuSources($Result) {
    Assert-PmuCommand $Result 'Querying available PMU sources'
    $sources=@()
    foreach ($match in [regex]::Matches($Result.stdout,'(?m)^\s*(\d+)\s+(\S+)\s+(\d+)\s+(\d+)\s+(\d+)\s*$')) {
        $sources += [pscustomobject]@{id=[int]$match.Groups[1].Value; name=$match.Groups[2].Value; interval=[long]$match.Groups[3].Value; minimum=[long]$match.Groups[4].Value; maximum=[long]$match.Groups[5].Value}
    }
    if ($sources.Count -eq 0) { throw 'WPR returned no parseable PMU sources. Raw diagnostics were retained.' }
    return $sources
}
function Assert-PmuIdle([string]$Label) {
    $status=Invoke-PmuNative $pmuWpr @('-status','profiles','collectors','-details') ($Label+'-wpr-status')
    Assert-PmuCommand $status 'Checking existing WPR recording'
    if (-not (Test-NoWprRecording $status.text)) { throw 'Existing or unrecognized WPR recording state. No recording was stopped or cancelled.' }
    $sessions=Invoke-PmuNative $pmuWpr @('-pmcsessions') ($Label+'-pmcsessions')
    Assert-PmuCommand $sessions 'Checking existing PMU users'
    if (-not (Test-NoPmuSession $sessions.text)) { throw 'PMU counters are occupied, or session status is unrecognized. No recording was stopped or cancelled.' }
    $traces=Invoke-PmuNative (Join-Path $env:SystemRoot 'System32\logman.exe') @('query','-ets') ($Label+'-etw-sessions')
    Assert-PmuCommand $traces 'Listing existing ETW sessions'
    if ($traces.stdout -match '(?im)^\s*WPR[_-]') { throw 'Another named WPR instance is active. No recording was stopped or cancelled.' }
}

try {
    if ([IntPtr]::Size -ne 8) { throw 'Use 64-bit Windows PowerShell.' }
    $PackageRoot=(Resolve-Path -LiteralPath $PackageRoot).Path
    $safeLabel=$MachineLabel -replace '[^A-Za-z0-9_-]','_'
    $stamp='{0}-{1}-{2}' -f $safeLabel,(Get-Date -Format 'yyyyMMdd-HHmmss'),([Guid]::NewGuid().ToString('N').Substring(0,8))
    $pmuRoot=Join-Path $env:LOCALAPPDATA ('AVL-BASIC-CpuComparison\PMU\'+$stamp)
    $null=New-Item -ItemType Directory -Path $pmuRoot
    foreach ($folder in @('diagnostics','traces','runs','package','provenance')) { $null=New-Item -ItemType Directory -Path (Join-Path $pmuRoot $folder) }
    $pmuState=[ordered]@{schema_version=1; machine_label=$MachineLabel; started_utc=[DateTime]::UtcNow.ToString('o'); completed_utc=$null; status='preflight'; failure=$null; source_package=$PackageRoot; local_directory=$pmuRoot; runs=$Runs; warmups=$Warmups; delay_seconds=$StartDelaySeconds; pilot=[bool]$Pilot; requested_profiles=$Profiles; captures=@(); native_commands=@(); source_hashes=@{}; method='Selected independent strict PMU counter sets. Same workloads/executable and unchanged cross_machine runner. Automatic and fixed-affinity modes are preserved. PMU traces are process-wide, not BASIC-body-only. Counter order must be verified from live session diagnostics before decoding.'}
    Save-PmuState
    $identity=[Security.Principal.WindowsIdentity]::GetCurrent()
    $principal=New-Object Security.Principal.WindowsPrincipal($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Run CAPTURAR-PMU.cmd explicitly as Administrator. This script will not request elevation itself.' }

    $pmuWpr=Join-Path $env:SystemRoot 'System32\wpr.exe'
    if (-not (Test-Path -LiteralPath $pmuWpr)) { throw 'Windows Performance Recorder (wpr.exe) was not found. Nothing will be installed.' }
    $localPackage=Join-Path $pmuRoot 'package'
    $manifestPath=Pmu-Child $PackageRoot 'workloads.json'
    $manifest=Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    if ($manifest.schema_version -ne 1) { throw 'Unsupported workloads.json schema.' }
    $baseManifestHash=Pmu-Hash $manifestPath
    $exeSource=Pmu-Child $PackageRoot 'avl-basic.exe'
    $exeHash=Check-PmuHash $exeSource $manifest.executable_sha256
    Copy-Item -LiteralPath $manifestPath -Destination (Join-Path $pmuRoot 'provenance\base-workloads.json')
    Copy-Item -LiteralPath $exeSource -Destination (Join-Path $localPackage 'avl-basic.exe')
    $null=Check-PmuHash (Join-Path $localPackage 'avl-basic.exe') $exeHash
    $selected=@()
    $workloadNames=@('pi-10000','jelly','zoomer')
    if ($Pilot) { $workloadNames=@('pi-10000') }
    foreach ($name in $workloadNames) {
        $workloadMatches=@($manifest.workloads | Where-Object name -eq $name)
        if ($workloadMatches.Count -ne 1) { throw "Expected exactly one base workload named $name." }
        $workload=$workloadMatches[0]
        $source=Pmu-Child $PackageRoot $workload.program
        $destination=Pmu-Child $localPackage $workload.program
        $null=Check-PmuHash $source $workload.sha256
        $null=[IO.Directory]::CreateDirectory((Split-Path -Parent $destination))
        Copy-Item -LiteralPath $source -Destination $destination
        $null=Check-PmuHash $destination $workload.sha256
        foreach ($asset in @(Pmu-Property $workload 'files' @())) {
            $from=Pmu-Child (Split-Path -Parent $source) $asset.path
            $to=Pmu-Child (Split-Path -Parent $destination) $asset.path
            $null=Check-PmuHash $from $asset.sha256
            $null=[IO.Directory]::CreateDirectory((Split-Path -Parent $to))
            Copy-Item -LiteralPath $from -Destination $to
            $null=Check-PmuHash $to $asset.sha256
        }
        $selected += $workload
    }
    $manifest.workloads=$selected
    $manifest | Add-Member -NotePropertyName derived_from_manifest_sha256 -NotePropertyValue $baseManifestHash -Force
    Save-PmuJson (Join-Path $localPackage 'workloads.json') $manifest
    $pmuState.source_hashes.base_manifest=$baseManifestHash
    $pmuState.source_hashes.derived_manifest=Pmu-Hash (Join-Path $localPackage 'workloads.json')
    $pmuState.source_hashes.executable=$exeHash
    foreach ($name in @('cross_machine.ps1','cross_cpu_pmu.wprp','capture_cross_cpu_pmu.ps1')) {
        $source=Pmu-Child $PSScriptRoot $name
        if ($name -eq 'cross_machine.ps1') { $source=Pmu-Child $PackageRoot $name }
        $hash=Pmu-Hash $source
        Copy-Item -LiteralPath $source -Destination (Join-Path $localPackage $name)
        $null=Check-PmuHash (Join-Path $localPackage $name) $hash
        $pmuState.source_hashes[$name]=$hash
    }
    $profilePath=Join-Path $localPackage 'cross_cpu_pmu.wprp'
    [xml]$profileXml=Get-Content -LiteralPath $profilePath -Raw
    $allProfiles=@(
        [pscustomobject]@{name='AvlCpuIPC'; hardware='AvlCrossCpuIpcCounters'},
        [pscustomobject]@{name='AvlCpuBranches'; hardware='AvlCrossCpuBranchCounters'},
        [pscustomobject]@{name='AvlCpuLLC'; hardware='AvlCrossCpuLlcCounters'}
    )
    $requestedProfileNames=@($Profiles | Select-Object -Unique | ForEach-Object { 'AvlCpu'+$_ })
    $profileSpecifications=@($allProfiles | Where-Object { $_.name -in $requestedProfileNames })
    foreach ($spec in $profileSpecifications) {
        $hardware=$profileXml.SelectSingleNode("/WindowsPerformanceRecorder/Profiles/HardwareCounter[@Id='$($spec.hardware)']")
        if ($null -eq $hardware -or $hardware.Strict -ne 'true') { throw "Required Strict=true counters missing for $($spec.name)." }
        $binding=$profileXml.SelectSingleNode("/WindowsPerformanceRecorder/Profiles/Profile[@Name='$($spec.name)']/Collectors/SystemCollectorId/HardwareCounterId")
        if ($null -eq $binding -or $binding.Value -ne $spec.hardware) { throw "Unexpected counter binding for $($spec.name)." }
        $spec | Add-Member -NotePropertyName counters -NotePropertyValue @($hardware.Counters.Counter | ForEach-Object Value)
    }
    $pmuState.profiles=$profileSpecifications
    $profiles=Invoke-PmuNative $pmuWpr @('-profiles',$profilePath) 'validate-profiles'
    Assert-PmuCommand $profiles 'Reading WPR profiles'
    foreach ($spec in $profileSpecifications) { if ($profiles.stdout -notmatch [regex]::Escape($spec.name)) { throw "WPR did not list profile $($spec.name)." } }
    Assert-PmuIdle 'initial'
    $sources=@(Get-PmuSources (Invoke-PmuNative $pmuWpr @('-pmcsources') 'initial-pmcsources'))
    foreach ($spec in $profileSpecifications) {
        foreach ($counter in $spec.counters) { if ($counter -notin @($sources | ForEach-Object name)) { throw "PMU source unavailable: $counter. No captures have started." } }
    }
    $pmuState.initial_sources=$sources
    $pmuState.status='waiting'
    Save-PmuState
    Write-Host "Local staging verified: $pmuRoot"
    Write-Host 'This phase records hardware counters and also preserves automatic/fixed-core comparisons.'
    Write-Host 'Keep the desktop unlocked. Disconnect Moonlight during this delay; do not run other busy applications.'
    Write-Host "Tracing begins in $StartDelaySeconds seconds."
    if ($Pilot) { Write-Host 'Pilot: Pi 10000 only, one warmup and one measured repetition in each automatic/fixed-core mode.' }
    else { Write-Host 'Full workloads: Pi 10000, Jelly and Zoomer. Expect a few minutes per selected counter profile.' }
    if ($StartDelaySeconds -gt 0) { Start-Sleep -Seconds $StartDelaySeconds }

    foreach ($spec in $profileSpecifications) {
        Assert-PmuIdle ($spec.name+'-before')
        $beforeSources=@(Get-PmuSources (Invoke-PmuNative $pmuWpr @('-pmcsources') ($spec.name+'-before-pmcsources')))
        foreach ($counter in $spec.counters) { if ($counter -notin @($beforeSources | ForEach-Object name)) { throw "PMU source disappeared: $counter." } }
        $instance='AVLCPU-'+[Guid]::NewGuid().ToString('N')
        $etl=Join-Path $pmuRoot ('traces\'+$spec.name+'.etl')
        $runnerOutput=Join-Path $pmuRoot ('runs\'+$spec.name)
        $capture=[ordered]@{profile=$spec.name; instance=$instance; started_utc=$null; stopped_utc=$null; status='starting'; counters_requested=$spec.counters; sources_before=$beforeSources; sources_during=$null; pmcsessions_during=$null; counter_order='Verify actual source ID order from live pmcsessions diagnostics; XML order alone is not sufficient.'; runner_output=('runs/'+$spec.name); etl=('traces/'+$spec.name+'.etl'); etl_sha256=$null; error=$null; stop_error=$null}
        $pmuState.captures += $capture
        $pmuState.status='capturing'
        Save-PmuState
        $started=$false
        $startIssued=$false
        try {
            Write-Host "Capturing $($spec.name)..."
            $startIssued=$true
            $start=Invoke-PmuNative $pmuWpr @('-start',($profilePath+'!'+$spec.name),'-filemode','-instancename',$instance) ($spec.name+'-start')
            Assert-PmuCommand $start 'Starting our unique WPR instance with strict counters'
            $started=$true
            $capture.started_utc=[DateTime]::UtcNow.ToString('o')
            $capture.sources_during=@(Get-PmuSources (Invoke-PmuNative $pmuWpr @('-pmcsources') ($spec.name+'-during-pmcsources')))
            $live=Invoke-PmuNative $pmuWpr @('-pmcsessions') ($spec.name+'-during-pmcsessions')
            Assert-PmuCommand $live 'Querying live PMU session configuration'
            $capture.pmcsessions_during=$live.diagnostic_prefix
            if (Test-NoPmuSession $live.text) { throw 'WPR started but reports no active PMU sources. Capture is unusable.' }
            Save-PmuState
            $runner=Join-Path $localPackage 'cross_machine.ps1'
            $shell=Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
            $runnerResult=Invoke-PmuNative $shell @('-NoProfile','-ExecutionPolicy','Bypass','-File',$runner,'-PackageRoot',$localPackage,'-MachineLabel',$MachineLabel,'-Runs',[string]$Runs,'-Warmups',[string]$Warmups,'-StartDelaySeconds','0','-Output',$runnerOutput) ($spec.name+'-runner') 1800
            Assert-PmuCommand $runnerResult 'Workload runner'
            $runnerMetadata=Get-Content -LiteralPath (Join-Path $runnerOutput 'metadata.json') -Raw | ConvertFrom-Json
            if ($runnerMetadata.status -ne 'complete') { throw 'Runner did not complete successfully.' }
            $capture.affinity_selection=$runnerMetadata.affinity_selection
            if ($runnerMetadata.affinity_selection.classification -ne 'p_core') { throw 'Runner could not classify a P-core. Raw data is preserved but this is not a verified P-core PMU comparison.' }
            $capture.status='measured'
        } catch {
            $capture.status='failed'; $capture.error=$_.Exception.Message
            throw
        } finally {
            # Recover an uncertain start only by querying our unguessable instance.
            # A status response naming our profile establishes that our start worked.
            if ($startIssued -and -not $started) {
                try {
                    $own=Invoke-PmuNative $pmuWpr @('-status','profiles','collectors','-details','-instancename',$instance) ($spec.name+'-own-status-after-start-error')
                    $started=($own.exit_code -eq 0 -and -not $own.timeout -and -not (Test-NoWprRecording $own.text) -and $own.text -match [regex]::Escape($spec.name))
                    if ($own.exit_code -ne 0 -or $own.timeout -or (-not $started -and -not (Test-NoWprRecording $own.text))) {
                        $pmuStopUncertain=$true
                        $capture.stop_error='The state of our attempted WPR instance could not be established.'
                    }
                } catch { $pmuStopUncertain=$true; $capture.stop_error='Could not verify whether our attempted start succeeded: '+$_.Exception.Message }
            }
            if ($started) {
                try {
                    $stop=Invoke-PmuNative $pmuWpr @('-stop',$etl,('AVL-BASIC cross CPU '+$spec.name),'-skipPdbGen','-compress','-instancename',$instance) ($spec.name+'-stop') 600
                    Assert-PmuCommand $stop 'Stopping only our WPR instance'
                    $capture.stopped_utc=[DateTime]::UtcNow.ToString('o')
                    if (-not (Test-Path -LiteralPath $etl) -or (Get-Item -LiteralPath $etl).Length -eq 0) { throw 'WPR did not produce a nonempty ETL.' }
                    $capture.etl_sha256=Pmu-Hash $etl
                    if ($capture.status -eq 'measured') { $capture.status='complete' }
                } catch {
                    $capture.stop_error=$_.Exception.Message
                    $capture.status='failed'
                    $pmuFailed=$true
                    $pmuStopUncertain=$true
                    Write-Warning ('Our WPR stop failed. Do not cancel other recordings. Unique instance: '+$instance)
                }
            }
            Save-PmuState
        }
        if ($capture.status -ne 'complete') { throw "Capture failed for $($spec.name). See its diagnostics; unique WPR instance $instance." }
    }
    $pmuState.status='complete'
} catch {
    $pmuFailed=$true
    if ($null -ne $pmuState) {
        $pmuState.status='failed'; $pmuState.failure=$_.Exception.Message
        [IO.File]::WriteAllText((Join-Path $pmuRoot 'failure.txt'),($_ | Out-String),$pmuUtf8)
    }
    Write-Warning $_.Exception.Message
} finally {
    if ($null -ne $pmuState) {
        $pmuState.completed_utc=[DateTime]::UtcNow.ToString('o')
        $pmuState.cleanup_uncertain=$pmuStopUncertain
        Save-PmuState
        Write-Host "Capture status: $($pmuState.status). Local data: $pmuRoot"
        if ($pmuStopUncertain) {
            Write-Warning 'Compression/export skipped because our WPR instance may still be active. Inspect capture.json and its unique instance name; do not cancel unrelated recordings.'
        } else { try {
            Add-Type -AssemblyName System.IO.Compression.FileSystem
            $archive=$pmuRoot+'.zip'
            [IO.Compression.ZipFile]::CreateFromDirectory($pmuRoot,$archive,[IO.Compression.CompressionLevel]::Fastest,$true)
            $zipHash=Pmu-Hash $archive
            [IO.File]::WriteAllText($archive+'.sha256',$zipHash+'  '+[IO.Path]::GetFileName($archive)+[Environment]::NewLine,$pmuUtf8)
            Write-Host "Local ZIP: $archive"
            if (-not $SkipExport) {
                if ([string]::IsNullOrWhiteSpace($ExportDirectory)) { $ExportDirectory=Join-Path $PSScriptRoot 'RESULTADOS-PMU' }
                $null=[IO.Directory]::CreateDirectory($ExportDirectory)
                $export=Join-Path $ExportDirectory ([IO.Path]::GetFileName($archive))
                if (Test-Path -LiteralPath $export) { throw "Export already exists: $export" }
                Copy-Item -LiteralPath $archive -Destination $export
                $null=Check-PmuHash $export $zipHash
                Copy-Item -LiteralPath ($archive+'.sha256') -Destination ($export+'.sha256')
                Write-Host "Verified export: $export"
            }
        } catch {
            $pmuFailed=$true
            Write-Warning ('Compression/export failed; all original local data remains: '+$_.Exception.Message)
        } }
    }
}
if ($pmuFailed) { exit 1 }
Write-Host 'Finished. The ETL files need offline counter-order, coverage and lost-event checks before IPC/cache conclusions.'
