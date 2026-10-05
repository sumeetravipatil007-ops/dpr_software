$ErrorActionPreference = 'Stop'
$repo = 'D:\INTERNSHIP\OpenCADStudio-main'
$remote = 'https://github.com/sumeetravipatil007-ops/dpr_software.git'
$logFile = Join-Path $repo 'git_push_log.txt'
$git = 'C:\Program Files\Git\cmd\git.exe'

"Starting Git push attempt" | Out-File -FilePath $logFile

Set-Location $repo

# Ensure git is available
if (-not (Test-Path $git)) {
    "Git not found at $git" | Out-File -FilePath $logFile -Append
    exit 1
}

# Initialize repo if needed
if (-not (Test-Path (Join-Path $repo '.git'))) {
    & $git init | Out-File -FilePath $logFile -Append
}

# Configure commit author, if needed
& $git config user.name "Sumeet" | Out-File -FilePath $logFile -Append
& $git config user.email "sumeet@example.com" | Out-File -FilePath $logFile -Append

# Add remote
try {
    & $git remote remove origin 2>$null | Out-Null
} catch {}
& $git remote add origin $remote | Out-File -FilePath $logFile -Append

# Stage and commit if there are changes
& $git add . | Out-File -FilePath $logFile -Append
$hasChanges = (& $git status --porcelain)
if ($hasChanges) {
    & $git commit -m "Initial project upload" | Out-File -FilePath $logFile -Append
}

# Push
try {
    & $git branch -M main | Out-File -FilePath $logFile -Append
    & $git push -u origin main | Tee-Object -FilePath $logFile -Append
    "SUCCESS: push completed" | Out-File -FilePath $logFile -Append
    exit 0
} catch {
    "ERROR: push failed" | Out-File -FilePath $logFile -Append
    $_ | Out-File -FilePath $logFile -Append
    exit 1
}
