# Creates default_config.zip containing default configuration files
# Usage: .\bundle-default-config.ps1 [-OutputDir <path>]

[CmdletBinding()]
Param(
    [Parameter()][string]$OutputDir = "target\release"
)

$ErrorActionPreference = 'Stop'

$zipName = "default_config.zip"

# Get workspace root (script is in script/ directory)
$workspaceRoot = Split-Path -Parent $PSScriptRoot

# Create temp directory for zip contents
$tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ([System.Guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $tempDir -Force | Out-Null

try {
    # Create config.json (manifest)
    $configJson = @{
        version = 1
        description = "Bspterm Default Configuration"
    } | ConvertTo-Json
    Set-Content -Path (Join-Path $tempDir "config.json") -Value $configJson -Encoding UTF8

    # Copy terminal rules
    Copy-Item -Path (Join-Path $workspaceRoot "assets\settings\default_terminal_rules.json") -Destination (Join-Path $tempDir "terminal_rules.json")

    # Create button_bar.json
    $buttonBarJson = @{
        version = 1
        buttons = @(
            @{
                label = "ne5000e_mpu_collector"
                script_path = "ne5000e_mpu_collector.py"
                tooltip = "Collect MPU IP addresses from NE5000E router"
                enabled = $true
            }
        )
        show_button_bar = $true
    } | ConvertTo-Json -Depth 10
    Set-Content -Path (Join-Path $tempDir "button_bar.json") -Value $buttonBarJson -Encoding UTF8

    # Create abbreviations.json
    $abbreviationsJson = @{
        version = 1
        abbreviations = @()
        expansion_enabled = $true
        show_abbr_bar = $true
    } | ConvertTo-Json
    Set-Content -Path (Join-Path $tempDir "abbreviations.json") -Value $abbreviationsJson -Encoding UTF8

    # Copy scripts
    $scriptsDir = Join-Path $tempDir "scripts"
    New-Item -ItemType Directory -Path $scriptsDir -Force | Out-Null
    Copy-Item -Path (Join-Path $workspaceRoot "assets\scripts\ne5000e_mpu_collector.py") -Destination $scriptsDir

    # Ensure output directory exists
    $fullOutputDir = if ([System.IO.Path]::IsPathRooted($OutputDir)) {
        $OutputDir
    } else {
        Join-Path $workspaceRoot $OutputDir
    }
    New-Item -ItemType Directory -Path $fullOutputDir -Force | Out-Null

    # Create zip archive
    $zipPath = Join-Path $fullOutputDir $zipName
    if (Test-Path $zipPath) {
        Remove-Item $zipPath -Force
    }
    Compress-Archive -Path (Join-Path $tempDir "*") -DestinationPath $zipPath -Force

    Write-Output "Created $zipPath"
}
finally {
    # Clean up temp directory
    if (Test-Path $tempDir) {
        Remove-Item -Path $tempDir -Recurse -Force
    }
}
