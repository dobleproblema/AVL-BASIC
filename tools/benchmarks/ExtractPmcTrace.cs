using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using Microsoft.Diagnostics.Tracing;
using Microsoft.Diagnostics.Tracing.Parsers.Kernel;

// Exact QPC endpoints are retained to match native records without rounding;
// TraceEvent marks this accessor discouraged in favour of relative milliseconds.
#pragma warning disable CS0618

// Offline, direct ETL reader. Do not insert TraceLog/ETLX conversion or Clone():
// those paths discard the PMC extended-data block used here.
public static class AvlPmcTrace
{
    const string ExpectedDllHash = "8a79a45ee040250d495c58b424fb054cdb3fe1d2678c59aea1c9196e5dd4ada7";
    static readonly CultureInfo Invariant = CultureInfo.InvariantCulture;
    static readonly JsonSerializerOptions JsonOptions = new JsonSerializerOptions { IncludeFields = true };
    static FieldInfo RecordField;

    public class Target
    {
        public int Pid;
        public string ExactImage;
        public string StartedUtc;
        public double WallSeconds;
        public string EffectiveAffinity, Label;
    }
    public class CounterName { public int Index; public string Name; public int? SourceId; }
    public class TargetCoverage
    {
        public int RequestIndex;
        public Target Request;
        public int[] MatchingProcessInstances;
        public string Status;
    }
    public class Life
    {
        public int Instance, Id, Pid;
        public long StartQpc = long.MinValue, EndQpc = long.MaxValue;
        public bool StartObserved, EndObserved, InterruptedByReuse;
        public string Image, CommandLine, StartUtc;
    }
    public class SwitchRow
    {
        public long Sequence, Qpc;
        public double TimeMs;
        public int Lp, OldTid, NewTid;
        public int? OldPid, NewPid, OldThreadInstance, NewThreadInstance;
        public string[] CounterValuesDecimal;
        public List<string> Flags = new List<string>();
    }
    public class Interval
    {
        public long StartSequence, EndSequence, StartQpc, EndQpc;
        public double DurationMs;
        public int Lp, Tid;
        public int? Pid, ThreadInstance, ProcessInstance;
        public bool IncludedInPartialSum;
        public string[] DeltaCountersDecimal;
        public List<string> Flags = new List<string>();
    }
    public class PartialSum
    {
        public int ProcessInstance, Pid, CounterCount;
        public long IncludedIntervals;
        public double IncludedScheduledMs;
        public string[] CounterSumsDecimal;
        public Dictionary<int, long> IncludedIntervalsByLp = new Dictionary<int, long>();
        public string Scope = "Complete observed intervals only; not a whole-process total or user-mode-only count.";
        internal ulong[] Values;
    }
    public class Report
    {
        public string Status, Etl, EtlSha256, TraceEventVersion, TraceEventSha256;
        public string SwitchRowsFile, IntervalsFile;
        public string SessionStartUtc, SessionEndUtc;
        public int EventsLost, NumberOfProcessors;
        public long BuffersLost;
        public long AllSwitchRows, AllPmcRows, ExportedSwitchRows, TargetIntervals, IncludedIntervals;
        public bool CounterOrderVerified;
        public string CounterOrderEvidence;
        public CounterName[] CounterNames;
        public List<Life> TargetProcesses = new List<Life>();
        public List<Life> TargetThreads = new List<Life>();
        public List<TargetCoverage> TargetCoverage = new List<TargetCoverage>();
        public List<PartialSum> PartialSums = new List<PartialSum>();
        public Dictionary<string, long> Exclusions = new Dictionary<string, long>();
        public List<string> Warnings = new List<string>();
        public string Scope = "Offline CSwitch PMC extraction. No IPC/GHz or whole-process totals are inferred. Kernel/interrupt work may contribute.";
    }

