param([string]$Executable = 'target/debug/examples/titlebar.exe')
# Drives a window that draws its own title bar with real mouse input: moves
# it by the strip, maximizes it by pressing the strip twice, resizes it by
# the corner, and minimizes it with the button. Nothing here is the system's
# frame, so every one of these is the framework's own doing.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class BarNative {
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, int dx, int dy, uint data, UIntPtr extra);
    [DllImport("user32.dll")] public static extern int GetSystemMetrics(int index);
    [DllImport("user32.dll")] public static extern IntPtr SendMessageW(IntPtr window, uint message, IntPtr wparam, IntPtr lparam);
}
'@

$MOVE = 0x0001; $LEFTDOWN = 0x0002; $LEFTUP = 0x0004; $ABSOLUTE = 0x8000

function Move-Pointer([double]$X, [double]$Y) {
    $width = [BarNative]::GetSystemMetrics(0)
    $height = [BarNative]::GetSystemMetrics(1)
    [BarNative]::mouse_event($MOVE -bor $ABSOLUTE, [int](($X * 65535) / ($width - 1)), [int](($Y * 65535) / ($height - 1)), 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 50
}
function Press-Pointer { [BarNative]::mouse_event($LEFTDOWN, 0, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 90 }
function Release-Pointer { [BarNative]::mouse_event($LEFTUP, 0, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 250 }
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
function Get-Rect($Window) { $Window.Current.BoundingRectangle }
function Get-State($Window) {
    $pattern = $Window.GetCurrentPattern([System.Windows.Automation.WindowPattern]::Pattern)
    ([System.Windows.Automation.WindowPattern]$pattern).Current.WindowVisualState
}

$process = [System.Diagnostics.Process]::Start((Resolve-Path -LiteralPath $Executable).Path)
try {
    $window = Wait-Window 'Glassmere / Title bar'
    Start-Sleep -Seconds 2
    $start = Get-Rect $window

    # The strip between the menu and the window buttons moves the window.
    $grab = @{ X = $start.X + $start.Width * 0.5; Y = $start.Y + 20 }
    Move-Pointer $grab.X $grab.Y
    Press-Pointer
    for ($step = 1; $step -le 6; $step++) { Move-Pointer ($grab.X + 20 * $step) ($grab.Y + 15 * $step) }
    Release-Pointer
    $moved = Get-Rect $window
    Write-Output ("MOVED by {0},{1}" -f [int]($moved.X - $start.X), [int]($moved.Y - $start.Y))
    if ([Math]::Abs($moved.X - $start.X) -lt 40 -or [Math]::Abs($moved.Y - $start.Y) -lt 30) {
        throw 'the window did not follow the pointer'
    }

    # Pressing the strip twice maximizes it, and twice again restores it.
    $grab = @{ X = $moved.X + $moved.Width * 0.5; Y = $moved.Y + 20 }
    Move-Pointer $grab.X $grab.Y
    Press-Pointer; Release-Pointer
    Press-Pointer; Release-Pointer
    Start-Sleep -Milliseconds 400
    Write-Output ("STATE " + (Get-State $window))
    if ((Get-State $window) -ne 'Maximized') { throw 'the window did not maximize' }
    $maximized = Get-Rect $window
    $grab = @{ X = $maximized.X + $maximized.Width * 0.5; Y = $maximized.Y + 20 }
    Move-Pointer $grab.X $grab.Y
    Press-Pointer; Release-Pointer
    Press-Pointer; Release-Pointer
    Start-Sleep -Milliseconds 400
    if ((Get-State $window) -ne 'Normal') { throw 'the window did not restore' }

    # The bottom-right corner resizes it.
    $before = Get-Rect $window
    $corner = @{ X = $before.X + $before.Width - 3; Y = $before.Y + $before.Height - 3 }
    Move-Pointer $corner.X $corner.Y
    Press-Pointer
    for ($step = 1; $step -le 6; $step++) { Move-Pointer ($corner.X - 20 * $step) ($corner.Y - 12 * $step) }
    Release-Pointer
    $resized = Get-Rect $window
    Write-Output ("RESIZED by {0},{1}" -f [int]($resized.Width - $before.Width), [int]($resized.Height - $before.Height))
    if ($resized.Width -ge $before.Width - 40 -or $resized.Height -ge $before.Height - 20) {
        throw 'the corner did not resize the window'
    }

    # Windows asks the window itself where its maximize button is, and only
    # an answer of HTMAXBUTTON (9) makes it offer the snap layouts there.
    $handle = [IntPtr]$window.Current.NativeWindowHandle
    foreach ($pair in @(@{ Name = 'Maximize'; Want = 9 }, @{ Name = 'Close'; Want = 1 })) {
        $condition = New-Object System.Windows.Automation.PropertyCondition -ArgumentList @(
            [System.Windows.Automation.AutomationElement]::NameProperty, $pair.Name)
        $button = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
        if ($null -eq $button) { throw ("the {0} button has no accessible name" -f $pair.Name) }
        $rectangle = $button.Current.BoundingRectangle
        $x = [int]($rectangle.X + $rectangle.Width / 2)
        $y = [int]($rectangle.Y + $rectangle.Height / 2)
        $answer = [BarNative]::SendMessageW($handle, 0x0084, [IntPtr]::Zero,
            [IntPtr](($y -shl 16) -bor ($x -band 0xffff))).ToInt32()
        Write-Output ("HITTEST {0} -> {1}" -f $pair.Name, $answer)
        if ($answer -ne $pair.Want) { throw ("{0} answered {1}" -f $pair.Name, $answer) }
    }

    # And the button minimizes it.
    $condition = New-Object System.Windows.Automation.PropertyCondition -ArgumentList @(
        [System.Windows.Automation.AutomationElement]::NameProperty, 'Minimize')
    $minimize = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
    if ($null -eq $minimize) { throw 'the minimize button has no accessible name' }
    ([System.Windows.Automation.InvokePattern]$minimize.GetCurrentPattern(
        [System.Windows.Automation.InvokePattern]::Pattern)).Invoke()
    Start-Sleep -Milliseconds 600
    Write-Output ("STATE " + (Get-State $window))
    if ((Get-State $window) -ne 'Minimized') { throw 'the window did not minimize' }
    Write-Output 'TITLEBAR_OK'
} finally {
    if (-not $process.HasExited) { $process.Kill(); $process.WaitForExit() }
}
