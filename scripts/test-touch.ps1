param([string]$Executable = 'target/debug/examples/controls.exe', [string]$Gestures = 'target/debug/examples/gestures.exe')
# Drives the controls gallery with injected touch input: a tap, a slider drag
# and a swipe with inertia; then the gestures example with two fingers: a pinch,
# and two sliders at once. The input is real and lands on whatever is on
# screen, so every gesture first checks that its point belongs to the example's
# window and skips otherwise.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Threading;
public static class TouchNative {
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; }
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)] public struct PointerInfo {
        public uint PointerType; public uint PointerId; public uint FrameId; public uint PointerFlags;
        public IntPtr SourceDevice; public IntPtr Target;
        public Point PixelLocation; public Point HimetricLocation; public Point PixelLocationRaw; public Point HimetricLocationRaw;
        public uint Time; public uint HistoryCount; public int InputData; public uint KeyStates;
        public ulong PerformanceCount; public int ButtonChangeType;
    }
    [StructLayout(LayoutKind.Sequential)] public struct PointerTouchInfo {
        public PointerInfo Pointer; public uint TouchFlags; public uint TouchMask;
        public Rect Contact; public Rect ContactRaw; public uint Orientation; public uint Pressure;
    }
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll", SetLastError = true)] public static extern bool InitializeTouchInjection(uint maxCount, uint mode);
    [DllImport("user32.dll", SetLastError = true)] public static extern bool InjectTouchInput(uint count, [In] PointerTouchInfo[] contacts);
    [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(Point point);
    [DllImport("user32.dll")] public static extern IntPtr GetAncestor(IntPtr window, uint flags);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr window, IntPtr after, int x, int y, int cx, int cy, uint flags);

    const uint InRange = 0x2, InContact = 0x4, Down = 0x10000, Update = 0x20000, Up = 0x40000;

    public static bool Owns(IntPtr window, int x, int y) {
        Point point = new Point { X = x, Y = y };
        return GetAncestor(WindowFromPoint(point), 2) == window;
    }

    static void Send(int x, int y, uint flags) {
        PointerTouchInfo[] contact = new PointerTouchInfo[1];
        contact[0].Pointer.PointerType = 2;
        contact[0].Pointer.PointerId = 0;
        contact[0].Pointer.PointerFlags = flags;
        contact[0].Pointer.PixelLocation = new Point { X = x, Y = y };
        contact[0].TouchMask = 1 | 2 | 4;
        contact[0].Contact = new Rect { Left = x - 2, Top = y - 2, Right = x + 2, Bottom = y + 2 };
        contact[0].Orientation = 90;
        contact[0].Pressure = 512;
        if (!InjectTouchInput(1, contact))
            throw new InvalidOperationException("InjectTouchInput failed: " + Marshal.GetLastWin32Error());
    }

    // Two contacts at once, each from one point to another.
    public static void Pair(int ax0, int ay0, int ax1, int ay1, int bx0, int by0, int bx1, int by1, int steps, int pause) {
        for (int step = -1; step <= steps + 1; step++) {
            int at = Math.Max(0, Math.Min(step, steps));
            uint flags = step < 0 ? (InRange | InContact | Down) : step > steps ? Up : (InRange | InContact | Update);
            PointerTouchInfo[] contacts = new PointerTouchInfo[2];
            for (int finger = 0; finger < 2; finger++) {
                int x = finger == 0 ? ax0 + (ax1 - ax0) * at / steps : bx0 + (bx1 - bx0) * at / steps;
                int y = finger == 0 ? ay0 + (ay1 - ay0) * at / steps : by0 + (by1 - by0) * at / steps;
                contacts[finger].Pointer.PointerType = 2;
                contacts[finger].Pointer.PointerId = (uint)finger;
                contacts[finger].Pointer.PointerFlags = flags;
                contacts[finger].Pointer.PixelLocation = new Point { X = x, Y = y };
                contacts[finger].TouchMask = 1 | 2 | 4;
                contacts[finger].Contact = new Rect { Left = x - 2, Top = y - 2, Right = x + 2, Bottom = y + 2 };
                contacts[finger].Orientation = 90;
                contacts[finger].Pressure = 512;
            }
            if (!InjectTouchInput(2, contacts))
                throw new InvalidOperationException("InjectTouchInput failed: " + Marshal.GetLastWin32Error());
            Thread.Sleep(pause);
        }
    }

    // A contact from one point to another in `steps` moves, `pause` ms apart.
    public static void Stroke(int x0, int y0, int x1, int y1, int steps, int pause) {
        Send(x0, y0, InRange | InContact | Down);
        for (int step = 1; step <= steps; step++) {
            Thread.Sleep(pause);
            Send(x0 + (x1 - x0) * step / steps, y0 + (y1 - y0) * step / steps, InRange | InContact | Update);
        }
        Send(x1, y1, Up);
    }
}
'@
$null = [TouchNative]::SetProcessDPIAware()
if (-not [TouchNative]::InitializeTouchInjection(2, 1)) { throw 'Touch injection is not available' }

