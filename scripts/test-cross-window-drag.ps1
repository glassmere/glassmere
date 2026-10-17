param([string]$Executable = 'target/debug/examples/application.exe')
# Drags a ticket out of the application workbench and into the details
# window with real mouse input, which is the only way to exercise the
# desktop's carrying of a drag between two of its own windows: the window the
# drag started in keeps the pointer while a button is held, so the second
# window never sees the move itself.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class DragNative {
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, int dx, int dy, uint data, UIntPtr extra);
    [DllImport("user32.dll")] public static extern int GetSystemMetrics(int index);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr window, IntPtr after, int x, int y, int cx, int cy, uint flags);
}
'@

$MOVE = 0x0001; $LEFTDOWN = 0x0002; $LEFTUP = 0x0004; $ABSOLUTE = 0x8000

function Move-Pointer([double]$X, [double]$Y, [uint32]$Extra = 0) {
    $width = [DragNative]::GetSystemMetrics(0)
    $height = [DragNative]::GetSystemMetrics(1)
    $nx = [int](($X * 65535) / ($width - 1))
    $ny = [int](($Y * 65535) / ($height - 1))
    [DragNative]::mouse_event($MOVE -bor $ABSOLUTE -bor $Extra, $nx, $ny, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 40
}
function Wait-Window([string]$Prefix) {
    for ($attempt = 0; $attempt -lt 100; $attempt++) {
        foreach ($window in [System.Windows.Automation.AutomationElement]::RootElement.FindAll(
                [System.Windows.Automation.TreeScope]::Children,
                [System.Windows.Automation.Condition]::TrueCondition)) {
            if ($window.Current.Name.StartsWith($Prefix)) { return $window }
        }
        Start-Sleep -Milliseconds 100
    }
    throw "Window not found: $Prefix"
}
function Find-Element($Root, [string]$Name) {
    $condition = New-Object System.Windows.Automation.PropertyCondition -ArgumentList @(
        [System.Windows.Automation.AutomationElement]::NameProperty, $Name)
    for ($attempt = 0; $attempt -lt 60; $attempt++) {
        $element = $Root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
        if ($null -ne $element) { return $element }
        Start-Sleep -Milliseconds 100
    }
    throw "Accessible element not found: $Name"
}
function Get-Center($Element) {
    $rectangle = $Element.Current.BoundingRectangle
    @{ X = $rectangle.X + $rectangle.Width / 2; Y = $rectangle.Y + $rectangle.Height / 2 }
}

$info = New-Object System.Diagnostics.ProcessStartInfo
$info.FileName = (Resolve-Path -LiteralPath $Executable).Path
$info.UseShellExecute = $false
$info.RedirectStandardError = $true
$process = [System.Diagnostics.Process]::Start($info)
$errors = $process.StandardError.ReadToEndAsync()
try {
    $main = Wait-Window 'Seaglass / Application workbench'
    Start-Sleep -Seconds 2
    # The details window is opened from the toolbar.
    $open = Find-Element $main 'Details window   Ctrl+D'
    ([System.Windows.Automation.InvokePattern]$open.GetCurrentPattern(
        [System.Windows.Automation.InvokePattern]::Pattern)).Invoke()
    $details = Wait-Window 'Seaglass / Ticket details'
    Start-Sleep -Seconds 1
    # Put the two windows side by side: a drop that lands where they overlap
    # would not say which window took it.
    $screen = [DragNative]::GetSystemMetrics(0)
    $null = [DragNative]::SetWindowPos([IntPtr]$main.Current.NativeWindowHandle, [IntPtr]::Zero, 0, 0, 0, 0, 0x0001 -bor 0x0004)
    $null = [DragNative]::SetWindowPos([IntPtr]$details.Current.NativeWindowHandle, [IntPtr]::Zero, $screen - 700, 120, 0, 0, 0x0001 -bor 0x0004)
    Start-Sleep -Seconds 1
    # The drop target is not a control in the accessibility tree; its own
    # label is, and sits inside it, which is enough to aim at.
    # A ticket has to be selected before the source can be dragged.
    $rows = $main.FindAll([System.Windows.Automation.TreeScope]::Descendants,
        (New-Object System.Windows.Automation.PropertyCondition -ArgumentList @(
            [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
            [System.Windows.Automation.ControlType]::ListItem)))
    $row = $null
    foreach ($candidate in $rows) {
        if ($candidate.Current.Name.StartsWith('Ticket #')) { $row = $candidate; break }
    }
    if ($null -eq $row) { throw 'No ticket row found' }
    $rowCenter = Get-Center $row
    Move-Pointer $rowCenter.X $rowCenter.Y
    [DragNative]::mouse_event($LEFTDOWN, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 80
    [DragNative]::mouse_event($LEFTUP, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 400

    # Selecting a ticket adds lines to the details window, so where the
    # target sits is only known afterwards.
    $target = Find-Element $details 'Drag a ticket here to link it'
    $drop = Get-Center $target
    $mainRect = $main.Current.BoundingRectangle
    if ($drop.X -ge $mainRect.X -and $drop.X -lt $mainRect.X + $mainRect.Width -and
        $drop.Y -ge $mainRect.Y -and $drop.Y -lt $mainRect.Y + $mainRect.Height) {
        Write-Output 'SKIP the windows overlap over the drop target'
        return
    }
    $source = $null
    for ($attempt = 0; $attempt -lt 40 -and $null -eq $source; $attempt++) {
        foreach ($element in $main.FindAll([System.Windows.Automation.TreeScope]::Descendants,
                [System.Windows.Automation.Condition]::TrueCondition)) {
            if ($element.Current.Name.StartsWith('Drag ticket #')) { $source = $element }
        }
        if ($null -eq $source) { Start-Sleep -Milliseconds 100 }
    }
    if ($null -eq $source) { throw 'No ticket is selected, so nothing can be dragged' }
    $from = Get-Center $source
    Move-Pointer $from.X $from.Y
    [DragNative]::mouse_event($LEFTDOWN, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 120
    # Several steps, because a drag has to pass the threshold and the desktop
    # hands the payload over on the move that leaves the window.
    for ($step = 1; $step -le 12; $step++) {
        $x = $from.X + ($drop.X - $from.X) * $step / 12
        $y = $from.Y + ($drop.Y - $from.Y) * $step / 12
        Move-Pointer $x $y
    }
    Start-Sleep -Milliseconds 200
    [DragNative]::mouse_event($LEFTUP, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 600

    $label = ''
    foreach ($element in $details.FindAll([System.Windows.Automation.TreeScope]::Descendants,
            [System.Windows.Automation.Condition]::TrueCondition)) {
        if ($element.Current.Name.StartsWith('Linked: #')) { $label = $element.Current.Name }
    }
    Write-Output "LINKED $label"
    if ($label -notmatch 'Linked: #') {
        Write-Output "FAILED the drop did not reach the details window"
    } else {
        Write-Output 'CROSS_WINDOW_DRAG_OK'
    }
} finally {
    if (-not $process.HasExited) { $process.Kill(); $process.WaitForExit() }
    $text = $errors.Result
    if ($text) { $text | Out-File -FilePath 'target/cross-window-drag.log' -Encoding utf8 }
}
