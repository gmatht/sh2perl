Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

# Global variables
$form = $null
$label = $null
$timer = $null
$filePath = "first_n_tests_passed.txt"
$isDragging = $false
$dragOffset = New-Object System.Drawing.Point(0, 0)

function Create-TooltipWindow {
    # Create the main form
    $script:form = New-Object System.Windows.Forms.Form
    $script:form.Text = "Test Monitor"
    $script:form.Size = New-Object System.Drawing.Size(300, 100)
    $script:form.StartPosition = "Manual"
    $script:form.Location = New-Object System.Drawing.Point(50, 50)
    $script:form.FormBorderStyle = "None"
    $script:form.TopMost = $true
    $script:form.BackColor = [System.Drawing.Color]::LightYellow
    $script:form.Opacity = 0.9
    
    # Make it draggable
    $script:form.Add_MouseDown({
        param($sender, $e)
        if ($e.Button -eq [System.Windows.Forms.MouseButtons]::Left) {
            $script:isDragging = $true
            $script:dragOffset = New-Object System.Drawing.Point($e.X, $e.Y)
            $script:form.Cursor = [System.Windows.Forms.Cursors]::SizeAll
        }
    })
    
    $script:form.Add_MouseMove({
        param($sender, $e)
        if ($script:isDragging) {
            $newX = $script:form.Location.X + $e.X - $script:dragOffset.X
            $newY = $script:form.Location.Y + $e.Y - $script:dragOffset.Y
            $script:form.Location = New-Object System.Drawing.Point($newX, $newY)
        }
    })
    
    $script:form.Add_MouseUp({
        param($sender, $e)
        $script:isDragging = $false
        $script:form.Cursor = [System.Windows.Forms.Cursors]::Default
    })
    
    # Create label for content
    $script:label = New-Object System.Windows.Forms.Label
    $script:label.AutoSize = $false
    $script:label.Size = New-Object System.Drawing.Size(280, 80)
    $script:label.Location = New-Object System.Drawing.Point(10, 10)
    $script:label.TextAlign = [System.Drawing.ContentAlignment]::TopLeft
    $script:label.Font = New-Object System.Drawing.Font("Consolas", 9)
    $script:label.BackColor = [System.Drawing.Color]::Transparent
    $script:form.Controls.Add($script:label)
    
    # Add click event to raise Cursor window
    $script:form.Add_Click({
        Raise-CursorWindow
    })
    $script:label.Add_Click({
        Raise-CursorWindow
    })
    
    # Create timer for updates
    $script:timer = New-Object System.Windows.Forms.Timer
    $script:timer.Interval = 2000  # 2 seconds
    $script:timer.Add_Tick({
        Update-Content
    })
    
    # Initial content load
    Update-Content
    $script:timer.Start()
    
    # Show the form
    $script:form.ShowDialog()
}

function Update-Content {
    if (Test-Path $filePath) {
        try {
            $content = Get-Content $filePath -Raw -ErrorAction Stop
            $script:label.Text = $content.Trim()
        }
        catch {
            $script:label.Text = "Error reading file: $($_.Exception.Message)"
        }
    }
    else {
        $script:label.Text = "File not found: $filePath"
    }
}

function Raise-CursorWindow {
    try {
        # Find Cursor window by title containing "sh2perl"
        $processes = Get-Process | Where-Object { $_.MainWindowTitle -like "*sh2perl*" -and $_.MainWindowTitle -like "*Cursor*" }
        
        if ($processes) {
            foreach ($process in $processes) {
                if ($process.MainWindowHandle -ne [IntPtr]::Zero) {
                    # Bring window to front
                    [System.Windows.Forms.SendKeys]::SendWait("%{TAB}")
                    Add-Type -TypeDefinition @"
                        using System;
                        using System.Runtime.InteropServices;
                        public class Win32 {
                            [DllImport("user32.dll")]
                            public static extern bool SetForegroundWindow(IntPtr hWnd);
                            [DllImport("user32.dll")]
                            public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
                        }
"@
                    [Win32]::ShowWindow($process.MainWindowHandle, 9)  # SW_RESTORE
                    [Win32]::SetForegroundWindow($process.MainWindowHandle)
                    break
                }
            }
        }
        else {
            # Fallback: try to find any Cursor window
            $cursorProcesses = Get-Process | Where-Object { $_.ProcessName -like "*cursor*" -or $_.MainWindowTitle -like "*Cursor*" }
            foreach ($process in $cursorProcesses) {
                if ($process.MainWindowHandle -ne [IntPtr]::Zero) {
                    [Win32]::ShowWindow($process.MainWindowHandle, 9)
                    [Win32]::SetForegroundWindow($process.MainWindowHandle)
                    break
                }
            }
        }
    }
    catch {
        Write-Host "Error raising Cursor window: $($_.Exception.Message)"
    }
}

# Cleanup function
function Cleanup {
    if ($script:timer) {
        $script:timer.Stop()
        $script:timer.Dispose()
    }
    if ($script:form) {
        $script:form.Close()
        $script:form.Dispose()
    }
}

# Handle cleanup on exit
Register-EngineEvent -SourceIdentifier PowerShell.Exiting -Action { Cleanup }

# Start the tooltip window
Create-TooltipWindow
