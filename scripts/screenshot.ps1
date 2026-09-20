param(
    [Parameter(Mandatory = $true)][string]$Executable,
    [Parameter(Mandatory = $true)][string]$TitlePrefix,
    [Parameter(Mandatory = $true)][string]$Name,
    [int]$SettleSeconds = 2,
    [string]$FocusName = '',
    [int]$TabAfter = 0,
    [string]$Arguments = ''
)
# Launches an example, waits for its window, saves target/<Name>.png and exits it.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class ShotNative {
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr window, IntPtr dc, uint flags);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr window, uint message, IntPtr wParam, IntPtr lParam);
    [DllImport("user32.dll")] public static extern uint MapVirtualKey(uint code, uint mapType);
}
'@

$info = New-Object System.Diagnostics.ProcessStartInfo
$info.FileName = (Resolve-Path -LiteralPath $Executable).Path
$info.UseShellExecute = $false
$info.Arguments = $Arguments
$process = [System.Diagnostics.Process]::Start($info)
try {
    $window = $null
    for ($attempt = 0; $attempt -lt 100 -and $null -eq $window; $attempt++) {
        $windows = [System.Windows.Automation.AutomationElement]::RootElement.FindAll(
            [System.Windows.Automation.TreeScope]::Children, [System.Windows.Automation.Condition]::TrueCondition)
        foreach ($candidate in $windows) {
            if ($candidate.Current.Name.StartsWith($TitlePrefix)) { $window = $candidate; break }
        }
        Start-Sleep -Milliseconds 100
    }
    if ($null -eq $window) { throw "Window not found: $TitlePrefix" }
    Start-Sleep -Seconds $SettleSeconds
    foreach ($target in ($FocusName -split '\|' | Where-Object { $_ })) {
        # Focusing a control scrolls it into view through the framework's focus
        # reveal; off-screen controls join the accessibility tree once nearby.
        $condition = New-Object System.Windows.Automation.PropertyCondition -ArgumentList @(
            [System.Windows.Automation.AutomationElement]::NameProperty, $target)
        $element = $null
        for ($attempt = 0; $attempt -lt 50 -and $null -eq $element; $attempt++) {
            $element = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
            Start-Sleep -Milliseconds 100
        }
        if ($null -eq $element) { throw "Accessible element not found: $target" }
        $element.SetFocus()
        Start-Sleep -Seconds 1
    }
    if ($TabAfter -gt 0) {
        # Tab moves focus onward; the framework scrolls the focused control into view.
        $handle = [IntPtr]$window.Current.NativeWindowHandle
        $scan = [ShotNative]::MapVirtualKey(0x09, 0)
        $flags = ([long]$scan -shl 16) -bor 1
        for ($press = 0; $press -lt $TabAfter; $press++) {
            $null = [ShotNative]::PostMessage($handle, 0x0100, [IntPtr]0x09, [IntPtr]$flags)
            Start-Sleep -Milliseconds 80
            $null = [ShotNative]::PostMessage($handle, 0x0101, [IntPtr]0x09, [IntPtr]($flags -bor 3221225472L))
            Start-Sleep -Milliseconds 200
        }
        Start-Sleep -Seconds 1
    }
    $rectangle = $window.Current.BoundingRectangle
    $bitmap = New-Object System.Drawing.Bitmap -ArgumentList @([int]$rectangle.Width, [int]$rectangle.Height)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $dc = $graphics.GetHdc()
    try {
        if (-not [ShotNative]::PrintWindow([IntPtr]$window.Current.NativeWindowHandle, $dc, 2)) { throw 'Window capture failed' }
    } finally { $graphics.ReleaseHdc($dc); $graphics.Dispose() }
    $imagePath = Join-Path (Get-Location) "target/$Name.png"
    $bitmap.Save($imagePath, [System.Drawing.Imaging.ImageFormat]::Png)
    $bitmap.Dispose()
    Write-Output "Screenshot: $imagePath"
} finally {
    if (-not $process.HasExited) { $process.Kill(); $process.WaitForExit() }
}
