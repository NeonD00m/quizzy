# Config
$EXE_PATH = ".\target\release\quizzy.exe"
$DB_PATH = "$env:LOCALAPPDATA\quizzy\quizzy.db"

Write-Host "Running cargo build --release..." -ForegroundColor Cyan
cargo build --release

#Check if the build actually succeeded
if ($LASTEXITCODE -ne 0) {
    Write-Error "Cargo build failed. Aborting MCP Test."
    exit $LASTEXITCODE
}

Write-Host "Starting MCP Inspector..." -ForegroundColor Cyan
$env:QUIZZY_DB = $DB_PATH
npx @modelcontextprotocol/inspector $EXE_PATH mcp
