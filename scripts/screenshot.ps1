param(
    [Parameter(Mandatory = $true)][string]$Executable,
    [Parameter(Mandatory = $true)][string]$TitlePrefix,
    [Parameter(Mandatory = $true)][string]$Name,
    [int]$SettleSeconds = 2,
    [string]$FocusName = '',
    [int]$TabAfter = 0,
    [string]$Arguments = '',
    [string]$HoverName = '',
    [string]$RightClickName = '',
    [int]$HoverSeconds = 1,
    [string]$TypeText = '',
    [string]$InvokeNames = '',
    [string]$ClickAt = '',
    [string]$Wheel = ''
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
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; }
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr window, IntPtr dc, uint flags);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr window, uint message, IntPtr wParam, IntPtr lParam);
    [DllImport("user32.dll")] public static extern uint MapVirtualKey(uint code, uint mapType);
    [DllImport("user32.dll")] public static extern bool ScreenToClient(IntPtr window, ref Point point);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
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
        # A visible label often carries the same name as its field: take the focusable one.
        $named = New-Object System.Windows.Automation.PropertyCondition -ArgumentList @(
            [System.Windows.Automation.AutomationElement]::NameProperty, $target)
        $focusable = New-Object System.Windows.Automation.PropertyCondition -ArgumentList @(
            [System.Windows.Automation.AutomationElement]::IsKeyboardFocusableProperty, $true)
        $condition = New-Object System.Windows.Automation.AndCondition -ArgumentList @($named, $focusable)
        $element = $null
        for ($attempt = 0; $attempt -lt 50 -and $null -eq $element; $attempt++) {
            $element = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
            Start-Sleep -Milliseconds 100
        }
        if ($null -eq $element) { throw "Accessible element not found: $target" }
        $element.SetFocus()
        Start-Sleep -Seconds 1
    }
    # Tab first, so that typed keys go to the control Tab arrived at.
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
    if ($TypeText) {
        $handle = [IntPtr]$window.Current.NativeWindowHandle
        # Letters and digits only: winit derives text from key presses, and
        # their virtual key codes equal their upper-case character codes.
        foreach ($character in $TypeText.ToUpperInvariant().ToCharArray()) {
            $code = [int]$character
            $scan = [ShotNative]::MapVirtualKey($code, 0)
            $flags = ([long]$scan -shl 16) -bor 1
            $null = [ShotNative]::PostMessage($handle, 0x0100, [IntPtr]$code, [IntPtr]$flags)
            Start-Sleep -Milliseconds 60
            $null = [ShotNative]::PostMessage($handle, 0x0101, [IntPtr]$code, [IntPtr]($flags -bor 3221225472L))
            Start-Sleep -Milliseconds 120
        }
        Start-Sleep -Seconds 1
    }
    foreach ($gesture in ($Wheel -split '\|' | Where-Object { $_ })) {
        # "x,y,right,down": the pointer moves to client pixel x,y, then the wheel
        # turns that many notches sideways and downwards (negative for left, up).
        $x, $y, $right, $down = $gesture -split ',' | ForEach-Object { [int]$_ }
        $handle = [IntPtr]$window.Current.NativeWindowHandle
        $position = [IntPtr](($y -shl 16) -bor ($x -band 65535))
        $null = [ShotNative]::PostMessage($handle, 0x0200, [IntPtr]0, $position)
        Start-Sleep -Milliseconds 100
        if ($down -ne 0) {
            $null = [ShotNative]::PostMessage($handle, 0x020A, [IntPtr]((-120 * $down) -shl 16), $position)
        }
        if ($right -ne 0) {
            $null = [ShotNative]::PostMessage($handle, 0x020E, [IntPtr]((120 * $right) -shl 16), $position)
        }
        Start-Sleep -Milliseconds 500
    }
    if ($ClickAt) {
        # A left click at "x,y" in client pixels, for places that have no name.
        $x, $y = $ClickAt -split ',' | ForEach-Object { [int]$_ }
        $handle = [IntPtr]$window.Current.NativeWindowHandle
        $position = [IntPtr](($y -shl 16) -bor ($x -band 65535))
        $null = [ShotNative]::PostMessage($handle, 0x0200, [IntPtr]0, $position)
        $null = [ShotNative]::PostMessage($handle, 0x0201, [IntPtr]1, $position)
        $null = [ShotNative]::PostMessage($handle, 0x0202, [IntPtr]0, $position)
        Start-Sleep -Seconds 1
    }
    foreach ($target in ($InvokeNames -split '\|' | Where-Object { $_ })) {
        # Activates buttons by name, after any typing.
        $condition = New-Object System.Windows.Automation.PropertyCondition -ArgumentList @(
            [System.Windows.Automation.AutomationElement]::NameProperty, $target)
        $element = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
        if ($null -eq $element) { throw "Accessible element not found: $target" }
        ([System.Windows.Automation.InvokePattern]$element.GetCurrentPattern(
            [System.Windows.Automation.InvokePattern]::Pattern)).Invoke()
        # Long enough for what the button started to show.
        Start-Sleep -Seconds 2
    }
    function Find-Named([string]$name) {
        $condition = New-Object System.Windows.Automation.PropertyCondition -ArgumentList @(
            [System.Windows.Automation.AutomationElement]::NameProperty, $name)
        $found = $null
        for ($attempt = 0; $attempt -lt 50 -and $null -eq $found; $attempt++) {
            $found = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
            Start-Sleep -Milliseconds 100
        }
        if ($null -eq $found) { throw "Accessible element not found: $name" }
        return $found
    }
    function Client-Position($element) {
        $handle = [IntPtr]$window.Current.NativeWindowHandle
        $rect = $element.Current.BoundingRectangle
        $point = New-Object ShotNative+Point
        $point.X = [int]($rect.X + $rect.Width / 2)
        $point.Y = [int]($rect.Y + $rect.Height / 2)
        $null = [ShotNative]::ScreenToClient($handle, [ref]$point)
        return [IntPtr](($point.Y -shl 16) -bor ($point.X -band 65535))
    }
    $restoreCursor = $null
    if ($HoverName) {
        # Move the real cursor: Windows follows a synthetic move with a mouse-leave
        # when the pointer is elsewhere, which would cancel the hover dwell.
        Add-Type -AssemblyName System.Windows.Forms
        $restoreCursor = [System.Windows.Forms.Cursor]::Position
        # UI Automation focus activates the window from a background process,
        # which SetForegroundWindow alone is not allowed to do.
        $element = Find-Named $HoverName
        $element.SetFocus()
        Start-Sleep -Milliseconds 300
        $null = [ShotNative]::SetForegroundWindow([IntPtr]$window.Current.NativeWindowHandle)
        $rect = $element.Current.BoundingRectangle
        [System.Windows.Forms.Cursor]::Position = New-Object System.Drawing.Point([int]($rect.X + $rect.Width / 2), [int]($rect.Y + $rect.Height / 2))
        Start-Sleep -Seconds $HoverSeconds
    }
    if ($RightClickName) {
        $handle = [IntPtr]$window.Current.NativeWindowHandle
        $position = Client-Position (Find-Named $RightClickName)
        $null = [ShotNative]::PostMessage($handle, 0x0200, [IntPtr]0, $position)
        $null = [ShotNative]::PostMessage($handle, 0x0204, [IntPtr]2, $position)
        $null = [ShotNative]::PostMessage($handle, 0x0205, [IntPtr]0, $position)
        Start-Sleep -Seconds 1
    }
    # PrintWindow renders the whole window rectangle, which on Windows 11 includes
    # invisible resize borders that the UI Automation rectangle leaves out; sizing
    # the bitmap from the latter cropped the right and bottom edges.
    $rectangle = New-Object ShotNative+Rect
    $null = [ShotNative]::GetWindowRect([IntPtr]$window.Current.NativeWindowHandle, [ref]$rectangle)
    $bitmap = New-Object System.Drawing.Bitmap -ArgumentList @(
        ($rectangle.Right - $rectangle.Left), ($rectangle.Bottom - $rectangle.Top))
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
    if ($null -ne $restoreCursor) { [System.Windows.Forms.Cursor]::Position = $restoreCursor }
    if (-not $process.HasExited) { $process.Kill(); $process.WaitForExit() }
}
