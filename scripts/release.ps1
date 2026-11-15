<#
  Builds Relay's installer, signs it for the in-app updater and writes latest.json.
  With -Publish it also creates the GitHub release (needs the GitHub CLI, logged in).

    .\scripts\release.ps1 -Version 0.2.0 -Notes "Mini player fixes"
    .\scripts\release.ps1 -Version 0.2.0 -NotesFile notes.md -Publish

  The updater's private key stays outside the repo, in %USERPROFILE%\.relay-keys\.
  The build goes to %LOCALAPPDATA%\relay-release\, outside the project folder.
  With -Publish, the release is also posted to the Discord webhook in %USERPROFILE%\.relay-keys\.
#>
param(
  [Parameter(Mandatory)] [string] $Version,
  [string] $Notes = "",
  [string] $NotesFile = "",
  [switch] $Publish,
  [switch] $SkipTests
)

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$repo = "xqn7/relay"
$keyFile = Join-Path $env:USERPROFILE ".relay-keys\relay-updater.key"

function Fail($message) {
  Write-Host $message -ForegroundColor Yellow
  exit 1
}

function Run($what, [scriptblock] $command) {
  Write-Host "`n== $what" -ForegroundColor Cyan
  & $command
  if ($LASTEXITCODE -ne 0) { Fail "$what failed." }
}

if ($Version -notmatch '^\d+\.\d+\.\d+$') { Fail "Version must look like 1.2.3" }
if (-not (Test-Path $keyFile)) { Fail "Signing key not found at $keyFile. Create one with: npx tauri signer generate -w `"$keyFile`"" }
if ($NotesFile) { $Notes = Get-Content $NotesFile -Raw -Encoding UTF8 }
if (-not $Notes) { $Notes = "Relay $Version" }

Set-Location $root
if (git status --porcelain) { Fail "Commit or stash your changes first, so the release matches a commit." }

# One version everywhere: package.json, Cargo.toml and tauri.conf.json.
$utf8 = New-Object System.Text.UTF8Encoding($false)
function SetVersion($file, $pattern, $replacement) {
  $path = Join-Path $root $file
  $text = [IO.File]::ReadAllText($path)
  $next = [regex]::Replace($text, $pattern, $replacement, [Text.RegularExpressions.RegexOptions]::Multiline)
  [IO.File]::WriteAllText($path, $next, $utf8)
}
SetVersion "package.json" '^(  "version": )"[^"]+"' "`$1`"$Version`""
SetVersion "src-tauri\tauri.conf.json" '^(  "version": )"[^"]+"' "`$1`"$Version`""
SetVersion "src-tauri\Cargo.toml" '^(version = )"[^"]+"' "`$1`"$Version`""

if (-not $SkipTests) {
  Run "Rust tests" { Push-Location src-tauri; cargo test --quiet; $c = $LASTEXITCODE; Pop-Location; $global:LASTEXITCODE = $c }
  Run "Type check" { npm run check --silent }
  Run "Frontend tests" { npm test --silent }
}

# The real Relay is built outside the project folder, which only ever holds Relay Dev builds.
$releaseTarget = Join-Path $env:LOCALAPPDATA "relay-release\target"
$env:CARGO_TARGET_DIR = $releaseTarget
Run "Build" { npx tauri build }
Remove-Item Env:CARGO_TARGET_DIR

$nsis = Join-Path $releaseTarget "release\bundle\nsis"
$setup = Get-ChildItem $nsis -Filter "Relay_${Version}_x64-setup.exe" | Select-Object -First 1
if (-not $setup) { Fail "Installer not found in $nsis" }

# Signed the way updater.rs checks it (minisign over the installer, Tauri's key format).
Run "Sign" { npx tauri signer sign -f $keyFile "--password=" $setup.FullName }
$sigFile = "$($setup.FullName).sig"
if (-not (Test-Path $sigFile)) { Fail "Signature not found: $sigFile" }

$latest = [ordered]@{
  version  = $Version
  notes    = $Notes.Trim()
  pub_date = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
  platforms = [ordered]@{
    "windows-x86_64" = [ordered]@{
      signature = (Get-Content $sigFile -Raw).Trim()
      url       = "https://github.com/$repo/releases/download/v$Version/$($setup.Name)"
    }
  }
}
$latestFile = Join-Path $nsis "latest.json"
[IO.File]::WriteAllText($latestFile, ($latest | ConvertTo-Json -Depth 5), $utf8)

$size = [math]::Round($setup.Length / 1MB, 2)
Write-Host "`nBuilt $($setup.Name) ($size MB), its signature and latest.json in $nsis" -ForegroundColor Green

git add package.json src-tauri/tauri.conf.json src-tauri/Cargo.toml src-tauri/Cargo.lock
git diff --cached --quiet
if ($LASTEXITCODE -ne 0) { git commit -m "Release $Version" | Out-Null }

if (-not $Publish) {
  Write-Host "Not published. Run again with -Publish (or upload the three files by hand) to release it."
  exit 0
}

$notesPath = Join-Path $env:TEMP "relay-notes-$Version.md"
[IO.File]::WriteAllText($notesPath, $Notes, $utf8)
Run "Push" { git push }
Run "GitHub release" { gh release create "v$Version" $setup.FullName $sigFile $latestFile --repo $repo --title "Relay $Version" --notes-file $notesPath }
Remove-Item $notesPath
Write-Host "Released v$Version. Installed copies pick it up on their next start." -ForegroundColor Green

# Tells the Discord channel behind the problem-report webhook (kept next to the signing key,
# never in the repo). A failed post doesn't undo the release.
$webhookFile = Join-Path $env:USERPROFILE ".relay-keys\report-webhook.txt"
if (Test-Path $webhookFile) {
  $webhook = (Get-Content $webhookFile -Raw).Trim()
  $post = @{
    username = "Relay"
    embeds   = @(@{
      title       = "Relay $Version is out"
      url         = "https://github.com/$repo/releases/tag/v$Version"
      description = $Notes.Trim().Substring(0, [Math]::Min($Notes.Trim().Length, 3500))
    })
  } | ConvertTo-Json -Depth 5
  try {
    Invoke-RestMethod -Uri $webhook -Method Post -ContentType "application/json; charset=utf-8" -Body ([Text.Encoding]::UTF8.GetBytes($post)) | Out-Null
    Write-Host "Posted the release to Discord."
  } catch {
    Write-Host "Couldn't post the release to Discord: $($_.Exception.Message)" -ForegroundColor Yellow
  }
}
