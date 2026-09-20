param([string]$Executable = 'target/release/examples/theming.exe')
# Opens the theming example, switches themes through UI Automation, edits the
# stylesheet on disk to exercise hot reload and saves screenshots under target/.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class ThemingNative {
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr window, IntPtr dc, uint flags);
}
'@

function Wait-Window([string]$Prefix) {
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
function Invoke-Element($Element) {
    ([System.Windows.Automation.InvokePattern]$Element.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)).Invoke()
}
function Save-Window($Window, [string]$Name) {
    $rectangle = $Window.Current.BoundingRectangle
    $bitmap = New-Object System.Drawing.Bitmap -ArgumentList @([int]$rectangle.Width, [int]$rectangle.Height)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $dc = $graphics.GetHdc()
    try {
        if (-not [ThemingNative]::PrintWindow([IntPtr]$Window.Current.NativeWindowHandle, $dc, 2)) { throw 'Window capture failed' }
    } finally { $graphics.ReleaseHdc($dc); $graphics.Dispose() }
    try {
        $imagePath = Join-Path (Get-Location) "target/$Name.png"
        $bitmap.Save($imagePath, [System.Drawing.Imaging.ImageFormat]::Png)
        Write-Output "Screenshot: $imagePath"
    } finally { $bitmap.Dispose() }
}

$stylesheet = Join-Path (Get-Location) 'crates/rust-gui/examples/theming.style'
$original = [System.IO.File]::ReadAllText($stylesheet)
$info = New-Object System.Diagnostics.ProcessStartInfo
$info.FileName = (Resolve-Path -LiteralPath $Executable).Path
$info.UseShellExecute = $false
$process = [System.Diagnostics.Process]::Start($info)
try {
    $window = Wait-Window 'Rust GUI / Theming'
    Start-Sleep -Seconds 2
    Save-Window $window 'theming-initial'
    Invoke-Element (Wait-Element $window 'Light theme')
    Start-Sleep -Seconds 1
    Save-Window $window 'theming-light'
    # Hot reload: retint the primary class and the accent token, then restore.
    $edited = $original.Replace('#f59e0b', '#2563eb').Replace('#fbbf24', '#3b82f6').Replace('#d97706', '#1d4ed8')
    [System.IO.File]::WriteAllText($stylesheet, $edited)
    Start-Sleep -Seconds 2
    Save-Window $window 'theming-reloaded'
    Invoke-Element (Wait-Element $window 'Dark theme')
    Start-Sleep -Seconds 1
    Save-Window $window 'theming-dark-reloaded'
    Write-Output 'THEMING_OK'
} finally {
    [System.IO.File]::WriteAllText($stylesheet, $original)
    if (-not $process.HasExited) { $process.Kill(); $process.WaitForExit() }
}