    static string Sha(string path)
    {
        using (var stream = File.OpenRead(path))
        using (var hash = SHA256.Create())
            return BitConverter.ToString(hash.ComputeHash(stream)).Replace("-", "").ToLowerInvariant();
    }
    static void ValidateApi(Report report)
    {
        if (IntPtr.Size != 8) throw new InvalidOperationException("The PMC reader requires an x64 host.");
        var assembly = typeof(TraceEvent).Assembly;
        report.TraceEventVersion = assembly.FullName;
        report.TraceEventSha256 = Sha(assembly.Location);
        if (assembly.GetName().Version != new Version(3, 0, 6, 0) || report.TraceEventSha256 != ExpectedDllHash)
            throw new InvalidOperationException("Unsupported TraceEvent DLL: validate its private record API before reading PMC pointers.");
        RecordField = typeof(TraceEvent).GetField("eventRecord", BindingFlags.Instance | BindingFlags.NonPublic);
        if (RecordField == null || !RecordField.FieldType.IsPointer)
            throw new InvalidOperationException("TraceEvent.eventRecord pointer not found.");
        var record = RecordField.FieldType.GetElementType();
        var items = record.GetField("ExtendedData").FieldType.GetElementType();
        if (Marshal.SizeOf(record) != 112 || Marshal.OffsetOf(record, "ExtendedDataCount").ToInt32() != 84 ||
            Marshal.OffsetOf(record, "ExtendedData").ToInt32() != 88 || Marshal.SizeOf(items) != 16 ||
            Marshal.OffsetOf(items, "ExtType").ToInt32() != 2 || Marshal.OffsetOf(items, "DataSize").ToInt32() != 6 ||
            Marshal.OffsetOf(items, "DataPtr").ToInt32() != 8)
            throw new InvalidOperationException("Native EVENT_RECORD/extended-data layout does not match the verified x64 layout.");
    }

    static unsafe ulong[] ReadCounters(TraceEvent data, List<string> flags)
    {
        var record = (IntPtr)Pointer.Unbox(RecordField.GetValue(data));
        if (record == IntPtr.Zero) { flags.Add("null_event_record"); return null; }
        int count = unchecked((ushort)Marshal.ReadInt16(record, 84));
        var items = Marshal.ReadIntPtr(record, 88);
        if (count == 0) { flags.Add("missing_pmc"); return null; }
        if (items == IntPtr.Zero || count > 1024) { flags.Add("invalid_extended_data"); return null; }
        ulong[] values = null;
        for (int i = 0; i < count; i++)
        {
            var item = IntPtr.Add(items, i * 16);
            if (unchecked((ushort)Marshal.ReadInt16(item, 2)) != 8) continue;
            if (values != null) { flags.Add("duplicate_pmc_block"); return null; }
            int bytes = unchecked((ushort)Marshal.ReadInt16(item, 6));
            var pointer = new IntPtr(Marshal.ReadInt64(item, 8));
            if (pointer == IntPtr.Zero || bytes == 0 || bytes % 8 != 0 || bytes > 128 * 8)
            { flags.Add("invalid_pmc_block"); return null; }
            values = new ulong[bytes / 8];
            for (int j = 0; j < values.Length; j++) values[j] = unchecked((ulong)Marshal.ReadInt64(pointer, j * 8));
        }
        if (values == null) flags.Add("missing_pmc");
        return values;
    }
    static string[] Decimals(ulong[] values)
    { return values == null ? null : values.Select(v => v.ToString(Invariant)).ToArray(); }
    static long BuffersLost(ETWTraceEventSource source)
    {
        // This is another version-pinned private API. EventsLost exposes only
        // LogfileHeader.EventsLost, so inspect the separate BuffersLost field too.
        var logsField = typeof(ETWTraceEventSource).GetField("logFiles", BindingFlags.Instance | BindingFlags.NonPublic);
        if (logsField == null) throw new InvalidOperationException("Cannot inspect ETL buffer-loss metadata.");
        var logs = (Array)logsField.GetValue(source);
        long lost = 0;
        foreach (object log in logs)
        {
            var header = log.GetType().GetField("LogfileHeader").GetValue(log);
            lost = checked(lost + Convert.ToInt64(header.GetType().GetField("BuffersLost").GetValue(header), Invariant));
        }
        return lost;
    }

