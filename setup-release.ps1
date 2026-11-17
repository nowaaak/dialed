# Richtet Repo, Signaturschluessel, Secrets und das erste Release ein.
# Laeuft mit deiner eigenen GitHub-Anmeldung (gh). Mehrfaches Ausfuehren ist unproblematisch.
$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

function Step($text) { Write-Host "`n== $text" -ForegroundColor Cyan }
function Run {
    & $args[0] $args[1..($args.Length - 1)]
    if ($LASTEXITCODE -ne 0) { throw "Befehl fehlgeschlagen: $($args -join ' ')" }
}

Step 'Werkzeuge pruefen'
foreach ($tool in 'git', 'gh') {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) { throw "$tool ist nicht installiert" }
}
$owner = (gh api user --jq .login)
if ($LASTEXITCODE -ne 0 -or -not $owner) { throw 'gh ist nicht angemeldet. Erst "gh auth login" ausfuehren.' }
$repo = "$owner/dialed"
Write-Host "Angemeldet als $owner, Ziel-Repo: $repo"
if ($owner -ne 'nowaaak') {
    Write-Host 'Hinweis: In src-tauri\tauri.conf.json steht als Update-Adresse nowaaak/dialed. Bitte anpassen.' -ForegroundColor Yellow
}

Step 'Signaturschluessel aus dem Projektordner holen'
$keyDir = Join-Path $env:USERPROFILE '.tauri'
$key = Join-Path $keyDir 'dialed.key'
$stray = Join-Path $PSScriptRoot '$HOME'
if (Test-Path -LiteralPath (Join-Path $stray '.tauri\dialed.key')) {
    New-Item -ItemType Directory -Force $keyDir | Out-Null
    Get-ChildItem -LiteralPath (Join-Path $stray '.tauri') -Filter 'dialed.key*' | ForEach-Object {
        Move-Item -LiteralPath $_.FullName -Destination $keyDir -Force
    }
    Remove-Item -LiteralPath $stray -Recurse -Force
    Write-Host "Schluessel verschoben nach $keyDir"
}
if (-not (Test-Path -LiteralPath $key)) { throw "Schluesseldatei fehlt: $key" }
Write-Host "Schluessel liegt unter $key"

Step 'Release-Workflow an seinen Platz legen'
$workflow = '.github\workflows\release.yml'
if ((Test-Path 'ci\release.yml') -and -not (Test-Path $workflow)) {
    New-Item -ItemType Directory -Force '.github\workflows' | Out-Null
    Move-Item 'ci\release.yml' $workflow
}
if ((Test-Path 'ci') -and -not (Get-ChildItem 'ci')) { Remove-Item 'ci' }
if (-not (Test-Path $workflow)) { throw "Workflow fehlt: $workflow" }

Step 'Git-Repository vorbereiten'
if (-not (Test-Path '.git')) { Run git init -b main }
Run git add -A
$staged = git ls-files
if ($staged | Where-Object { $_ -match '(^|/)dialed\.key$' -or $_ -like '$HOME/*' }) {
    throw 'Der private Schluessel waere im Commit gelandet. Abbruch.'
}
git diff --cached --quiet
if ($LASTEXITCODE -ne 0) { Run git commit -m 'Dialed: Windows-Tweaking-Tool mit Zustandserkennung' }

Step 'Repo auf GitHub anlegen und pushen'
cmd /c "gh repo view $repo --json name >nul 2>&1"
if ($LASTEXITCODE -ne 0) {
    Run gh repo create $repo --public --description 'Windows tweaking tool with state detection (Rust + Tauri)' --source . --remote origin
}
if (-not (git remote | Where-Object { $_ -eq 'origin' })) { Run git remote add origin "https://github.com/$repo.git" }
git push -u origin main
if ($LASTEXITCODE -ne 0) {
    Write-Host 'Push abgelehnt. Meist fehlt dem Token das Recht fuer Workflow-Dateien, ich fordere es an ...' -ForegroundColor Yellow
    Run gh auth refresh -h github.com -s workflow
    Run gh auth setup-git
    Run git push -u origin main
}

Step 'Secrets fuer die Signatur hinterlegen'
Run gh secret set TAURI_SIGNING_PRIVATE_KEY --repo $repo --body ((Get-Content -LiteralPath $key -Raw).Trim())
$secure = Read-Host 'Passwort des Signaturschluessels (leer lassen, falls keins gesetzt wurde)' -AsSecureString
$plain = [Runtime.InteropServices.Marshal]::PtrToStringBSTR([Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure))
if ($plain) {
    Run gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD --repo $repo --body $plain
} else {
    Write-Host 'Kein Passwort eingegeben, das Passwort-Secret bleibt leer.'
}
$plain = $null

Step 'Erstes Release'
$version = (Get-Content 'src-tauri\tauri.conf.json' -Raw | ConvertFrom-Json).version
$tag = "v$version"
if (git tag --list $tag) {
    Write-Host "Tag $tag existiert schon. Fuer ein neues Release die Version erhoehen und neu taggen."
} elseif ((Read-Host "Release $tag jetzt bauen lassen? (j/n)") -eq 'j') {
    Run git tag $tag
    Run git push origin $tag
    Write-Host "Der Build laeuft: https://github.com/$repo/actions" -ForegroundColor Green
    Write-Host "Danach liegt der Installer unter: https://github.com/$repo/releases"
}

Write-Host "`nFertig." -ForegroundColor Green
