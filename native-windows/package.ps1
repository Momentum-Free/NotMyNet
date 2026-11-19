# Run from native-windows after the locked x64 release build.
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$target = 'x86_64-pc-windows-msvc'
$metadata = cargo metadata --locked --format-version 1 --filter-platform $target | ConvertFrom-Json
$version = ($metadata.packages | Where-Object id -eq $metadata.resolve.root).version
if ($version -cnotmatch '^\d+\.\d+\.\d+$') { throw "Invalid release version: $version" }
$name = "NotMyNet-Windows-x64-$version"
$stage = New-Item -ItemType Directory (Join-Path $env:RUNNER_TEMP $name)
$output = New-Item -ItemType Directory (Join-Path $env:RUNNER_TEMP 'notmynet-release')
$exe = Join-Path $metadata.target_directory "$target/release/notmynet-windows.exe"
Copy-Item $exe, ../LICENSE $stage
Copy-Item BUILD-README.txt (Join-Path $stage README.txt)

# Pin the notice generator separately from the app's Cargo.lock.
$tool = New-Item -ItemType Directory (Join-Path $env:RUNNER_TEMP 'cargo-about')
$archive = "cargo-about-0.9.3-$target.tar.gz"
$archivePath = Join-Path $tool $archive
Invoke-WebRequest "https://github.com/EmbarkStudios/cargo-about/releases/download/0.9.3/$archive" -OutFile $archivePath
if ((Get-FileHash $archivePath).Hash -ne '9e2942533ef1c592c4a70bdb384cae1550a3f5fc9ae40379b6662e3d2f16fe02') {
    throw 'cargo-about checksum mismatch'
}
tar.exe -xf $archivePath -C $tool
$about = Join-Path $tool "cargo-about-0.9.3-$target/cargo-about.exe"
& $about generate --locked --target $target --fail -o (Join-Path $stage THIRD-PARTY-NOTICES.txt) about.hbs
$assets = $metadata.packages | Where-Object name -eq 'gpui-kit-assets'
Copy-Item (Join-Path (Split-Path $assets.manifest_path) LICENSE-LUCIDE) $stage
$rustDoc = Join-Path (rustc --print sysroot) 'share/doc/rust'
$rustNotices = New-Item -ItemType Directory (Join-Path $stage 'rust-notices')
Copy-Item (Join-Path $rustDoc COPYRIGHT-library.html), (Join-Path $rustDoc licenses) $rustNotices -Recurse

$zip = Join-Path $output "$name.zip"
Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $zip
$hash = (Get-FileHash $zip).Hash.ToLowerInvariant()
Set-Content -Path (Join-Path $output 'SHA256SUMS.txt') -Value "$hash  $name.zip" -Encoding ascii
$extracted = Join-Path $env:RUNNER_TEMP 'notmynet-extracted'
Expand-Archive $zip $extracted
$packagedExe = Join-Path $extracted 'notmynet-windows.exe'
$exeHash = (Get-FileHash $packagedExe).Hash
if ($exeHash -ne (Get-FileHash $exe).Hash) { throw 'Extracted executable does not match build' }

$process = Start-Process -FilePath $packagedExe -WorkingDirectory $extracted -PassThru
try {
    $deadline = (Get-Date).AddSeconds(20)
    do {
        Start-Sleep -Milliseconds 250
        $process.Refresh()
    } until ($process.HasExited -or $process.MainWindowTitle -eq 'NotMyNet' -or (Get-Date) -ge $deadline)
    if ($process.HasExited -or $process.MainWindowTitle -ne 'NotMyNet') {
        throw 'The extracted executable did not open its NotMyNet window'
    }
} finally {
    if (!$process.HasExited) { Stop-Process -Id $process.Id }
}
Write-Host "Extracted app opened. EXE SHA-256: $exeHash; ZIP SHA-256: $hash"
Add-Content -Path $env:GITHUB_OUTPUT -Value "name=$name"
