param(
    # 用法： . .\scripts\env.ps1    （点号调用，才会把 PATH 写进当前会话）
)

$cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
$mingwBin = Join-Path $env:USERPROFILE 'mingw-toolchain\mingw64\bin'

foreach ($dir in @($cargoBin, $mingwBin)) {
    if (-not (Test-Path $dir)) {
        Write-Warning "缺少目录: $dir —— 请先按 README 安装工具链"
        continue
    }
    if ($env:PATH -notlike "*$dir*") {
        $env:PATH = "$dir;$env:PATH"
    }
}

Write-Host "PATH 已就绪:" -ForegroundColor Green
Write-Host "  cargo : $((Get-Command cargo -ErrorAction SilentlyContinue).Source)"
Write-Host "  dlltool: $((Get-Command dlltool -ErrorAction SilentlyContinue).Source)"
