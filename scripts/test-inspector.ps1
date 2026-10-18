param([string]$Executable = 'target/release/examples/application.exe')
# Opens the application workbench with GLASSMERE_INSPECT=1, drives the inspector
# window through UI Automation and saves screenshots under target/.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class InspectorNative {
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr window, IntPtr dc, uint flags);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr window, uint message, IntPtr wParam, IntPtr lParam);
    [DllImport("user32.dll")] public static extern uint MapVirtualKey(uint code, uint mapType);
}
'@

function Wait-Window([string]$Prefix) {
    # Titles contain non-ASCII separators; match by prefix so the script stays ASCII.
    for ($attempt = 0; $attempt -lt 100; $attempt++) {
        $windows = [System.Windows.Automation.AutomationElement]::RootElement.FindAll(
            [System.Windows.Automation.TreeScope]::Children, [System.Windows.Automation.Condition]::TrueCondition)
        foreach ($window in $windows) {
            if ($window.Current.Name.StartsWith($Prefix)) { return $window }
        }
        Start-Sleep -Milliseconds 100
    }
    throw "Window not found: $Prefix"
}
function Wait-Element($Root, [string]$Name) {
    $condition = New-Object System.Windows.Automation.PropertyCondition -ArgumentList @(
        [System.Windows.Automation.AutomationElement]::NameProperty, $Name)
    for ($attempt = 0; $attempt -lt 70; $attempt++) {
        $element = $Root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
        if ($null -ne $element) { return $element }
        Start-Sleep -Milliseconds 100
    }
    throw "Accessible element not found: $Name"
}
function Send-Key([IntPtr]$Handle, [uint32]$Key) {
    $scan = [InspectorNative]::MapVirtualKey($Key, 0)
    $flags = ([long]$scan -shl 16) -bor 1
    if ($Key -ge 0x21 -and $Key -le 0x28) { $flags = $flags -bor 0x01000000 }
    $null = [InspectorNative]::PostMessage($Handle, 0x0100, [IntPtr]$Key, [IntPtr]$flags)
    Start-Sleep -Milliseconds 80
    $null = [InspectorNative]::PostMessage($Handle, 0x0101, [IntPtr]$Key, [IntPtr]($flags -bor 3221225472L))
}
function Save-Window($Window, [string]$Name) {
    $rectangle = $Window.Current.BoundingRectangle
    $bitmap = New-Object System.Drawing.Bitmap -ArgumentList @([int]$rectangle.Width, [int]$rectangle.Height)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $dc = $graphics.GetHdc()
    try {
        if (-not [InspectorNative]::PrintWindow([IntPtr]$Window.Current.NativeWindowHandle, $dc, 2)) { throw 'Window capture failed' }
    } finally { $graphics.ReleaseHdc($dc); $graphics.Dispose() }
    try {
        $imagePath = Join-Path (Get-Location) "target/$Name.png"
        $bitmap.Save($imagePath, [System.Drawing.Imaging.ImageFormat]::Png)
        Write-Output "Screenshot: $imagePath"
    } finally { $bitmap.Dispose() }
}

$info = New-Object System.Diagnostics.ProcessStartInfo
$info.FileName = (Resolve-Path -LiteralPath $Executable).Path
$info.UseShellExecute = $false
$info.EnvironmentVariables['GLASSMERE_INSPECT'] = '1'
$process = [System.Diagnostics.Process]::Start($info)
try {
    $target = Wait-Window 'Glassmere / Application workbench'
    $inspector = Wait-Window 'Inspector'
    Start-Sleep -Seconds 2
    Save-Window $inspector 'inspector-tree'
    $handle = [IntPtr]$inspector.Current.NativeWindowHandle
    $nodes = Wait-Element $inspector 'Nodes'
    $nodes.SetFocus()
    Start-Sleep -Milliseconds 200
    # Home, then Down x3 selects the page scroll container's first children.
    Send-Key $handle 0x24
    Send-Key $handle 0x28
    Send-Key $handle 0x28
    Send-Key $handle 0x28
    $repaints = Wait-Element $inspector 'Repaints'
    ([System.Windows.Automation.TogglePattern]$repaints.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern)).Toggle()
    Start-Sleep -Seconds 1
    Save-Window $inspector 'inspector-details'
    Save-Window $target 'inspector-target'
    $frames = Wait-Element $inspector 'Frames'
    ([System.Windows.Automation.InvokePattern]$frames.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)).Invoke()
    Start-Sleep -Seconds 1
    Save-Window $inspector 'inspector-frames'
    $events = Wait-Element $inspector 'Events'
    ([System.Windows.Automation.InvokePattern]$events.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)).Invoke()
    Start-Sleep -Seconds 1
    Save-Window $inspector 'inspector-events'
    Write-Output 'INSPECTOR_OK'
} finally {
    if (-not $process.HasExited) { $process.Kill(); $process.WaitForExit() }
}