    sealed class Lifetimes
    {
        public readonly List<Life> All = new List<Life>();
        readonly Dictionary<int, List<Life>> ById = new Dictionary<int, List<Life>>();
        readonly Dictionary<int, Life> Active = new Dictionary<int, Life>();
        public void Event(int id, int pid, long qpc, bool start, bool stop, bool rundown, string image, string command, string utc)
        {
            Life life;
            Active.TryGetValue(id, out life);
            if (start && !rundown && life != null && life.StartObserved)
            {
                life.EndQpc = qpc; life.InterruptedByReuse = true;
                Active.Remove(id); life = null;
            }
            if (life == null)
            {
                life = new Life { Instance = All.Count, Id = id, Pid = pid };
                All.Add(life);
                List<Life> list;
                if (!ById.TryGetValue(id, out list)) ById[id] = list = new List<Life>();
                list.Add(life); Active[id] = life;
            }
            if (start && !rundown) { life.StartQpc = qpc; life.StartObserved = true; life.StartUtc = utc; }
            if (!String.IsNullOrWhiteSpace(image)) life.Image = image;
            if (!String.IsNullOrWhiteSpace(command)) life.CommandLine = command;
            if (stop && !rundown) { life.EndQpc = qpc; life.EndObserved = true; Active.Remove(id); }
        }
        public Life Resolve(int id, long qpc)
        {
            List<Life> list;
            if (!ById.TryGetValue(id, out list)) return null;
            Life found = null;
            foreach (var life in list)
                if (life.StartQpc <= qpc && qpc <= life.EndQpc)
                { if (found != null) return null; found = life; }
            return found;
        }
    }
    static bool Matches(Life process, Target target)
    {
        if (process.Id != target.Pid) return false;
        if (!String.IsNullOrWhiteSpace(target.ExactImage))
        {
            string path = target.ExactImage, cmd = (process.CommandLine ?? "").TrimStart();
            bool image = String.Equals(process.Image, path, StringComparison.OrdinalIgnoreCase);
            bool command = cmd.StartsWith("\"" + path + "\"", StringComparison.OrdinalIgnoreCase) ||
                           cmd.StartsWith(path + " ", StringComparison.OrdinalIgnoreCase) ||
                           String.Equals(cmd, path, StringComparison.OrdinalIgnoreCase);
            if (!image && !command) return false;
        }
        DateTimeOffset targetStart, actualStart;
        if (!String.IsNullOrWhiteSpace(target.StartedUtc) && !String.IsNullOrWhiteSpace(process.StartUtc) &&
            DateTimeOffset.TryParse(target.StartedUtc, Invariant, DateTimeStyles.AssumeUniversal, out targetStart) &&
            DateTimeOffset.TryParse(process.StartUtc, Invariant, DateTimeStyles.AssumeUniversal, out actualStart))
        {
            // Runner startup surrounds process creation. This only disambiguates PID reuse;
            // it does not define a BASIC computation region or trim counter intervals.
            if (actualStart < targetStart.AddSeconds(-2) || actualStart > targetStart.AddSeconds(Math.Max(target.WallSeconds, 0) + 2)) return false;
        }
        return true;
    }
    static void Increment(Dictionary<string, long> counts, string key)
    { long value; counts.TryGetValue(key, out value); counts[key] = value + 1; }

