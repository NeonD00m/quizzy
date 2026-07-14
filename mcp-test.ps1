# Config
$USER_ID = "idk"
$EXE_PATH = ".\target\release\quizzy.exe"

Write-Host "Running cargo build --release..." -ForegroundColor Cyan
cargo build --release

#Check if the build actually succeeded
if ($LASTEXITCODE -ne 0) {
    Write-Error "Cargo build failed. Aborting MCP Test."
    exit $LASTEXITCODE
}

Write-Host "Starting MCP Inspector with USER_ID=$USER_ID..." -ForegroundColor Cyan
$env:USER_ID = USER_ID
npx @modelcontextprotocol/inspector $EXE_PATH
