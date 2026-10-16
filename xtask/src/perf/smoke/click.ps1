param([int]$ProcessId, [string]$Name, [int]$Seconds)
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$element = [System.Windows.Automation.AutomationElement]
$scope = [System.Windows.Automation.TreeScope]
$byProcess = New-Object System.Windows.Automation.PropertyCondition($element::ProcessIdProperty, $ProcessId)
$byButton = New-Object System.Windows.Automation.PropertyCondition($element::ControlTypeProperty, [System.Windows.Automation.ControlType]::Button)
$byName = New-Object System.Windows.Automation.PropertyCondition($element::NameProperty, $Name)
$target = New-Object System.Windows.Automation.AndCondition($byButton, $byName)
$deadline = (Get-Date).AddSeconds($Seconds)
while ((Get-Date) -lt $deadline) {
    foreach ($window in $element::RootElement.FindAll($scope::Children, $byProcess)) {
        $button = $window.FindFirst($scope::Descendants, $target)
        if ($button -and $button.Current.IsEnabled) {
            $button.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
            exit 0
        }
    }
    Start-Sleep -Milliseconds 500
}
Write-Output "no enabled button named '$Name' in process $ProcessId after $Seconds s"
exit 1
