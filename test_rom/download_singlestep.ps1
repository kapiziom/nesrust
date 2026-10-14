# Downloads the first part (default 400 KB, a few hundred tests) of every opcode file of SingleStepTests/65x02
# (the nes6502 set: the 6502 inside the 2A03, no decimal mode) into test_rom/singlestep/.
# Usage: powershell -File test_rom/download_singlestep.ps1 [-Bytes 400000]
param([int]$Bytes = 400000)

$base = 'https://raw.githubusercontent.com/SingleStepTests/65x02/main/nes6502/v1'
$out = Join-Path $PSScriptRoot 'singlestep'
New-Item -ItemType Directory -Force $out | Out-Null

foreach ($opcode in 0..255) {
    $name = '{0:x2}' -f $opcode
    $file = Join-Path $out "$name.json"
    if (Test-Path $file) { continue }

    $tmp = "$file.part"
    & curl.exe -s -f -r "0-$($Bytes - 1)" "$base/$name.json" -o $tmp
    if (-not (Test-Path $tmp) -or (Get-Item $tmp).Length -eq 0) {
        Write-Host "no tests for $name"
        Remove-Item $tmp -ErrorAction SilentlyContinue
        continue
    }

    # every test sits on its own line: keep the complete ones and close the array
    $lines = Get-Content $tmp
    Remove-Item $tmp
    $tests = $lines | Where-Object { $_ -match '^\{.*\},?$' }
    if ($lines.Count -gt 0 -and $lines[-1] -notmatch '^\]') {
        $tests = $tests | Select-Object -SkipLast 1 # the last line may have been cut in the middle
    }
    $tests = $tests | ForEach-Object { $_.TrimEnd(',') }
    Set-Content -Path $file -Value ('[' + ($tests -join ",`n") + ']') -Encoding ascii
    Write-Host "$name : $($tests.Count) tests"
}