function Wait-Element($Root, [string]$Name, [int]$Attempts = 60) {
    $condition = New-Object System.Windows.Automation.PropertyCondition -ArgumentList @(
        [System.Windows.Automation.AutomationElement]::NameProperty, $Name)
    for ($attempt = 0; $attempt -lt $Attempts; $attempt++) {
        $found = $Root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
        if ($null -ne $found) { return $found }
        Start-Sleep -Milliseconds 100
    }
    throw "Element not found: $Name"
}
function Find-State($Root) {
    $all = $Root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
    foreach ($element in $all) { if ($element.Current.Name -like 'State: *') { return $element.Current.Name } }
    return ''
}
function Center($Element) {
    $rect = $Element.Current.BoundingRectangle
    return @([int]($rect.X + $rect.Width / 2), [int]($rect.Y + $rect.Height / 2), $rect)
}

function Open-Example([string]$Path, [string]$Title) {
    $info = New-Object System.Diagnostics.ProcessStartInfo
    $info.FileName = (Resolve-Path -LiteralPath $Path).Path
    $info.UseShellExecute = $false
    # Diagnostics on stderr would end a PowerShell job that runs this script.
    $info.RedirectStandardError = $true
    $process = [System.Diagnostics.Process]::Start($info)
    $processCondition = New-Object System.Windows.Automation.PropertyCondition -ArgumentList @(
        [System.Windows.Automation.AutomationElement]::ProcessIdProperty, [int]$process.Id)
    $titleCondition = New-Object System.Windows.Automation.PropertyCondition -ArgumentList @(
        [System.Windows.Automation.AutomationElement]::NameProperty, $Title)
    $condition = New-Object System.Windows.Automation.AndCondition -ArgumentList @($processCondition, $titleCondition)
    $window = $null
    for ($attempt = 0; $attempt -lt 100 -and $null -eq $window; $attempt++) {
        $window = [System.Windows.Automation.AutomationElement]::RootElement.FindFirst([System.Windows.Automation.TreeScope]::Children, $condition)
        if ($null -eq $window) { Start-Sleep -Milliseconds 100 }
    }
    if ($null -eq $window) { $process.Kill(); throw "Window not found: $Title" }
    $handle = [IntPtr]$window.Current.NativeWindowHandle
    $null = [TouchNative]::SetForegroundWindow($handle)
    # A background script may not take the foreground; topmost needs no such
    # right. HWND_TOPMOST, without moving, sizing or activating.
    $null = [TouchNative]::SetWindowPos($handle, [IntPtr](-1), 0, 0, 0, 0, 0x0013)
    Start-Sleep -Milliseconds 800
    return @($process, $window, $handle)
}
function Find-Label($Root, [string]$Pattern) {
    $all = $Root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
    foreach ($element in $all) { if ($element.Current.Name -like $Pattern) { return $element.Current.Name } }
    return ''
}

