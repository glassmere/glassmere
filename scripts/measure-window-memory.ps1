param(
    [Parameter(Mandatory = $true)] [string]$Executable,
    [ValidateSet('auto', 'vulkan', 'dx12', 'gl')]
    [string]$Backend = 'auto',
    [switch]$TraceStages,
    [switch]$Stress,
    [ValidateRange(2, 100000)] [int]$StressFrames = 600,
    [ValidateSet('default', 'memory', 'compact')]
    [string]$MemoryProfile = 'default',
    [ValidateSet(1, 2)] [int]$FrameLatency = 1,
    [ValidateRange(16, 1000000)] [int]$BindingLimit = 4096,
    [int]$SettleSeconds = 4
)
$ErrorActionPreference = 'Stop'

# Native counters distinguish private resident RAM from shared pages and commit.
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class GuiMemoryCounters {
    [StructLayout(LayoutKind.Sequential)]
    public struct Counters {
        public uint cb, PageFaultCount;
        public UIntPtr PeakWorkingSetSize, WorkingSetSize;
        public UIntPtr QuotaPeakPagedPoolUsage, QuotaPagedPoolUsage;
        public UIntPtr QuotaPeakNonPagedPoolUsage, QuotaNonPagedPoolUsage;
        public UIntPtr PagefileUsage, PeakPagefileUsage, PrivateUsage;
        public UIntPtr PrivateWorkingSetSize;
        public ulong SharedCommitUsage;
    }
    [DllImport("psapi.dll", SetLastError = true)]
    private static extern bool GetProcessMemoryInfo(IntPtr process, ref Counters counters, uint size);
    public static Counters Read(IntPtr process) {
        var counters = new Counters();
        counters.cb = (uint)Marshal.SizeOf(typeof(Counters));
        if (!GetProcessMemoryInfo(process, ref counters, counters.cb))
            throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
        return counters;
    }
}
'@

function Read-ProbeMemory([System.Diagnostics.Process]$Process, [string]$Stage) {
    $Process.Refresh()
    $memory = [GuiMemoryCounters]::Read($Process.Handle)
    [pscustomobject]@{
        Backend = $Backend
        MemoryProfile = $MemoryProfile
        FrameLatency = $FrameLatency
        BindingLimit = $BindingLimit
        Stage = $Stage
        ProcessId = $Process.Id
        PrivateWorkingSetMiB = [math]::Round($memory.PrivateWorkingSetSize.ToUInt64() / 1MB, 2)
        TotalWorkingSetMiB = [math]::Round($memory.WorkingSetSize.ToUInt64() / 1MB, 2)
        PrivateCommitMiB = [math]::Round($memory.PrivateUsage.ToUInt64() / 1MB, 2)
        CpuMilliseconds = [math]::Round($Process.TotalProcessorTime.TotalMilliseconds, 2)
    }
}

$probeInfo = New-Object System.Diagnostics.ProcessStartInfo
$probeInfo.FileName = (Resolve-Path -LiteralPath $Executable).Path
$probeInfo.UseShellExecute = $false
$probeInfo.CreateNoWindow = $true
$probeInfo.WindowStyle = [System.Diagnostics.ProcessWindowStyle]::Hidden
if ($Backend -eq 'auto') { $probeInfo.EnvironmentVariables.Remove('WGPU_BACKEND') }
else { $probeInfo.EnvironmentVariables['WGPU_BACKEND'] = $Backend }
if ($MemoryProfile -eq 'default') { $probeInfo.EnvironmentVariables.Remove('RUST_GUI_MEMORY_PROFILE') }
else { $probeInfo.EnvironmentVariables['RUST_GUI_MEMORY_PROFILE'] = $MemoryProfile }
$probeInfo.EnvironmentVariables['RUST_GUI_FRAME_LATENCY'] = [string]$FrameLatency
$probeInfo.EnvironmentVariables['RUST_GUI_BINDING_LIMIT'] = [string]$BindingLimit
if ($TraceStages) {
    $probeInfo.EnvironmentVariables['RUST_GUI_MEMORY_TRACE'] = '1'
    $probeInfo.RedirectStandardInput = $true
    $probeInfo.RedirectStandardOutput = $true
    $probeInfo.Arguments = if ($Stress) { "--stress --frames $StressFrames" } else { '--smoke' }
} else { $probeInfo.EnvironmentVariables.Remove('RUST_GUI_MEMORY_TRACE') }
$windowProbe = [System.Diagnostics.Process]::Start($probeInfo)
try {
    if ($TraceStages) {
        $checkpointCount = 0
        while ($null -ne ($probeLine = $windowProbe.StandardOutput.ReadLine())) {
            if ($probeLine.StartsWith('MEMORY_STAGE ')) {
                Read-ProbeMemory $windowProbe $probeLine.Substring(13)
                $checkpointCount += 1
                $windowProbe.StandardInput.WriteLine()
                $windowProbe.StandardInput.Flush()
            }
        }
        $windowProbe.WaitForExit()
        if ($windowProbe.ExitCode -ne 0) { throw "Window probe failed: $($windowProbe.ExitCode)" }
        if ($checkpointCount -eq 0) { throw 'No checkpoints: build the counter with --features diagnostics' }
        return
    }
    Start-Sleep -Seconds $SettleSeconds
    if ($windowProbe.HasExited) { throw "Window probe exited before measurement" }
    Read-ProbeMemory $windowProbe 'idle'
    $cpuBefore = $windowProbe.TotalProcessorTime.TotalMilliseconds
    Start-Sleep -Seconds 2
    $windowProbe.Refresh()
    Write-Information "Idle CPU over 2 seconds: $($windowProbe.TotalProcessorTime.TotalMilliseconds - $cpuBefore) ms" -InformationAction Continue
} finally {
    if (-not $windowProbe.HasExited) {
        $null = $windowProbe.CloseMainWindow()
        if (-not $windowProbe.WaitForExit(5000)) { $windowProbe.Kill() }
    }
    $windowProbe.Dispose()
}
