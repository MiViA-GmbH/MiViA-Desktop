param([int]$Seconds)
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$element = [System.Windows.Automation.AutomationElement]
$scope = [System.Windows.Automation.TreeScope]
$type = [System.Windows.Automation.ControlType]
function Find($window, $controlType, $name) {
    $byType = New-Object System.Windows.Automation.PropertyCondition($element::ControlTypeProperty, $controlType)
    $byName = New-Object System.Windows.Automation.PropertyCondition($element::NameProperty, $name)
    $window.FindFirst($scope::Descendants, (New-Object System.Windows.Automation.AndCondition($byType, $byName)))
}
$byDialog = New-Object System.Windows.Automation.PropertyCondition($element::ClassNameProperty, '#32770')
$deadline = (Get-Date).AddSeconds($Seconds)
$steps = @()
$gone = 0
while ((Get-Date) -lt $deadline) {
    $windows = @($element::RootElement.FindAll($scope::Children, $byDialog) | Where-Object { $_.Current.Name -like '*MiViA*' })
    if ($windows.Count -eq 0 -and $steps -contains 'clicked Uninstall') {
        $gone += 1
        if ($gone -ge 6) {
            Write-Output (($steps + 'window closed') -join ', ')
            exit 0
        }
    }
    foreach ($window in $windows) {
        $box = Find $window $type::CheckBox 'Delete the application data'
        if ($box) {
            $toggle = $box.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern)
            if ($toggle.Current.ToggleState -eq [System.Windows.Automation.ToggleState]::Off) {
                $toggle.Toggle()
                $steps += 'checked delete data'
                continue
            }
        }
        foreach ($label in 'Yes', 'Uninstall', 'Close') {
            $button = Find $window $type::Button $label
            if ($button -and $button.Current.IsEnabled) {
                $button.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
                $steps += "clicked $label"
                if ($label -eq 'Close') {
                    Write-Output ($steps -join ', ')
                    exit 0
                }
                break
            }
        }
    }
    Start-Sleep -Milliseconds 500
}
$seen = foreach ($window in $element::RootElement.FindAll($scope::Children, [System.Windows.Automation.Condition]::TrueCondition)) {
    $buttons = @()
    if ($window.Current.ClassName -eq '#32770') {
        $buttons = $window.FindAll($scope::Descendants, [System.Windows.Automation.Condition]::TrueCondition) | ForEach-Object { "$($_.Current.ControlType.ProgrammaticName) '$($_.Current.Name)'" }
    }
    "'$($window.Current.Name)' $($window.Current.ClassName) pid $($window.Current.ProcessId) [$($buttons -join '|')]"
}
Write-Output ("the uninstaller did not finish after $Seconds s, steps: " + ($steps -join ', ') + '; windows: ' + ($seen -join '; '))
exit 1
