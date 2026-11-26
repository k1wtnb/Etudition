# F0 のダミー機器へ、ST-LINK の仮想 COM ポートで指示を送り、返事を確かめる。
# 使い方：
#   powershell -ExecutionPolicy Bypass -File send.ps1 COM5 ping
#   powershell -ExecutionPolicy Bypass -File send.ps1 COM5 on 60 100
#   powershell -ExecutionPolicy Bypass -File send.ps1 COM5 off 60
#   powershell -ExecutionPolicy Bypass -File send.ps1 COM5 beat 20 250   # ノート 60 を 250 ms ごとに 20 回押し、半分で離す
# 返事が期待どおりでなければ、終了コード 1。引数の誤りは 2。
param(
    [Parameter(Mandatory = $true, Position = 0)][string]$Port,
    [Parameter(Mandatory = $true, Position = 1)][string]$Command,
    [Parameter(Position = 2)][int]$A = -1,
    [Parameter(Position = 3)][int]$B = -1
)
$ErrorActionPreference = "Stop"

function Fail-Usage([string]$Message) {
    [Console]::Error.WriteLine($Message)
    exit 2
}

# 送る行と送る時刻（ms）の組を作る（ファームの App/Inc/dummy_cmd.h と同じ形）
$plan = @()
switch ($Command) {
    "ping" { $plan += , @(0, "ping") }
    "on" {
        if ($A -lt 0 -or $A -gt 127 -or $B -lt 1 -or $B -gt 127) { Fail-Usage "on <ノート 0-127> <ベロシティ 1-127>" }
        $plan += , @(0, "on $A $B")
    }
    "off" {
        if ($A -lt 0 -or $A -gt 127) { Fail-Usage "off <ノート 0-127>" }
        $plan += , @(0, "off $A")
    }
    "beat" {
        if ($A -lt 1 -or $B -lt 20) { Fail-Usage "beat <回数 1 以上> <間隔 ms 20 以上>" }
        for ($k = 0; $k -lt $A; $k++) {
            $plan += , @(($k * $B), "on 60 100")
            $plan += , @(($k * $B + [int]($B / 2)), "off 60")
        }
    }
    default { Fail-Usage "指示は ping | on <ノート> <ベロシティ> | off <ノート> | beat <回数> <間隔ms>" }
}

$serial = New-Object System.IO.Ports.SerialPort $Port, 115200, ([System.IO.Ports.Parity]::None), 8, ([System.IO.Ports.StopBits]::One)
$serial.NewLine = "`n"
$serial.ReadTimeout = 500
$serial.WriteTimeout = 500
try {
    $serial.Open()   # ほかのソフトが開いていれば、ここで失敗する（排他で開く）
} catch {
    [Console]::Error.WriteLine("$Port を開けない: $($_.Exception.Message)")
    exit 1
}

$failed = 0
try {
    # 途中まで届いた行が残っていても、空の行で区切り直す（ファームは空の行に返事をしない）
    $serial.Write("`n")
    Start-Sleep -Milliseconds 50
    $serial.DiscardInBuffer()

    $clock = [System.Diagnostics.Stopwatch]::StartNew()
    foreach ($item in $plan) {
        $wait = $item[0] - $clock.ElapsedMilliseconds
        if ($wait -gt 0) { Start-Sleep -Milliseconds $wait }
        $line = $item[1]
        $serial.Write("$line`n")
        try {
            $reply = $serial.ReadLine().TrimEnd("`r")
        } catch [System.TimeoutException] {
            [Console]::Error.WriteLine("$line → 返事が 500 ms 来ない（ポート、配線、ファームを確かめる）")
            exit 1
        }
        $expected = if ($line -eq "ping") { "pong" } else { "ok" }
        if ($reply -ne $expected) {
            [Console]::Error.WriteLine("$line → $reply")
            $failed++
        } elseif ($plan.Count -eq 1) {
            Write-Output "$line → $reply"
        }
    }
    if ($plan.Count -gt 1) { Write-Output "送った行 $($plan.Count) 個。失敗 $failed 個。" }
} finally {
    $serial.Close()
}
if ($failed -gt 0) { exit 1 }
exit 0
