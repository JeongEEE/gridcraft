# Isolated CI only: prove the updater's MSI invocation upgrades an existing per-user product.
param([Parameter(Mandatory=$true)][string] $Msi)
$ErrorActionPreference = 'Stop'
$Msi = (Resolve-Path $Msi).Path
if ((Test-Path 'HKCU:\Software\Grid') -or (Test-Path 'HKLM:\Software\Grid')) {
    throw 'Grid automatic update tests require a clean, disposable Windows runner'
}
$root = Join-Path $env:RUNNER_TEMP ('GridUpgrade-' + [guid]::NewGuid())
New-Item -ItemType Directory $root | Out-Null
$baseline = Join-Path $root 'Grid-0.0.1.msi'
$folder = Join-Path $root 'Grid 한글 With Spaces'
Copy-Item -LiteralPath $Msi -Destination $baseline
# Make a test-only older product with its own product/package identities. The release MSI is
# never edited. Matching UpgradeCode/components let MSI exercise its real major-upgrade path.
& (Join-Path $PSHOME 'pwsh.exe') -NoProfile -NonInteractive -File (Join-Path $PSScriptRoot 'create-update-baseline.ps1') -Path $baseline
if ($LASTEXITCODE -ne 0) { throw 'Could not create the baseline MSI' }
$probe = [IO.File]::Open($baseline, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::None)
$probe.Dispose()
Write-Host 'Baseline MSI is readable and unlocked.'
$installed = $false
function Invoke-Msi([string] $Arguments) {
    $p = Start-Process msiexec.exe -ArgumentList $Arguments -PassThru
    if (-not $p.WaitForExit(120000)) { $p.Kill(); throw 'MSI test timed out' }
    if ($p.ExitCode -ne 0) {
        Get-ChildItem $root -Filter *.log | ForEach-Object { Get-Content $_.FullName -Tail 50 }
        throw "MSI test failed: $($p.ExitCode)"
    }
}
try {
    Invoke-Msi "/i `"$baseline`" INSTALLFOLDER=`"$folder`" ALLUSERS=`"`" MSIINSTALLPERUSER=1 /qn /norestart /l*v `"$(Join-Path $root 'baseline-install.log')`""
    $installed = $true
    $env:GRID_UPDATE_SMOKE_PACKAGE = $Msi
    $env:GRID_UPDATE_SMOKE_INSTALL = $folder
    Get-Content (Join-Path $root 'baseline-install.log') | Select-String 'Property\(S\): (ALLUSERS|MSIINSTALLPERUSER)'
    cargo test -p gridcraft-update --lib native_windows_msi_upgrade -- --ignored --nocapture
    if ($LASTEXITCODE -ne 0) { throw 'Native MSI update regression failed' }
} finally {
    Remove-Item Env:GRID_UPDATE_SMOKE_PACKAGE -ErrorAction SilentlyContinue
    Remove-Item Env:GRID_UPDATE_SMOKE_INSTALL -ErrorAction SilentlyContinue
    if ($installed) {
        # Either the old or new product can remain when a test fails.
        $products = foreach ($hive in 'HKCU:', 'HKLM:') {
            foreach ($key in 'Software\Microsoft\Windows\CurrentVersion\Uninstall', 'Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall') {
                $path = Join-Path $hive $key
                if (Test-Path $path) {
                    Get-ChildItem $path | Where-Object {
                        $p = Get-ItemProperty $_.PSPath
                        $p.DisplayName -eq 'Grid' -and $p.Publisher -eq 'JeongEEE'
                    }
                }
            }
        }
        $products = $products | Sort-Object PSChildName -Unique
        foreach ($product in $products) { Invoke-Msi "/x $($product.PSChildName) /qn /norestart" }
    }
    Remove-Item -Recurse -Force $root -ErrorAction SilentlyContinue
}
