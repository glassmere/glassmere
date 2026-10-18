param([string]$Executable = 'target/release/examples/controls.exe', [switch]$Screenshot, [switch]$PointerOnly)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class ControlsNative {
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; }
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr window, uint message, IntPtr wParam, IntPtr lParam);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr window, IntPtr dc, uint flags);
    [DllImport("user32.dll")] public static extern uint MapVirtualKey(uint code, uint mapType);
    [DllImport("user32.dll")] public static extern bool ScreenToClient(IntPtr window, ref Point point);
}
'@

function Wait-Element($Root, [string]$Name) {
    $condition = New-Object System.Windows.Automation.PropertyCondition -ArgumentList @(
        [System.Windows.Automation.AutomationElement]::NameProperty, $Name)
    for ($attempt = 0; $attempt -lt 70; $attempt++) {
        $element = $Root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
        if ($null -ne $element) { return $element }
        Start-Sleep -Milliseconds 100
    }
    $Root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition) |
        ForEach-Object { Write-Information "Child: $($_.Current.Name) $($_.Current.ControlType.ProgrammaticName)" -InformationAction Continue }
    throw "Accessible element not found: $Name"
}
function Invoke-Element($Element) {
    ([System.Windows.Automation.InvokePattern]$Element.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)).Invoke()
}
function Focus-Element($Element) {
    for ($attempt = 0; $attempt -lt 30; $attempt++) {
        $Element.SetFocus()
        Start-Sleep -Milliseconds 100
        if ($Element.Current.HasKeyboardFocus) { return }
    }
    throw "Keyboard focus was not established: $($Element.Current.Name)"
}
function Send-Key([IntPtr]$Handle, [uint32]$Key) {
    $scan = [ControlsNative]::MapVirtualKey($Key, 0)
    $flags = ([long]$scan -shl 16) -bor 1
    if ($Key -ge 0x21 -and $Key -le 0x28) { $flags = $flags -bor 0x01000000 }
    $null = [ControlsNative]::PostMessage($Handle, 0x0100, [IntPtr]$Key, [IntPtr]$flags)
    # TranslateMessage must enqueue WM_CHAR before the synthetic key release.
    Start-Sleep -Milliseconds 80
    $null = [ControlsNative]::PostMessage($Handle, 0x0101, [IntPtr]$Key, [IntPtr]($flags -bor 3221225472L))
}
function Click-Element([IntPtr]$Handle, $Element) {
    $bounds = $Element.Current.BoundingRectangle
    $point = New-Object ControlsNative+Point
    $point.X = [int]($bounds.X + $bounds.Width / 2)
    $point.Y = [int]($bounds.Y + $bounds.Height / 2)
    $null = [ControlsNative]::ScreenToClient($Handle, [ref]$point)
    $position = [IntPtr](($point.Y -shl 16) -bor ($point.X -band 65535))
    $null = [ControlsNative]::PostMessage($Handle, 0x0200, [IntPtr]0, $position)
    $null = [ControlsNative]::PostMessage($Handle, 0x0201, [IntPtr]1, $position)
    $null = [ControlsNative]::PostMessage($Handle, 0x0202, [IntPtr]0, $position)
}
function Save-Window($Window, [string]$Name) {
    Add-Type -AssemblyName System.Drawing
    $rectangle = $Window.Current.BoundingRectangle
    $bitmap = New-Object System.Drawing.Bitmap -ArgumentList @([int]$rectangle.Width, [int]$rectangle.Height)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $dc = $graphics.GetHdc()
    try {
        if (-not [ControlsNative]::PrintWindow([IntPtr]$Window.Current.NativeWindowHandle, $dc, 2)) { throw 'Window capture failed' }
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
$info.CreateNoWindow = $true
$info.WindowStyle = [System.Diagnostics.ProcessWindowStyle]::Hidden
$info.RedirectStandardOutput = $true
$info.RedirectStandardError = $true
$probe = [System.Diagnostics.Process]::Start($info)
try {
    $processCondition = New-Object System.Windows.Automation.PropertyCondition -ArgumentList @(
        [System.Windows.Automation.AutomationElement]::ProcessIdProperty, [int]$probe.Id)
    $titleCondition = New-Object System.Windows.Automation.PropertyCondition -ArgumentList @(
        [System.Windows.Automation.AutomationElement]::NameProperty, 'Glassmere / Text and controls')
    $condition = New-Object System.Windows.Automation.AndCondition -ArgumentList @($processCondition, $titleCondition)
    $window = $null
    for ($attempt = 0; $attempt -lt 100 -and $null -eq $window; $attempt++) {
        $window = [System.Windows.Automation.AutomationElement]::RootElement.FindFirst([System.Windows.Automation.TreeScope]::Children, $condition)
        if ($null -eq $window) { Start-Sleep -Milliseconds 100 }
    }
    if ($null -eq $window) { throw 'Controls native window not found' }
    $input = Wait-Element $window 'Name'
    # Use ControlType as the visible label also has the name "Name".
    $editCondition = New-Object System.Windows.Automation.PropertyCondition -ArgumentList @(
        [System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Edit)
    $input = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $editCondition)
    $identity = [string]::Join(',', $input.GetRuntimeId())
    $value = [System.Windows.Automation.ValuePattern]$input.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern)
    $value.SetValue('Native edit')
    $null = Wait-Element $window 'Text: Native edit'
    if ([string]::Join(',', $input.GetRuntimeId()) -ne $identity) { throw 'Editing changed retained identity' }
    $handle = [IntPtr]$window.Current.NativeWindowHandle
    if (-not $PointerOnly) {
        Focus-Element $input
        Send-Key $handle 0x51
        $null = Wait-Element $window 'Text: Native editq'
        Send-Key $handle 0x08
        $null = Wait-Element $window 'Text: Native edit'
        Send-Key $handle 0x0D
        $null = Wait-Element $window 'Submissions: 1'
        Write-Output 'PASS: native typing, Backspace and Enter submission'
    }
    $check = Wait-Element $window 'Notifications'
    ([System.Windows.Automation.TogglePattern]$check.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern)).Toggle()
    $null = Wait-Element $window 'State: true / 0 / 50 / 0'
    $radio = Wait-Element $window 'Balanced'
    ([System.Windows.Automation.SelectionItemPattern]$radio.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern)).Select()
    $null = Wait-Element $window 'State: true / 1 / 50 / 0'
    $slider = Wait-Element $window 'Volume'
    ([System.Windows.Automation.RangeValuePattern]$slider.GetCurrentPattern([System.Windows.Automation.RangeValuePattern]::Pattern)).SetValue(72)
    $null = Wait-Element $window 'State: true / 1 / 70 / 0'
    $comboCondition = New-Object System.Windows.Automation.PropertyCondition -ArgumentList @(
        [System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::ComboBox)
    $combo = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $comboCondition)
    # accesskit_windows 0.32.1 does not implement UIA ExpandCollapsePattern,
    # and excludes ComboBox from InvokePattern. Exercise focus + ArrowDown.
    if ($PointerOnly) { Click-Element $handle $combo }
    else { Focus-Element $combo; Send-Key $handle 0x28 }
    $light = Wait-Element $window 'Light'
    if ($Screenshot) { Save-Window $window 'controls-popup' }
    ([System.Windows.Automation.SelectionItemPattern]$light.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern)).Select()
    $null = Wait-Element $window 'State: true / 1 / 70 / 1'
    $actions = Wait-Element $window ('Actions  ' + [char]0x25BE)
    if ($PointerOnly) { Click-Element $handle $actions }
    else { Focus-Element $actions; Send-Key $handle 0x28 }
    $disabled = Wait-Element $window 'Publish (unavailable)'
    if ($disabled.Current.IsEnabled) { throw 'Disabled menu item is exposed as enabled' }
    Invoke-Element (Wait-Element $window 'Save draft')
    $null = Wait-Element $window 'Last action: saved'
    $unavailable = Wait-Element $window 'Unavailable field'
    if ($unavailable.Current.IsEnabled) { throw 'Disabled input is exposed as enabled' }
    if ($Screenshot) { Save-Window $window 'controls' }
    Write-Output 'PASS: native text ValuePattern, stable identity, checkbox TogglePattern, radio SelectionItemPattern, slider RangeValuePattern, combo expansion/selection, menus and disabled states'
} finally {
    if (-not $probe.HasExited) { $null = $probe.CloseMainWindow(); if (-not $probe.WaitForExit(5000)) { $probe.Kill() } }
    $errors = $probe.StandardError.ReadToEnd()
    if ($errors) { Write-Output $errors }
    $probe.Dispose()
}