$probe, $window, $handle = Open-Example $Executable 'Glassmere / Text and controls'
try {
    $check = Wait-Element $window 'Notifications'
    $x, $y, $null = Center $check
    if (-not [TouchNative]::Owns($handle, $x, $y)) { Write-Output 'SKIP: the window is covered'; return }
    $before = Find-State $window
    [TouchNative]::Stroke($x, $y, $x + 2, $y + 1, 2, 20)
    Start-Sleep -Milliseconds 700
    $after = Find-State $window
    if ($after -notlike 'State: true*') { throw "Tap did not toggle exactly once: '$before' then '$after'" }
    Write-Output "PASS: a tap toggles once ($after)"

    $slider = Wait-Element $window 'Volume'
    $x, $y, $rect = Center $slider
    if (-not [TouchNative]::Owns($handle, $x, $y)) { Write-Output 'SKIP: the window is covered'; return }
    [TouchNative]::Stroke($x, $y, [int]($rect.X + $rect.Width * 0.9), $y, 10, 15)
    Start-Sleep -Milliseconds 700
    $after = Find-State $window
    $volume = [double](($after -split ' / ')[2])
    if ($volume -lt 80) { throw "The slider did not follow the finger: $after" }
    Write-Output "PASS: a slider follows the finger ($after)"

    $x, $y, $rect = Center $check
    $top = $rect.Y
    $sx = [int]($window.Current.BoundingRectangle.X + $window.Current.BoundingRectangle.Width / 2)
    $sy = [int]($window.Current.BoundingRectangle.Y + $window.Current.BoundingRectangle.Height * 0.7)
    if (-not [TouchNative]::Owns($handle, $sx, $sy)) { Write-Output 'SKIP: the window is covered'; return }
    # A short, gentle swipe, so that the page does not reach its end before the
    # inertia has shown: 40 physical pixels, then whatever the fling adds.
    [TouchNative]::Stroke($sx, $sy, $sx, $sy - 40, 8, 10)
    Start-Sleep -Milliseconds 2000
    $findCheck = New-Object System.Windows.Automation.PropertyCondition -ArgumentList @(
        [System.Windows.Automation.AutomationElement]::NameProperty, 'Notifications')
    $moved = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $findCheck)
    if ($null -eq $moved) { throw 'The checkbox left the view; the swipe was meant to be gentle' }
    $travel = $top - $moved.Current.BoundingRectangle.Y
    if ($travel -lt 38) { throw "The swipe of 40 px scrolled only $travel px" }
    if ($travel -lt 70) { throw "The swipe of 40 px scrolled $travel px: no inertia" }
    Write-Output "PASS: a swipe of 40 px scrolled $travel px, the rest by inertia"
    $state = Find-State $window
    if ($state -ne $after) { throw "The swipe pressed something: '$after' became '$state'" }
} finally {
    if (-not $probe.HasExited) { $probe.Kill() }
}

$probe, $window, $handle = Open-Example $Gestures 'Glassmere / Gestures'
try {
    $photo = Wait-Element $window 'Hills at dusk'
    $x, $y, $null = Center $photo
    if (-not ([TouchNative]::Owns($handle, $x - 100, $y) -and [TouchNative]::Owns($handle, $x + 100, $y))) {
        Write-Output 'SKIP: the window is covered'; return
    }
    # Two fingers from 80 px apart to 160 px apart, about the photo's centre.
    [TouchNative]::Pair($x - 40, $y, $x - 80, $y, $x + 40, $y, $x + 80, $y, 10, 15)
    Start-Sleep -Milliseconds 600
    $state = Find-Label $window 'Zoom *'
    $zoom = [double](($state -split ' ')[1])
    if ($zoom -lt 1.9 -or $zoom -gt 2.1) { throw "A pinch to twice the distance gave: $state" }
    if ($state -notlike '*taps 0*') { throw "The pinch counted as a tap: $state" }
    Write-Output "PASS: a pinch to twice the distance zooms by two ($state)"

    $left = Wait-Element $window 'Corners'
    $right = Wait-Element $window 'Opacity'
    $lx, $ly, $lrect = Center $left
    $rx, $ry, $rrect = Center $right
    if (-not ([TouchNative]::Owns($handle, $lx, $ly) -and [TouchNative]::Owns($handle, $rx, $ry))) {
        Write-Output 'SKIP: the window is covered'; return
    }
    [TouchNative]::Pair($lx, $ly, [int]($lrect.X + $lrect.Width * 0.1), $ly, $rx, $ry, [int]($rrect.X + $rrect.Width * 0.9), $ry, 10, 15)
    Start-Sleep -Milliseconds 600
    $state = Find-Label $window 'Zoom *'
    $sliders = (($state -split 'sliders ')[1]) -split ' / '
    if ([double]$sliders[0] -gt 10 -or [double]$sliders[1] -lt 90) { throw "Two sliders under two fingers gave: $state" }
    Write-Output "PASS: two sliders follow two fingers at once (sliders $($sliders -join ' / '))"
    Write-Output 'TOUCH_OK'
} finally {
    if (-not $probe.HasExited) { $probe.Kill() }
}
