param([string]$Installer, [string]$Target, [int]$Seconds)
$name = 'MiViA Desktop smoke system install'
$action = New-ScheduledTaskAction -Execute $Installer -Argument "/S /D=$Target"
$principal = New-ScheduledTaskPrincipal -UserId 'SYSTEM' -LogonType ServiceAccount -RunLevel Highest
Register-ScheduledTask -TaskName $name -Action $action -Principal $principal -Force | Out-Null
Start-ScheduledTask -TaskName $name
$deadline = (Get-Date).AddSeconds($Seconds)
do {
    Start-Sleep -Seconds 1
    $state = (Get-ScheduledTask -TaskName $name).State
    $result = (Get-ScheduledTaskInfo -TaskName $name).LastTaskResult
} while (($state -eq 'Running' -or $result -eq 267011) -and (Get-Date) -lt $deadline)
Unregister-ScheduledTask -TaskName $name -Confirm:$false
Write-Output $result
