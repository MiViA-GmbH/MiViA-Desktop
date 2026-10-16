param([int]$Seconds)
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class Win32 {
    [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr window, uint message, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr window, uint message, IntPtr w, IntPtr l);
}
'@
$element = [System.Windows.Automation.AutomationElement]
$scope = [System.Windows.Automation.TreeScope]
$click = 0xF5
$checked = 0xF0
function Find($window, $name) {
    $byClass = New-Object System.Windows.Automation.PropertyCondition($element::ClassNameProperty, 'Button')
    $byName = New-Object System.Windows.Automation.PropertyCondition($element::NameProperty, $name)
    $window.FindFirst($scope::Descendants, (New-Object System.Windows.Automation.AndCondition($byClass, $byName)))
}
function Handle($control) { [IntPtr]$control.Current.NativeWindowHandle }
$byDialog = New-Object System.Windows.Automation.PropertyCondition($element::ClassNameProperty, '#32770')
$deadline = (Get-Date).AddSeconds($Seconds)
$steps = @()
$gone = 0
while ((Get-Date) -lt $deadline) {
    $windows = @($element::RootElement.FindAll($scope::Children, $byDialog) | Where-Object { $_.Current.Name -like '*MiViA*' -or $_.Current.Name -eq 'Installer Language' })
    if ($windows.Count -eq 0 -and $steps -contains 'clicked Uninstall') {
        $gone += 1
        if ($gone -ge 6) {
            Write-Output (($steps + 'window closed') -join ', ')
            exit 0
        }
    }
    foreach ($window in $windows) {
        $box = Find $window 'Delete the application data'
        if ($box -and [Win32]::SendMessage((Handle $box), $checked, [IntPtr]::Zero, [IntPtr]::Zero) -eq [IntPtr]::Zero) {
            [Win32]::PostMessage((Handle $box), $click, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
            $steps += 'checked delete data'
            continue
        }
        foreach ($label in 'OK', 'Yes', 'Uninstall', 'Close') {
            $button = Find $window $label
            if ($button -and $button.Current.IsEnabled) {
                [Win32]::PostMessage((Handle $button), $click, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
                $steps += "clicked $label"
                if ($label -eq 'Close') {
                    Write-Output ($steps -join ', ')
                    exit 0
                }
                break
            }
        }
    }
    Start-Sleep -Milliseconds 700
}
$seen = foreach ($window in $element::RootElement.FindAll($scope::Children, [System.Windows.Automation.Condition]::TrueCondition)) {
    $controls = @()
    if ($window.Current.ClassName -eq '#32770') {
        $controls = $window.FindAll($scope::Descendants, [System.Windows.Automation.Condition]::TrueCondition) | ForEach-Object { "$($_.Current.ClassName) '$($_.Current.Name)'" }
    }
    "'$($window.Current.Name)' $($window.Current.ClassName) pid $($window.Current.ProcessId) [$($controls -join '|')]"
}
Write-Output ("the uninstaller did not finish after $Seconds s, steps: " + ($steps -join ', ') + '; windows: ' + ($seen -join '; '))
exit 1