    public static Report Read(string etl, string outputDirectory, Target[] targets, CounterName[] names, bool orderVerified, string orderEvidence)
    {
        var report = new Report { Etl = Path.GetFullPath(etl), CounterOrderVerified = orderVerified,
            CounterOrderEvidence = orderEvidence, CounterNames = orderVerified ? names : null };
        ValidateApi(report);
        if (orderVerified && (names == null || names.Length == 0 || String.IsNullOrWhiteSpace(orderEvidence) ||
            names.Where((n, i) => n == null || n.Index != i || String.IsNullOrWhiteSpace(n.Name)).Any() ||
            names.Select(n => n.Name).Distinct(StringComparer.OrdinalIgnoreCase).Count() != names.Length))
            throw new ArgumentException("Verified counter order needs evidence and unique, contiguous indexed names.");
        report.EtlSha256 = Sha(etl);
        var processes = new Lifetimes(); var threads = new Lifetimes();
        using (var source = new ETWTraceEventSource(etl))
        {
            Action<ProcessTraceData, bool, bool, bool> process = (d, start, stop, rundown) =>
                processes.Event(d.ProcessID, d.ProcessID, d.TimeStampQPC, start, stop, rundown, d.ImageFileName, d.CommandLine, d.TimeStamp.ToUniversalTime().ToString("O"));
            Action<ThreadTraceData, bool, bool, bool> thread = (d, start, stop, rundown) =>
                threads.Event(d.ThreadID, d.ProcessID, d.TimeStampQPC, start, stop, rundown, null, null, d.TimeStamp.ToUniversalTime().ToString("O"));
            source.Kernel.ProcessStart += d => process(d, true, false, false);
            source.Kernel.ProcessStop += d => process(d, false, true, false);
            source.Kernel.ProcessDCStart += d => process(d, false, false, true);
            source.Kernel.ProcessDCStop += d => process(d, false, false, true);
            source.Kernel.ThreadStart += d => thread(d, true, false, false);
            source.Kernel.ThreadStop += d => thread(d, false, true, false);
            source.Kernel.ThreadDCStart += d => thread(d, false, false, true);
            source.Kernel.ThreadDCStop += d => thread(d, false, false, true);
            source.Process();
            report.EventsLost = source.EventsLost;
            report.BuffersLost = BuffersLost(source);
            report.NumberOfProcessors = source.NumberOfProcessors;
            report.SessionStartUtc = source.SessionStartTime.ToUniversalTime().ToString("O");
            report.SessionEndUtc = source.SessionEndTime.ToUniversalTime().ToString("O");
        }
        bool explicitTargets = targets != null && targets.Length != 0;
        if (explicitTargets)
        {
            var matched = new HashSet<int>();
            for (int i = 0; i < targets.Length; i++)
            {
                var candidates = processes.All.Where(p => Matches(p, targets[i])).ToList();
                string status = candidates.Count == 1 ? "matched" : candidates.Count == 0 ? "unmatched_target" : "ambiguous_target";
                report.TargetCoverage.Add(new TargetCoverage { RequestIndex = i, Request = targets[i],
                    MatchingProcessInstances = candidates.Select(p => p.Instance).ToArray(), Status = status });
                if (candidates.Count == 1) matched.Add(candidates[0].Instance);
                else { Increment(report.Exclusions, status); report.Warnings.Add(status + ": request " + i + ", PID " + targets[i].Pid + "; inspect TargetCoverage."); }
            }
            report.TargetProcesses = processes.All.Where(p => matched.Contains(p.Instance)).ToList();
        }
        else report.TargetProcesses = processes.All.Where(p =>
            String.Equals(Path.GetFileName(p.Image ?? ""), "avl-basic.exe", StringComparison.OrdinalIgnoreCase)).ToList();
        var selectedProcesses = new HashSet<int>(report.TargetProcesses.Select(p => p.Instance));
        Func<Life, long, Life> selectedProcess = (thread, qpc) => {
            if (thread == null) return null;
            var process = processes.Resolve(thread.Pid, qpc);
            return process != null && selectedProcesses.Contains(process.Instance) ? process : null;
        };
        if (!explicitTargets) report.Warnings.Add("No PID metadata supplied; selected process lifetimes whose image basename is avl-basic.exe.");
        if (!orderVerified) report.Warnings.Add("Counter order is unverified: values and sums retain numeric indices only; no labelled IPC or rates are produced.");
        if (report.EventsLost != 0 || report.BuffersLost != 0) report.Warnings.Add("The ETL reports lost events/buffers; partial sums are disabled.");
        report.SwitchRowsFile = Path.Combine(outputDirectory, "switches.ndjson");
        report.IntervalsFile = Path.Combine(outputDirectory, "intervals.ndjson");
        var exported = new HashSet<long>(); var selectedThreads = new HashSet<int>();
        var previous = new Dictionary<int, SwitchRow>(); var sums = new Dictionary<int, PartialSum>();
        using (var rows = new StreamWriter(report.SwitchRowsFile, false, new UTF8Encoding(false)))
        using (var intervals = new StreamWriter(report.IntervalsFile, false, new UTF8Encoding(false)))
        using (var source = new ETWTraceEventSource(etl))
        {
            Action<SwitchRow> emit = row => { if (exported.Add(row.Sequence)) { rows.WriteLine(JsonSerializer.Serialize(row, JsonOptions)); report.ExportedSwitchRows++; } };
            source.Kernel.ThreadCSwitch += data => {
                var row = new SwitchRow { Sequence = ++report.AllSwitchRows, Qpc = data.TimeStampQPC,
                    TimeMs = data.TimeStampRelativeMSec, Lp = data.ProcessorNumber, OldTid = data.OldThreadID, NewTid = data.NewThreadID };
                var raw = ReadCounters(data, row.Flags); row.CounterValuesDecimal = Decimals(raw);
                if (raw != null) report.AllPmcRows++;
                if (orderVerified && raw != null && raw.Length != names.Length) row.Flags.Add("counter_order_length_mismatch");
                var oldThread = threads.Resolve(row.OldTid, row.Qpc); var newThread = threads.Resolve(row.NewTid, row.Qpc);
                row.OldPid = oldThread == null ? (int?)null : oldThread.Pid; row.NewPid = newThread == null ? (int?)null : newThread.Pid;
                row.OldThreadInstance = oldThread == null ? (int?)null : oldThread.Instance;
                row.NewThreadInstance = newThread == null ? (int?)null : newThread.Instance;
                bool selectedOld = selectedProcess(oldThread, row.Qpc) != null, selectedNew = selectedProcess(newThread, row.Qpc) != null;
                SwitchRow prior;
                if (previous.TryGetValue(row.Lp, out prior))
                {
                    var priorThread = threads.Resolve(prior.NewTid, prior.Qpc);
                    var processAtStart = selectedProcess(priorThread, prior.Qpc);
                    var processAtEnd = selectedProcess(oldThread, row.Qpc);
                    if (processAtStart != null || processAtEnd != null)
                    {
                        emit(prior); emit(row); report.TargetIntervals++;
                        var interval = new Interval { StartSequence = prior.Sequence, EndSequence = row.Sequence,
                            StartQpc = prior.Qpc, EndQpc = row.Qpc, DurationMs = row.TimeMs - prior.TimeMs, Lp = row.Lp, Tid = prior.NewTid,
                            Pid = priorThread == null ? (int?)null : priorThread.Pid,
                            ThreadInstance = priorThread == null ? (int?)null : priorThread.Instance,
                            ProcessInstance = processAtStart == null ? (int?)null : processAtStart.Instance };
                        if (priorThread != null) selectedThreads.Add(priorThread.Instance);
                        if (oldThread != null) selectedThreads.Add(oldThread.Instance);
                        if (prior.NewTid != row.OldTid) interval.Flags.Add("thread_discontinuity");
                        if (priorThread == null || oldThread == null || priorThread.Instance != oldThread.Instance) interval.Flags.Add("thread_lifetime_unresolved_or_changed");
                        if (processAtStart == null || processAtEnd == null || processAtStart.Instance != processAtEnd.Instance) interval.Flags.Add("process_lifetime_unresolved_or_changed");
                        if (priorThread != null && !priorThread.StartObserved) interval.Flags.Add("thread_start_not_observed");
                        if (processAtStart != null && !processAtStart.StartObserved) interval.Flags.Add("process_start_not_observed");
                        if (priorThread != null && priorThread.InterruptedByReuse) interval.Flags.Add("thread_reuse_without_stop");
                        if (processAtStart != null && processAtStart.InterruptedByReuse) interval.Flags.Add("process_reuse_without_stop");
                        if (row.Qpc <= prior.Qpc || interval.DurationMs <= 0) interval.Flags.Add("nonpositive_interval");
                        foreach (var flag in prior.Flags) interval.Flags.Add("start_" + flag);
                        foreach (var flag in row.Flags) interval.Flags.Add("end_" + flag);
                        ulong[] delta = null;
                        if (prior.CounterValuesDecimal != null && raw != null)
                        {
                            if (prior.CounterValuesDecimal.Length != raw.Length) interval.Flags.Add("counter_count_changed");
                            else
                            {
                                delta = new ulong[raw.Length];
                                for (int i = 0; i < raw.Length; i++)
                                {
                                    ulong before = UInt64.Parse(prior.CounterValuesDecimal[i], Invariant);
                                    if (raw[i] < before) { interval.Flags.Add("counter_decreased_index_" + i); delta = null; break; }
                                    delta[i] = raw[i] - before;
                                }
                            }
                        }
                        interval.DeltaCountersDecimal = Decimals(delta);
                        if (report.EventsLost != 0) interval.Flags.Add("trace_events_lost");
                        if (report.BuffersLost != 0) interval.Flags.Add("trace_buffers_lost");
                        if (delta != null && interval.Flags.Count == 0)
                        {
                            PartialSum sum;
                            if (!sums.TryGetValue(processAtStart.Instance, out sum)) sums[processAtStart.Instance] = sum = new PartialSum {
                                ProcessInstance = processAtStart.Instance, Pid = processAtStart.Id, CounterCount = delta.Length, Values = new ulong[delta.Length] };
                            if (sum.CounterCount != delta.Length) interval.Flags.Add("process_counter_count_changed");
                            else
                            {
                                var added = new ulong[delta.Length]; bool overflow = false;
                                for (int i = 0; i < delta.Length; i++) { try { added[i] = checked(sum.Values[i] + delta[i]); } catch (OverflowException) { overflow = true; } }
                                if (overflow) interval.Flags.Add("partial_sum_overflow");
                                else { sum.Values = added; sum.IncludedIntervals++; sum.IncludedScheduledMs += interval.DurationMs;
                                    long lpCount; sum.IncludedIntervalsByLp.TryGetValue(row.Lp, out lpCount); sum.IncludedIntervalsByLp[row.Lp] = lpCount + 1;
                                    interval.IncludedInPartialSum = true; report.IncludedIntervals++; }
                            }
                        }
                        foreach (var flag in interval.Flags.Distinct()) Increment(report.Exclusions, flag);
                        intervals.WriteLine(JsonSerializer.Serialize(interval, JsonOptions));
                    }
                }
                else if (selectedOld) { row.Flags.Add("first_switch_missing_interval_start"); Increment(report.Exclusions, "first_switch_missing_interval_start"); }
                if (selectedOld || selectedNew) emit(row);
                previous[row.Lp] = row;
            };
            source.Process();
            if (source.EventsLost != report.EventsLost || BuffersLost(source) != report.BuffersLost)
                throw new InvalidDataException("The two ETL passes disagree about lost events/buffers.");
        }
        foreach (var last in previous.Values)
            if (selectedProcess(threads.Resolve(last.NewTid, last.Qpc), last.Qpc) != null)
                Increment(report.Exclusions, "last_switch_missing_interval_end");
        report.TargetThreads = threads.All.Where(t => selectedThreads.Contains(t.Instance)).ToList();
        foreach (var sum in sums.Values) { sum.CounterSumsDecimal = Decimals(sum.Values); report.PartialSums.Add(sum); }
        if (report.AllPmcRows == 0) { report.Status = "rejected_no_pmc_data"; report.Warnings.Add("No CSwitch event carried a valid PMC block. A CPU sampling ETL is not a counter trace."); }
        else if (report.TargetProcesses.Count == 0) report.Status = "rejected_no_matching_target";
        else if (report.IncludedIntervals == 0) report.Status = "rejected_no_valid_target_intervals";
        else report.Status = report.Exclusions.Count == 0 ? "partial_intervals_extracted" : "partial_intervals_extracted_with_exclusions";
        report.Warnings.Add("Sums cover accepted scheduled intervals only. Missing starts/ends and trace boundaries must be reviewed before interpreting coverage.");
        return report;
    }
}
