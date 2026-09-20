param(
    [ValidateSet('all', 'vulkan', 'dx12', 'gl', 'fonts')]
    [string]$Mode = 'all',
    [ValidateSet('performance', 'memory')]
    [string]$MemoryHint = 'performance',
    [string]$Executable = (Join-Path $PSScriptRoot '../target/release/examples/memory_probe.exe')
)
$ErrorActionPreference = 'Stop'
$probeInfo = New-Object System.Diagnostics.ProcessStartInfo
$probeInfo.FileName = (Resolve-Path -LiteralPath $Executable).Path
$probeInfo.Arguments = "$Mode $MemoryHint"
$probeInfo.UseShellExecute = $false
$probeInfo.CreateNoWindow = $true
$probeInfo.RedirectStandardOutput = $true
$probeInfo.RedirectStandardInput = $true
$probeProcess = [System.Diagnostics.Process]::Start($probeInfo)
try {
    while ($null -ne ($probeLine = $probeProcess.StandardOutput.ReadLine())) {
        if ($probeLine.StartsWith('MEMORY_STAGE ')) {
            $probeProcess.Refresh()
            [pscustomobject]@{
                Mode = $Mode
                MemoryHint = $MemoryHint
                Stage = $probeLine.Substring(13)
                WorkingSetMiB = [math]::Round($probeProcess.WorkingSet64 / 1MB, 2)
                PrivateCommitMiB = [math]::Round($probeProcess.PrivateMemorySize64 / 1MB, 2)
            }
            $probeProcess.StandardInput.WriteLine()
            $probeProcess.StandardInput.Flush()
        }
    }
    $probeProcess.WaitForExit()
    if ($probeProcess.ExitCode -ne 0) { throw "Memory probe failed with exit code $($probeProcess.ExitCode)" }
} finally {
    if (-not $probeProcess.HasExited) { $probeProcess.Kill() }
    $probeProcess.Dispose()
}
