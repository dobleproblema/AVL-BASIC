using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using Microsoft.Diagnostics.Symbols;
using Microsoft.Diagnostics.Tracing;
using Microsoft.Diagnostics.Tracing.Etlx;
using Microsoft.Diagnostics.Tracing.Parsers.Kernel;

// Offline ETL reader. One SampledProfile event is one observation; raw Count is
// retained separately. Inclusive counts deduplicate recursive function frames.
public static class AvlCpuTrace
{
    public class ProcessRow { public int Id; public string Image; public string CommandLine; public double StartMs; public double EndMs; }
    public class FrameRow { public int Id; public string Address; public string Module; public string Function; public string Source; public int Line; public bool Resolved; }
    public class SampleRow { public double TimeMs; public int ThreadId; public int RawCount; public int[] Frames; public bool HasStack; }
    public class AggregateRow { public string Name; public long Exclusive; public long Inclusive; public double ExclusivePct; public double InclusivePct; }
    public class Report
    {
        public string Etl; public string Etlx; public ProcessRow Process;
        public List<ProcessRow> Processes = new List<ProcessRow>();
        public string SymbolPath; public string TraceEventVersion;
        public long Samples; public long SamplesWithStack; public long UnknownLeafSamples; public long SamplesWithUnknownFrame;
        public long NonProcessEventsSkipped; public long CountSum; public int EventsLost; public bool Truncated; public double SampleIntervalMs;
        public double FromProcessMs; public double ToProcessMs;
        public List<string> Warnings = new List<string>();
        public List<FrameRow> Frames = new List<FrameRow>();
        public List<SampleRow> Observations = new List<SampleRow>();
        public List<AggregateRow> Functions = new List<AggregateRow>();
        public List<AggregateRow> Lines = new List<AggregateRow>();
    }

    static AggregateRow GetRow(Dictionary<string, AggregateRow> rows, string key)
    {
        AggregateRow row;
        if (!rows.TryGetValue(key, out row)) { row = new AggregateRow { Name = key }; rows.Add(key, row); }
        return row;
    }
    static string FunctionKey(FrameRow f) { return f.Module + "!" + f.Function; }
    static string LineKey(FrameRow f) { return (String.IsNullOrEmpty(f.Source) ? "[unknown source]" : f.Source) + ":" + f.Line + " " + FunctionKey(f); }
    static bool MatchesImage(TraceProcess process, string expected)
    {
        if (String.Equals(process.ImageFileName, expected, StringComparison.OrdinalIgnoreCase)) return true;
        if (!Path.IsPathRooted(expected) || !String.Equals(process.ImageFileName, Path.GetFileName(expected), StringComparison.OrdinalIgnoreCase)) return false;
        // Kernel ProcessStart often stores only the image basename. Require
        // the command line's executable token to match the requested full path.
        var command = (process.CommandLine ?? "").TrimStart();
        return command.StartsWith("\"" + expected + "\"", StringComparison.OrdinalIgnoreCase)
            || command.StartsWith(expected + " ", StringComparison.OrdinalIgnoreCase)
            || String.Equals(command, expected, StringComparison.OrdinalIgnoreCase);
    }
    static void CountFrames(List<FrameRow> frames, Dictionary<string, AggregateRow> functions, Dictionary<string, AggregateRow> lines)
    {
        GetRow(functions, FunctionKey(frames[0])).Exclusive++;
        GetRow(lines, LineKey(frames[0])).Exclusive++;
        var seenFunctions = new HashSet<string>(); var seenLines = new HashSet<string>();
        foreach (var f in frames)
        {
            var fk = FunctionKey(f); var lk = LineKey(f);
            if (seenFunctions.Add(fk)) GetRow(functions, fk).Inclusive++;
            if (seenLines.Add(lk)) GetRow(lines, lk).Inclusive++;
        }
    }

