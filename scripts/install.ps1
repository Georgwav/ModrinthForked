# Installs or updates Threadrinth on Windows from the latest GitHub release.
#
#   irm https://raw.githubusercontent.com/Georgwav/Threadrinth/main/scripts/install.ps1 | iex
#
# Downloads the installer, checks it against the SHA-256 GitHub publishes for
# it, and runs it silently for the current user.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$repo = 'Georgwav/Threadrinth'

Write-Host 'Finding the latest Threadrinth release...'
$release = Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest"
$asset = $release.assets | Where-Object { $_.name -like '*_x64-setup.exe' } | Select-Object -First 1
if (-not $asset) { throw 'The latest release has no Windows installer.' }

$installer = Join-Path $env:TEMP $asset.name
Write-Host "Downloading $($asset.name)..."
Invoke-WebRequest $asset.browser_download_url -OutFile $installer

try {
	if ($asset.digest -like 'sha256:*') {
		$expected = $asset.digest.Substring(7)
		$actual = (Get-FileHash $installer -Algorithm SHA256).Hash
		if ($actual -ne $expected) { throw 'The download is corrupted (checksum mismatch).' }
	}

	Write-Host 'Installing...'
	$process = Start-Process $installer -ArgumentList '/S' -Wait -PassThru
	if ($process.ExitCode -ne 0) { throw "The installer failed with exit code $($process.ExitCode)." }
}
finally {
	Remove-Item $installer -ErrorAction SilentlyContinue
}

Write-Host 'Threadrinth is installed. Start it from the Start menu.'
