param(
    [string]$Executable = 'target/release/examples/workbench.exe',
    [switch]$WakeupOnly,
    [switch]$Screenshot
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class WorkbenchKeys {
    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool PostMessage(IntPtr window, uint message, IntPtr wParam, IntPtr lParam);
    [DllImport("user32.dll")]
    public static extern bool PrintWindow(IntPtr window, IntPtr dc, uint flags);
}
'@

function Start-Probe([string]$Arguments) {
    $info = New-Object System.Diagnostics.ProcessStartInfo
    $info.FileName = (Resolve-Path -LiteralPath $Executable).Path
    $info.Arguments = $Arguments
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.WindowStyle = [System.Diagnostics.ProcessWindowStyle]::Hidden
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $info.EnvironmentVariables.Remove('GLASSMERE_MEMORY_TRACE')
    [System.Diagnostics.Process]::Start($info)
}

function Close-Probe([System.Diagnostics.Process]$Process) {
    if (-not $Process.HasExited) {
        $null = $Process.CloseMainWindow()
        if (-not $Process.WaitForExit(5000)) { $Process.Kill() }
    }
    $Process.Dispose()
}

$wakeProbe = Start-Probe '--smoke'
try {
    if (-not $wakeProbe.WaitForExit(20000)) { throw 'Worker wakeup smoke test timed out' }
    $output = $wakeProbe.StandardOutput.ReadToEnd()
    $errors = $wakeProbe.StandardError.ReadToEnd()
    if ($wakeProbe.ExitCode -ne 0 -or $output -notmatch 'WAKE_SMOKE_OK' -or $output -notmatch 'WAKE_UPDATE 0' -or $output -notmatch 'WAKE_CLOSED_OK') {
        throw "Worker wakeup failed: $output $errors"
    }
    Write-Output $output.Trim()
    Write-Output 'PASS: worker updates wake the idle event loop and the handle requests shutdown'
} finally { Close-Probe $wakeProbe }
if ($WakeupOnly) { return }

function Wait-Element($Root, [string]$Name) {
    $condition = New-Object System.Windows.Automation.PropertyCondition -ArgumentList @(
        [System.Windows.Automation.AutomationElement]::NameProperty, $Name)
    for ($attempt = 0; $attempt -lt 80; $attempt++) {
        $element = $Root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
        if ($null -ne $element) { return $element }
        Start-Sleep -Milliseconds 100
    }
    Write-Information "Root: $($Root.Current.Name), handle=$($Root.Current.NativeWindowHandle)" -InformationAction Continue
    $Root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition) |
        Select-Object -First 35 | ForEach-Object { Write-Information "Child: $($_.Current.Name) $($_.Current.ControlType.ProgrammaticName)" -InformationAction Continue }
    throw "Accessible element not found: $Name"
}

function Invoke-Element($Element) {
    $pattern = $Element.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
    ([System.Windows.Automation.InvokePattern]$pattern).Invoke()
}

$accessibilityProbe = Start-Probe ''
try {
    $processCondition = New-Object System.Windows.Automation.PropertyCondition -ArgumentList @(
        [System.Windows.Automation.AutomationElement]::ProcessIdProperty, [int]$accessibilityProbe.Id)
    $titleCondition = New-Object System.Windows.Automation.PropertyCondition -ArgumentList @(
        [System.Windows.Automation.AutomationElement]::NameProperty, 'Glassmere / Architecture workbench')
    $windowCondition = New-Object System.Windows.Automation.AndCondition -ArgumentList @($processCondition, $titleCondition)
    $window = $null
    for ($attempt = 0; $attempt -lt 100 -and $null -eq $window; $attempt++) {
        $window = [System.Windows.Automation.AutomationElement]::RootElement.FindFirst(
            [System.Windows.Automation.TreeScope]::Children, $windowCondition)
        if ($null -eq $window) { Start-Sleep -Milliseconds 100 }
    }
    if ($null -eq $window) { throw 'Workbench native window not found' }
    $left = Wait-Element $window 'Increase left'
    $right = Wait-Element $window 'Increase right'
    $leftId = [string]::Join(',', $left.GetRuntimeId())
    Invoke-Element $left
    $null = Wait-Element $window 'left count: 1'
    $null = Wait-Element $window 'right count: 10'
    if ([string]::Join(',', (Wait-Element $window 'Increase left').GetRuntimeId()) -ne $leftId) {
        throw 'Accessibility identity changed after an update'
    }
    $disabled = Wait-Element $window 'Unavailable action'
    if ($disabled.Current.IsEnabled) { throw 'Disabled button is exposed as enabled' }
    Invoke-Element (Wait-Element $window 'Custom action: 0')
    $null = Wait-Element $window 'Custom action: 1'
    # AccessKit exposes visible clipped children and adjacent rows, allowing
    # assistive technology to progressively reveal content via ScrollIntoView.
    $next = Wait-Element $window 'Row 006'
    $before = $next.Current.BoundingRectangle
    $scroll = $next.GetCurrentPattern([System.Windows.Automation.ScrollItemPattern]::Pattern)
    ([System.Windows.Automation.ScrollItemPattern]$scroll).ScrollIntoView()
    $visible = $false
    for ($attempt = 0; $attempt -lt 60; $attempt++) {
        $after = $next.Current.BoundingRectangle
        if ($after.Top -lt $before.Top -and $after.Bottom -le $window.Current.BoundingRectangle.Bottom) {
            $visible = $true
            break
        }
        Start-Sleep -Milliseconds 100
    }
    if (-not $visible) { throw 'ScrollIntoView did not reveal the adjacent offscreen row' }
    $focused = $false
    for ($attempt = 0; $attempt -lt 30; $attempt++) {
        $next.SetFocus()
        Start-Sleep -Milliseconds 100
        if ($next.Current.HasKeyboardFocus) { $focused = $true; break }
    }
    if (-not $focused) { throw 'Keyboard focus was not established on the scroll row' }
    # Send End only to this test's window, without global keyboard injection.
    $handle = [IntPtr]$window.Current.NativeWindowHandle
    $null = [WorkbenchKeys]::PostMessage($handle, 0x0100, [IntPtr]0x23, [IntPtr]0x014F0001)
    Start-Sleep -Milliseconds 80
    $null = [WorkbenchKeys]::PostMessage($handle, 0x0101, [IntPtr]0x23, [IntPtr]3243171841L)
    $last = Wait-Element $window 'Row 249'
    $last.SetFocus()
    Invoke-Element $last
    $null = Wait-Element $window 'Selected row: 249'
    if ($Screenshot) {
        Add-Type -AssemblyName System.Drawing
        $rectangle = $window.Current.BoundingRectangle
        $bitmap = New-Object System.Drawing.Bitmap -ArgumentList @([int]$rectangle.Width, [int]$rectangle.Height)
        $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
        $dc = $graphics.GetHdc()
        try {
            if (-not [WorkbenchKeys]::PrintWindow($handle, $dc, 2)) { throw 'Window capture failed' }
        } finally { $graphics.ReleaseHdc($dc); $graphics.Dispose() }
        try {
            $imagePath = Join-Path (Get-Location) 'target/workbench.png'
            $bitmap.Save($imagePath, [System.Drawing.Imaging.ImageFormat]::Png)
            Write-Output "Screenshot: $imagePath"
        } finally { $bitmap.Dispose() }
    }
    Write-Output 'PASS: native UI Automation tree, stable IDs, labels, disabled state, built-in/custom activation, ScrollIntoView, and keyboard scrolling'
} finally { Close-Probe $accessibilityProbe }
