#requires -version 5.1
<#
Run an identical, hashed AVL-BASIC package on two Windows machines.
Use 64-bit Windows PowerShell: powershell.exe -NoProfile -File .\cross_machine.ps1
No settings are changed. Each child starts suspended, receives its affinity and
normal priority before running, and writes to a new, private result directory.

workloads.json:
 {"schema_version":1,"executable_sha256":"...","workloads":[
   {"name":"pi","program":"programs/pi/program.bas","sha256":"...",
    "graphics":false,"timeout_seconds":120,"frames":null,
    "files":[{"path":"asset.png","sha256":"..."}]}]}
Optional workload fields: png_file (default frame.png), expected_stdout_sha256,
expected_png_sha256. Asset paths are relative to the program's directory.
The only stdout normalization is CRLF -> LF and the numeric timing marker.
Smoke uses the SAME first program with zero warmups and one measured repetition.
QueryProcessCycleTime is diagnostic process accounting, NOT IPC or a clock meter.
#>
[CmdletBinding()]
param(
    [string]$PackageRoot = $PSScriptRoot,
    [string]$MachineLabel = $env:COMPUTERNAME,
    [ValidateRange(1,100)][int]$Runs = 7,
    [ValidateRange(0,20)][int]$Warmups = 2,
    [ValidateRange(0,3600)][int]$StartDelaySeconds = 30,
    [string]$Output,
    [switch]$Smoke
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2
if ([IntPtr]::Size -ne 8) { throw 'Use 64-bit Windows PowerShell.' }
if ($Smoke) { $Runs = 1; $Warmups = 0 }
$utf8 = New-Object System.Text.UTF8Encoding($false)
$invariant = [System.Globalization.CultureInfo]::InvariantCulture

function Property-Or($Object, [string]$Name, $Default) {
    $p = $Object.PSObject.Properties[$Name]
    if ($null -eq $p -or $null -eq $p.Value) { return $Default }
    return $p.Value
}
function Safe-Child([string]$Root, [string]$Relative) {
    if ([IO.Path]::IsPathRooted($Relative) -or [string]::IsNullOrWhiteSpace($Relative)) {
        throw "Expected a relative package path: $Relative"
    }
    $base = [IO.Path]::GetFullPath($Root).TrimEnd('\') + '\'
    $path = [IO.Path]::GetFullPath((Join-Path $base $Relative))
    if (-not $path.StartsWith($base, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Path escapes its directory: $Relative"
    }
    return $path
}
function File-Hash([string]$Path) { (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
function Text-Hash([string]$Text) {
    $sha = [Security.Cryptography.SHA256]::Create()
    try { return ([BitConverter]::ToString($sha.ComputeHash($utf8.GetBytes($Text)))).Replace('-','').ToLowerInvariant() }
    finally { $sha.Dispose() }
}
function Check-Hash([string]$Path, [string]$Expected) {
    if ($Expected -notmatch '^[0-9a-fA-F]{64}$') { throw "Missing/invalid expected SHA256 for $Path" }
    $actual = File-Hash $Path
    if ($actual -ne $Expected.ToLowerInvariant()) { throw "SHA256 mismatch: $Path (expected $Expected; actual $actual)" }
    return $actual
}
function Save-Json([string]$Path, $Value) {
    [IO.File]::WriteAllText($Path, ($Value | ConvertTo-Json -Depth 30), $utf8)
}
function Median([double[]]$Values) {
    $v = @($Values | Sort-Object)
    if ($v.Count -eq 0) { return $null }
    $mid = [int][Math]::Floor($v.Count / 2)
    if ($v.Count % 2) { return [double]$v[$mid] }
    return ([double]$v[$mid-1] + [double]$v[$mid]) / 2
}
function Metric-Stats([double[]]$Values) {
    $med = Median $Values
    if ($null -eq $med) { return $null }
    $deviations = @($Values | ForEach-Object { [Math]::Abs($_ - $med) })
    return [ordered]@{median=$med; mad=(Median $deviations); min=($Values | Measure-Object -Minimum).Minimum; max=($Values | Measure-Object -Maximum).Maximum}
}
function Read-Inventory {
    $info = [ordered]@{}
    foreach ($spec in @(
        @('os','Win32_OperatingSystem',@('Caption','Version','BuildNumber','OSArchitecture','LastBootUpTime')),
        @('system','Win32_ComputerSystem',@('Manufacturer','Model','TotalPhysicalMemory','NumberOfLogicalProcessors','HypervisorPresent')),
        @('cpu','Win32_Processor',@('Name','Manufacturer','ProcessorId','NumberOfCores','NumberOfLogicalProcessors','MaxClockSpeed','CurrentClockSpeed','LoadPercentage')),
        @('memory','Win32_PhysicalMemory',@('Manufacturer','PartNumber','Capacity','Speed','ConfiguredClockSpeed','SMBIOSMemoryType')),
        @('gpu_display','Win32_VideoController',@('Name','DriverVersion','DriverDate','CurrentHorizontalResolution','CurrentVerticalResolution','CurrentRefreshRate','VideoModeDescription')),
        @('bios','Win32_BIOS',@('Manufacturer','SMBIOSBIOSVersion','ReleaseDate'))
    )) {
        try { $info[$spec[0]] = @(Get-CimInstance -ClassName $spec[1] | Select-Object -Property $spec[2]) }
        catch { $info[$spec[0]] = @{error=$_.Exception.Message} }
    }
    try { $info.power_plan = (& powercfg.exe /getactivescheme 2>&1 | Out-String).Trim() } catch { $info.power_plan = $_.Exception.Message }
    try { $info.power_settings = (& powercfg.exe /query SCHEME_CURRENT SUB_PROCESSOR 2>&1 | Out-String).Trim() } catch { $info.power_settings = $_.Exception.Message }
    $info.streaming_processes = @(Get-Process | Where-Object { $_.ProcessName -match 'sunshine|moonlight|obs|nvstream|parsec|mstsc' } | Select-Object ProcessName,Id,CPU)
    $info.session = $env:SESSIONNAME
    $info.note = 'CIM clocks are static snapshots, not measured workload frequencies. Streaming process presence does not establish an active stream.'
    return $info
}

Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.IO;
using System.Collections;
using System.Collections.Generic;
using System.ComponentModel;
using System.Diagnostics;
using System.Runtime.InteropServices;
namespace AvlCrossMachine {
    public class CpuSet {
        public uint Id; public ushort Group; public byte LogicalProcessorIndex, CoreIndex, LastLevelCacheIndex, NumaNodeIndex, EfficiencyClass, Flags;
        public string RawHex;
    }
    public class RunResult {
        public uint ProcessId, ExitCode;
        public double WallSeconds, RunningWallSeconds, CpuSeconds, UserSeconds, KernelSeconds;
        public ulong Cycles, RequestedAffinity, EffectiveAffinity;
        public bool TimedOut, AffinitySetBeforeResume, CyclesAvailable;
        public int CyclesError;
    }
    public static class Native {
        [StructLayout(LayoutKind.Sequential)] struct SA { public int Length; public IntPtr Descriptor; public int Inherit; }
        [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] struct SI {
            public uint cb; public string reserved, desktop, title;
            public uint x,y,xsize,ysize,xchars,ychars,fill,flags; public ushort show,reserved2;
            public IntPtr reservedBytes, stdin, stdout, stderr;
        }
        [StructLayout(LayoutKind.Sequential)] struct PI { public IntPtr process, thread; public uint pid, tid; }
        [StructLayout(LayoutKind.Sequential)] struct FT { public uint low, high; public ulong Value { get { return ((ulong)high << 32) | low; } } }
        [DllImport("kernel32.dll", SetLastError=true)] static extern bool GetSystemCpuSetInformation(IntPtr buffer, uint length, out uint required, IntPtr process, uint flags);
        [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern IntPtr CreateFileW(string path, uint access, uint share, ref SA sa, uint creation, uint flags, IntPtr template);
        [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern bool CreateProcessW(string app, StringBuilder command, IntPtr psa, IntPtr tsa, bool inherit, uint flags, IntPtr environment, string cwd, ref SI si, out PI pi);
        [DllImport("kernel32.dll", SetLastError=true)] static extern bool SetProcessAffinityMask(IntPtr process, UIntPtr mask);
        [DllImport("kernel32.dll", SetLastError=true)] static extern bool GetProcessAffinityMask(IntPtr process, out UIntPtr mask, out UIntPtr system);
        [DllImport("kernel32.dll", SetLastError=true)] static extern uint ResumeThread(IntPtr thread);
        [DllImport("kernel32.dll", SetLastError=true)] static extern uint WaitForSingleObject(IntPtr handle, uint ms);
        [DllImport("kernel32.dll", SetLastError=true)] static extern bool GetExitCodeProcess(IntPtr process, out uint code);
        [DllImport("kernel32.dll", SetLastError=true)] static extern bool GetProcessTimes(IntPtr process, out FT created, out FT exited, out FT kernel, out FT user);
        [DllImport("kernel32.dll", SetLastError=true)] static extern bool QueryProcessCycleTime(IntPtr process, out ulong cycles);
        [DllImport("kernel32.dll", SetLastError=true)] static extern bool TerminateProcess(IntPtr process, uint code);
        [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
        static void Check(bool ok, string operation) { if (!ok) throw new Win32Exception(Marshal.GetLastWin32Error(), operation); }
        static void Close(IntPtr h) { if (h != IntPtr.Zero && h != new IntPtr(-1)) CloseHandle(h); }
        public static CpuSet[] Topology() {
            uint needed; GetSystemCpuSetInformation(IntPtr.Zero, 0, out needed, IntPtr.Zero, 0);
            if (needed == 0) throw new Win32Exception(Marshal.GetLastWin32Error(), "GetSystemCpuSetInformation size");
            IntPtr data = Marshal.AllocHGlobal((int)needed);
            try {
                Check(GetSystemCpuSetInformation(data, needed, out needed, IntPtr.Zero, 0), "GetSystemCpuSetInformation");
                var result = new List<CpuSet>();
                for (int off=0; off<(int)needed;) {
                    IntPtr p = IntPtr.Add(data,off); int size=Marshal.ReadInt32(p); int type=Marshal.ReadInt32(p,4);
                    if (size<8 || off+size>needed) throw new InvalidDataException("Invalid CPU topology record");
                    if (type==0 && size>=32) {
                        byte[] raw=new byte[size]; Marshal.Copy(p,raw,0,size);
                        result.Add(new CpuSet { Id=(uint)Marshal.ReadInt32(p,8), Group=(ushort)Marshal.ReadInt16(p,12),
                            LogicalProcessorIndex=Marshal.ReadByte(p,14), CoreIndex=Marshal.ReadByte(p,15),
                            LastLevelCacheIndex=Marshal.ReadByte(p,16), NumaNodeIndex=Marshal.ReadByte(p,17),
                            EfficiencyClass=Marshal.ReadByte(p,18), Flags=Marshal.ReadByte(p,19), RawHex=BitConverter.ToString(raw).Replace("-", "") });
                    }
                    off+=size;
                }
                return result.ToArray();
            } finally { Marshal.FreeHGlobal(data); }
        }
        static string Quote(string value) {
            var b=new StringBuilder("\""); int slashes=0;
            foreach(char c in value) {
                if(c=='\\') { slashes++; continue; }
                if(c=='\"') { b.Append('\\',slashes*2+1); b.Append(c); slashes=0; continue; }
                b.Append('\\',slashes); slashes=0; b.Append(c);
            }
            b.Append('\\',slashes*2); b.Append('"'); return b.ToString();
        }
        static IntPtr Env(string window) {
            var sorted=new SortedDictionary<string,string>(StringComparer.OrdinalIgnoreCase);
            foreach(DictionaryEntry entry in Environment.GetEnvironmentVariables()) sorted[(string)entry.Key]=(string)entry.Value;
            sorted["AVL_BASIC_WINDOW"]=window;
            var b=new StringBuilder(); foreach(var pair in sorted) b.Append(pair.Key).Append('=').Append(pair.Value).Append('\0');
            b.Append('\0'); return Marshal.StringToHGlobalUni(b.ToString());
        }
        public static RunResult Run(string exe, string program, string cwd, string window, ulong affinity, int timeoutMs, string stdout, string stderr) {
            SA sa=new SA {Length=Marshal.SizeOf(typeof(SA)), Inherit=1};
            IntPtr output=IntPtr.Zero,error=IntPtr.Zero,input=IntPtr.Zero,environment=IntPtr.Zero;
            PI pi=new PI(); bool done=false; var result=new RunResult {RequestedAffinity=affinity};
            try {
                output=CreateFileW(stdout,0x40000000,1,ref sa,1,0x80,IntPtr.Zero); Check(output!=new IntPtr(-1),"Create stdout");
                error=CreateFileW(stderr,0x40000000,1,ref sa,1,0x80,IntPtr.Zero); Check(error!=new IntPtr(-1),"Create stderr");
                input=CreateFileW("NUL",0x80000000,3,ref sa,3,0x80,IntPtr.Zero); Check(input!=new IntPtr(-1),"Open NUL stdin");
                SI si=new SI {cb=(uint)Marshal.SizeOf(typeof(SI)),flags=0x100,stdin=input,stdout=output,stderr=error};
                environment=Env(window);
                var wall=Stopwatch.StartNew();
                Check(CreateProcessW(exe,new StringBuilder(Quote(exe)+" "+Quote(program)),IntPtr.Zero,IntPtr.Zero,true,
                    0x4|0x20|0x400|0x08000000,environment,cwd,ref si,out pi),"CreateProcessW");
                result.ProcessId=pi.pid;
                if(affinity!=0) { Check(SetProcessAffinityMask(pi.process,new UIntPtr(affinity)),"SetProcessAffinityMask before resume"); result.AffinitySetBeforeResume=true; }
                UIntPtr actual,system; Check(GetProcessAffinityMask(pi.process,out actual,out system),"GetProcessAffinityMask");
                result.EffectiveAffinity=actual.ToUInt64();
                if(affinity!=0 && result.EffectiveAffinity!=affinity) throw new InvalidOperationException("Affinity verification failed");
                var running=Stopwatch.StartNew();
                if(ResumeThread(pi.thread)==0xffffffff) throw new Win32Exception(Marshal.GetLastWin32Error(),"ResumeThread");
                uint wait=WaitForSingleObject(pi.process,(uint)timeoutMs);
                if(wait==258) { result.TimedOut=true; Check(TerminateProcess(pi.process,124),"Terminate timed-out child"); Check(WaitForSingleObject(pi.process,10000)==0,"Wait after termination"); }
                else if(wait!=0) throw new Win32Exception(Marshal.GetLastWin32Error(),"WaitForSingleObject");
                running.Stop(); wall.Stop(); done=true;
                result.WallSeconds=wall.Elapsed.TotalSeconds; result.RunningWallSeconds=running.Elapsed.TotalSeconds;
                uint code; Check(GetExitCodeProcess(pi.process,out code),"GetExitCodeProcess"); result.ExitCode=code;
                FT created,exited,kernel,user; Check(GetProcessTimes(pi.process,out created,out exited,out kernel,out user),"GetProcessTimes");
                result.KernelSeconds=kernel.Value/10000000.0; result.UserSeconds=user.Value/10000000.0; result.CpuSeconds=result.KernelSeconds+result.UserSeconds;
                ulong cycles; result.CyclesAvailable=QueryProcessCycleTime(pi.process,out cycles); result.Cycles=cycles;
                if(!result.CyclesAvailable) result.CyclesError=Marshal.GetLastWin32Error();
                return result;
            } finally {
                if(pi.process!=IntPtr.Zero && !done) { TerminateProcess(pi.process,125); WaitForSingleObject(pi.process,10000); }
                Close(pi.thread); Close(pi.process); Close(input); Close(output); Close(error);
                if(environment!=IntPtr.Zero) Marshal.FreeHGlobal(environment);
            }
        }
    }
}
'@

$PackageRoot = (Resolve-Path -LiteralPath $PackageRoot).Path
$exe = Safe-Child $PackageRoot 'avl-basic.exe'
$manifestPath = Safe-Child $PackageRoot 'workloads.json'
$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
if ((Property-Or $manifest 'schema_version' 0) -ne 1) { throw 'Unsupported workloads.json schema_version (expected 1).' }
$exeHash = Check-Hash $exe ([string]$manifest.executable_sha256)
$workloads = @($manifest.workloads)
if ($workloads.Count -eq 0) { throw 'No workloads in manifest.' }
$names = @{}
foreach ($w in $workloads) {
    if ($w.name -notmatch '^[A-Za-z0-9_-]+$' -or $names.ContainsKey($w.name)) { throw "Invalid/duplicate workload name: $($w.name)" }
    $names[$w.name] = $true
    $source = Safe-Child $PackageRoot $w.program
    $null = Check-Hash $source ([string]$w.sha256)
    foreach ($asset in @(Property-Or $w 'files' @())) {
        $null = Check-Hash (Safe-Child (Split-Path -Parent $source) $asset.path) ([string]$asset.sha256)
    }
    $timeout = [double](Property-Or $w 'timeout_seconds' 120)
    if ($timeout -le 0 -or $timeout -gt 2147483 -or [double]::IsNaN($timeout)) { throw "Invalid timeout for $($w.name)" }
}
if ($Smoke) { $workloads = @($workloads[0]) }
if ([string]::IsNullOrWhiteSpace($Output)) {
    $label = $MachineLabel -replace '[^A-Za-z0-9_-]','_'
    $Output = Join-Path $env:LOCALAPPDATA ('AVL-BASIC-CpuComparison\results\{0}-{1}-{2}' -f $label,(Get-Date -Format 'yyyyMMdd-HHmmss'),([Guid]::NewGuid().ToString('N').Substring(0,8)))
}
$Output = [IO.Path]::GetFullPath($Output)
if (Test-Path -LiteralPath $Output) { throw "Output already exists; choose a new directory: $Output" }
$null = New-Item -ItemType Directory -Path $Output

$topology = @(); $topologyError = $null
try { $topology = @([AvlCrossMachine.Native]::Topology()) } catch { $topologyError = $_.Exception.Message }
$selection = [ordered]@{classification='unavailable'; reason='CPU-set topology unavailable; pinning skipped'; cpu_set=$null; affinity_mask=$null}
$affinity = [UInt64]0
if ($topology.Count -gt 0) {
    $groups = @($topology | Select-Object -ExpandProperty Group -Unique)
    if ($groups.Count -ne 1 -or $groups[0] -ne 0) {
        $selection.reason = 'Multiple/nonzero processor groups: this runner does not pin across groups.'
    } else {
        $eligible = @($topology | Where-Object { ($_.Flags -band 2) -eq 0 -or ($_.Flags -band 4) -ne 0 })
        $classes = @($topology | Select-Object -ExpandProperty EfficiencyClass -Unique | Sort-Object)
        if ($eligible.Count -gt 0) {
            $classification = 'fixed_unclassified'
            $reason = 'EfficiencyClass is not differentiated; the selected physical core is NOT identified as a P-core.'
            if ($classes.Count -gt 1) {
                $maximum = $classes[-1]
                $eligible = @($eligible | Where-Object EfficiencyClass -eq $maximum)
                $classification = 'p_core'
                $reason = 'Highest differentiated Windows EfficiencyClass; second physical core of that class where available; one logical processor.'
            }
            $cores = @($eligible | Sort-Object CoreIndex,LogicalProcessorIndex | Group-Object CoreIndex)
            if ($cores.Count -gt 0) {
                $core = $cores[0]; if ($cores.Count -gt 1) { $core = $cores[1] }
                $selected = @($core.Group | Sort-Object LogicalProcessorIndex)[0]
                $affinity = [UInt64]1 -shl [int]$selected.LogicalProcessorIndex
                $selection = [ordered]@{classification=$classification; reason=$reason; cpu_set=$selected; affinity_mask=('0x{0:X16}' -f $affinity)}
            }
        }
    }
}

$records = New-Object 'System.Collections.Generic.List[object]'
$baselines = @{}
$metadata = [ordered]@{
    schema_version=1; machine_label=$MachineLabel; started_utc=[DateTime]::UtcNow.ToString('o'); completed_utc=$null; status='running'; failure=$null
    package_root=$PackageRoot; output_directory=$Output; executable_sha256=$exeHash; executable_version=[Diagnostics.FileVersionInfo]::GetVersionInfo($exe).FileVersion
    manifest_sha256=(File-Hash $manifestPath); runner_sha256=(File-Hash $PSCommandPath); manifest=$manifest
    powershell_version=$PSVersionTable.PSVersion.ToString(); runs=$Runs; warmups=$Warmups; start_delay_seconds=$StartDelaySeconds; smoke=[bool]$Smoke; inherited_avl_environment=@(Get-ChildItem Env: | Where-Object Name -like 'AVL_BASIC_*' | Select-Object Name,Value)
    affinity_selection=$selection; topology=$topology; topology_error=$topologyError; inventory_before=(Read-Inventory); inventory_after=$null
    method=[ordered]@{
        priority='Normal'; affinity='Set on suspended child before its first instruction; automatic mode inherits the runner process affinity'
        timing='wall_s includes process creation through exit; running_wall_s starts just before ResumeThread; body_s is the BASIC marker. Body times exceeding wall_s + 0.02 are preserved but excluded from body/FPS statistics.'
        cycles='QueryProcessCycleTime is process accounting only: do not derive IPC or GHz from it'
        order='Workloads rotate by repetition; their order reverses on odd repetitions. Mode order also rotates by repetition and workload.'
        correctness='Same program/assets in every mode; normalized stdout, stderr and PNG checked across warmups, repetitions and modes'
        statistics='Measured runs only; median and unscaled median absolute deviation (MAD); no outliers removed'
    }
}
function Save-State {
    Save-Json (Join-Path $Output 'metadata.json') $metadata
    Save-Json (Join-Path $Output 'raw.json') ([ordered]@{metadata=$metadata; records=@($records.ToArray())})
}
function Save-Summary {
    $summary = @()
    foreach ($group in @($records | Where-Object { -not $_.warmup -and $_.valid } | Group-Object workload,mode)) {
        $items = @($group.Group)
        $bodyItems = @($items | Where-Object body_valid)
        $row = [ordered]@{workload=$items[0].workload; mode=$items[0].mode; n=$items.Count; body_n=$bodyItems.Count; wall_s=(Metric-Stats @($items | ForEach-Object wall_s)); running_wall_s=(Metric-Stats @($items | ForEach-Object running_wall_s)); body_s=(Metric-Stats @($bodyItems | ForEach-Object body_s)); cpu_s=(Metric-Stats @($items | ForEach-Object cpu_s)); correctness_sha256=$items[0].correctness_sha256}
        if ($null -ne $items[0].frames -and [double]$items[0].frames -gt 0) { $row.fps = Metric-Stats @($bodyItems | ForEach-Object { [double]$_.frames / $_.body_s }) }
        $summary += $row
    }
    Save-Json (Join-Path $Output 'summary.json') ([ordered]@{machine_label=$MachineLabel; status=$metadata.status; summary=$summary})
}

Save-State
Write-Host "Package verified. Results: $Output"
Write-Host "Affinity: $($selection.classification). $($selection.reason)"
Write-Host "Start in $StartDelaySeconds seconds. Leave other applications idle; disconnect streaming if testing without streaming."
try {
    if ($StartDelaySeconds -gt 0) { Start-Sleep -Seconds $StartDelaySeconds }
    $sequence = 0
    for ($round = 0; $round -lt ($Warmups + $Runs); $round++) {
        $warmup = $round -lt $Warmups
        $repetition = $round - $Warmups + 1
        if ($warmup) { $repetition = $round + 1 }
        $indices = @(0..($workloads.Count-1) | ForEach-Object { ($_ + $round) % $workloads.Count })
        if ($round % 2) { [Array]::Reverse($indices) }
        foreach ($wi in $indices) {
            $w = $workloads[$wi]
            $modes = @([pscustomobject]@{name='headless_auto'; window='0'; affinity=[UInt64]0})
            if ($affinity -ne 0) { $modes += [pscustomobject]@{name=('headless_'+$selection.classification); window='0'; affinity=$affinity} }
            if ([bool]$w.graphics) {
                $windowName = 'windowed_auto'; if ($affinity -ne 0) { $windowName = 'windowed_'+$selection.classification }
                $modes += [pscustomobject]@{name=$windowName; window='1'; affinity=$affinity}
            }
            $modeIndices = @(0..($modes.Count-1) | ForEach-Object { ($_ + $round + $wi) % $modes.Count })
            foreach ($mi in $modeIndices) {
                $mode = $modes[$mi]; $sequence++
                $runName = '{0:D4}-{1}-{2}' -f $sequence,$w.name,$mode.name
                $runDir = Join-Path $Output $runName
                $null = New-Item -ItemType Directory -Path $runDir
                $programSource = Safe-Child $PackageRoot $w.program
                $programPath = Join-Path $runDir ([IO.Path]::GetFileName($programSource))
                Copy-Item -LiteralPath $programSource -Destination $programPath
                $null = Check-Hash $programPath ([string]$w.sha256)
                foreach ($asset in @(Property-Or $w 'files' @())) {
                    $destination = Safe-Child $runDir $asset.path
                    $null = [IO.Directory]::CreateDirectory((Split-Path -Parent $destination))
                    Copy-Item -LiteralPath (Safe-Child (Split-Path -Parent $programSource) $asset.path) -Destination $destination
                    $null = Check-Hash $destination ([string]$asset.sha256)
                }
                $stdoutPath = Join-Path $runDir 'stdout.txt'; $stderrPath = Join-Path $runDir 'stderr.txt'
                $stage = 'measured'; if ($warmup) { $stage = 'warmup' }
                Write-Host ('[{0}] {1} {2}/{3}: {4} / {5}' -f $sequence,$stage,$repetition,(@($Runs,$Warmups)[[int]$warmup]),$w.name,$mode.name)
                $entry = [ordered]@{sequence=$sequence; workload=$w.name; mode=$mode.name; warmup=$warmup; repetition=$repetition; started_utc=[DateTime]::UtcNow.ToString('o'); program_sha256=$w.sha256; frames=(Property-Or $w 'frames' $null); directory=$runName; valid=$false; error=$null; window_environment=$mode.window}
                try {
                    $result = [AvlCrossMachine.Native]::Run($exe,$programPath,$runDir,$mode.window,$mode.affinity,[int]([double](Property-Or $w 'timeout_seconds' 120)*1000),$stdoutPath,$stderrPath)
                    $stdout = [IO.File]::ReadAllText($stdoutPath,$utf8).Replace("`r`n","`n")
                    $stderr = [IO.File]::ReadAllText($stderrPath,$utf8).Replace("`r`n","`n")
                    $entry.native=$result; $entry.wall_s=$result.WallSeconds; $entry.running_wall_s=$result.RunningWallSeconds; $entry.cpu_s=$result.CpuSeconds; $entry.stdout=$stdout; $entry.stderr=$stderr
                    if ($result.TimedOut) { throw "Timeout in $($w.name)/$($mode.name)" }
                    if ($result.ExitCode -ne 0) { throw "Child exited with code $($result.ExitCode); inspect $stderrPath and $stdoutPath" }
                    $pattern = '(?m)^__BENCH_SECONDS__\s*([0-9.eE+\-]+)\s*$'
                    $markers = [regex]::Matches($stdout,$pattern)
                    if ($markers.Count -ne 1) { throw "Expected exactly one timing marker, found $($markers.Count)" }
                    $body = [double]::Parse($markers[0].Groups[1].Value,$invariant)
                    if ($body -le 0 -or [double]::IsNaN($body) -or [double]::IsInfinity($body)) { throw 'Invalid BASIC body time.' }
                    $entry.body_s=$body
                    $entry.body_valid=($body -le $result.WallSeconds + 0.02)
                    if (-not $entry.body_valid) { Write-Warning 'BASIC TIME exceeds monotonic wall time; preserving raw value but excluding body/FPS statistics.' }
                    $normalized = [regex]::Replace($stdout,$pattern,'__BENCH_SECONDS__<timing>')
                    $stdoutHash = Text-Hash $normalized
                    $pngHash = $null
                    if ([bool]$w.graphics) {
                        $pngPath = Safe-Child $runDir ([string](Property-Or $w 'png_file' 'frame.png'))
                        $pngHash = File-Hash $pngPath
                        $pngHeader = [IO.File]::ReadAllBytes($pngPath)
                        if ($pngHeader.Length -lt 8 -or [BitConverter]::ToString($pngHeader,0,8) -ne '89-50-4E-47-0D-0A-1A-0A') { throw 'Expected a PNG output artifact.' }
                    }
                    $entry.normalized_stdout_sha256=$stdoutHash; $entry.png_sha256=$pngHash
                    $entry.correctness_sha256 = Text-Hash ($stdoutHash + ':' + (Text-Hash $stderr) + ':' + $pngHash)
                    $expectedStdout = [string](Property-Or $w 'expected_stdout_sha256' '')
                    $expectedPng = [string](Property-Or $w 'expected_png_sha256' '')
                    if ($expectedStdout -and $stdoutHash -ne $expectedStdout.ToLowerInvariant()) { throw 'Output differs from manifest expected_stdout_sha256.' }
                    if ($expectedPng -and $pngHash -ne $expectedPng.ToLowerInvariant()) { throw 'PNG differs from manifest expected_png_sha256.' }
                    if ($baselines.ContainsKey($w.name) -and $baselines[$w.name] -ne $entry.correctness_sha256) { throw "Determinism mismatch across repetitions/modes for $($w.name); timings must not be compared." }
                    $baselines[$w.name]=$entry.correctness_sha256
                    $entry.valid=$true
                    Write-Host ('    BASIC {0:F6} s; wall {1:F6} s; CPU {2:F6} s; output verified' -f $body,$result.WallSeconds,$result.CpuSeconds)
                } catch {
                    $entry.error=$_.Exception.Message
                    throw
                } finally {
                    $records.Add([pscustomobject]$entry)
                    Save-Json (Join-Path $runDir 'result.json') $entry
                    [IO.File]::AppendAllText((Join-Path $Output 'records.jsonl'),(($entry | ConvertTo-Json -Depth 12 -Compress)+[Environment]::NewLine),$utf8)
                    Save-State
                }
            }
        }
    }
    $metadata.status='complete'
} catch {
    $metadata.status='failed'; $metadata.failure=$_.Exception.Message
    [IO.File]::WriteAllText((Join-Path $Output 'failure.txt'),($_ | Out-String),$utf8)
    throw
} finally {
    $metadata.completed_utc=[DateTime]::UtcNow.ToString('o')
    $metadata.inventory_after=Read-Inventory
    Save-State
    Save-Summary
    Write-Host "Status: $($metadata.status). Results preserved in $Output"
}