    public static Report Read(string etl, string outputDirectory, int processId, string exactImage, string symbolPath, double fromProcessMs, double toProcessMs)
    {
        var report = new Report { Etl = Path.GetFullPath(etl), SymbolPath = symbolPath, TraceEventVersion = typeof(TraceLog).Assembly.FullName,
            FromProcessMs = fromProcessMs, ToProcessMs = toProcessMs };
        var etlx = Path.Combine(outputDirectory, "capture.etlx"); report.Etlx = etlx;
        using (var conversionLog = File.CreateText(Path.Combine(outputDirectory, "conversion.log")))
        {
            var options = new TraceLogOptions { ConversionLog = conversionLog, LocalSymbolsOnly = true, ContinueOnError = false };
            TraceLog.CreateFromEventTraceLogFile(report.Etl, etlx, options);
        }
        using (var log = new TraceLog(etlx))
        using (var symbolLog = File.CreateText(Path.Combine(outputDirectory, "symbols.log")))
        using (var reader = new SymbolReader(symbolLog, symbolPath))
        {
            // No symbol server added. Only the explicitly supplied local path is
            // used, and no JIT/NGen commands are spawned by the reader.
            reader.Options = SymbolReaderOptions.CacheOnly | SymbolReaderOptions.NoNGenSymbolCreation;
            reader.SecurityCheck = path => symbolPath.Split(';').Any(dir => !String.IsNullOrEmpty(dir) && Path.GetFullPath(path).StartsWith(Path.GetFullPath(dir).TrimEnd(Path.DirectorySeparatorChar) + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase));
            report.EventsLost = log.EventsLost; report.Truncated = log.Truncated; report.SampleIntervalMs = log.SampleProfileInterval.TotalMilliseconds;
            var selected = new List<TraceProcess>();
            foreach (var p in log.Processes)
            {
                // Keep unrelated process metadata out of the exported artifact.
                if ((processId != 0 && p.ProcessID == processId) || String.Equals(p.ImageFileName, Path.GetFileName(exactImage), StringComparison.OrdinalIgnoreCase))
                    report.Processes.Add(new ProcessRow { Id = p.ProcessID, Image = p.ImageFileName, CommandLine = p.CommandLine, StartMs = p.StartTimeRelativeMsec, EndMs = p.EndTimeRelativeMsec });
                if ((processId == 0 || p.ProcessID == processId) && MatchesImage(p, exactImage)) selected.Add(p);
            }
            if (selected.Count != 1)
            {
                File.WriteAllLines(Path.Combine(outputDirectory, "processes.txt"), report.Processes.Select(p => p.Id + "\t" + p.StartMs + "\t" + p.EndMs + "\t" + p.Image + "\t" + p.CommandLine));
                throw new InvalidOperationException("Exact process selection found " + selected.Count + " matches. See processes.txt; supply the exact recorded image and PID (PID 0 permits one exact-image match).");
            }
            var process = selected[0];
            report.Process = report.Processes.First(p => p.Id == process.ProcessID && p.StartMs == process.StartTimeRelativeMsec);
            var resolvedModules = new HashSet<ModuleFileIndex>();
            var frameCache = new Dictionary<string, FrameRow>();
            var functions = new Dictionary<string, AggregateRow>(); var lines = new Dictionary<string, AggregateRow>();

            Func<TraceCodeAddress, ulong, FrameRow> frameFor = (code, rawAddress) =>
            {
                var key = code == null ? "raw:" + rawAddress : "code:" + (int)code.CodeAddressIndex;
                FrameRow frame;
                if (frameCache.TryGetValue(key, out frame)) return frame;
                var module = code == null ? null : code.ModuleFile;
                if (module != null && resolvedModules.Add(module.ModuleFileIndex))
                {
                    try { log.CodeAddresses.LookupSymbolsForModule(reader, module); }
                    catch (Exception e) { report.Warnings.Add("Symbols " + module.FilePath + ": " + e.Message); }
                }
                frame = new FrameRow { Id = report.Frames.Count, Address = "0x" + rawAddress.ToString("x"), Module = module == null ? "[unknown module]" : module.FilePath, Function = "[unknown function]", Source = "", Line = 0, Resolved = false };
                if (code != null && code.Method != null && !String.IsNullOrEmpty(code.FullMethodName))
                {
                    frame.Function = code.FullMethodName; frame.Resolved = true;
                    try { var source = code.GetSourceLine(reader); if (source != null) { frame.Source = source.SourceFile.BuildTimeFilePath; frame.Line = source.LineNumber; } }
                    catch (Exception e) { report.Warnings.Add("Source " + frame.Address + ": " + e.Message); }
                }
                report.Frames.Add(frame); frameCache.Add(key, frame); return frame;
            };

            foreach (var sample in process.EventsInProcess.ByEventType<SampledProfileTraceData>())
            {
                double timeInProcess = sample.TimeStampRelativeMSec - process.StartTimeRelativeMsec;
                if (timeInProcess < fromProcessMs || timeInProcess >= toProcessMs) continue;
                if (sample.NonProcess) { report.NonProcessEventsSkipped++; continue; }
                var frames = new List<FrameRow>();
                var ipIndex = log.GetCodeAddressIndexAtEvent(sample.InstructionPointer, sample);
                TraceCodeAddress ipCode = ipIndex == CodeAddressIndex.Invalid ? null : log.CodeAddresses[ipIndex];
                frames.Add(frameFor(ipCode, sample.InstructionPointer));
                var stack = log.GetCallStackForEvent(sample); bool hasStack = stack != null;
                var seenStackNodes = new HashSet<CallStackIndex>();
                while (stack != null && seenStackNodes.Add(stack.CallStackIndex) && frames.Count < 512)
                {
                    var code = stack.CodeAddress;
                    if (code != null && (frames.Count > 1 || code.Address != sample.InstructionPointer)) frames.Add(frameFor(code, code.Address));
                    stack = stack.Caller;
                }
                report.Samples++; report.CountSum += sample.Count;
                if (hasStack) report.SamplesWithStack++;
                if (!frames[0].Resolved) report.UnknownLeafSamples++;
                if (frames.Any(f => !f.Resolved)) report.SamplesWithUnknownFrame++;
                report.Observations.Add(new SampleRow { TimeMs = timeInProcess, ThreadId = sample.ThreadID, RawCount = sample.Count, Frames = frames.Select(f => f.Id).ToArray(), HasStack = hasStack });
                CountFrames(frames, functions, lines);
            }
            if (report.Samples == 0) report.Warnings.Add("No kernel SampledProfile events matched. The capture may use a different sampling payload/provider or contain no CPU samples.");
            foreach (var row in functions.Values.Concat(lines.Values))
            {
                row.ExclusivePct = report.Samples == 0 ? 0 : 100.0 * row.Exclusive / report.Samples;
                row.InclusivePct = report.Samples == 0 ? 0 : 100.0 * row.Inclusive / report.Samples;
            }
            report.Functions = functions.Values.OrderByDescending(r => r.Exclusive).ThenByDescending(r => r.Inclusive).ToList();
            report.Lines = lines.Values.OrderByDescending(r => r.Exclusive).ThenByDescending(r => r.Inclusive).ToList();
            return report;
        }
    }
}
