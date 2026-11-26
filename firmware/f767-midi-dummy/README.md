# F0 ダミー機器（NUCLEO-F767ZI）

楽器が届くまで MIDI の入力の代わりにする、試験用のファームです（計画の F0）。

- 利用者用の USB 端子（CN13、OTG FS）から、USB MIDI の機器として Note On と Note Off を送ります。チャンネルは 1 です。
- 送る音の指示は、PC から ST-LINK の仮想 COM ポート（USART3、115200 bps、8N1）で1行ずつ受けます。
- ボタン（B1）でも音を出せます。押すとノート 60、ベロシティ 100 の Note On、離すと Note Off です。

## 指示と返事

指示は1行ずつ送ります（形は `App/Inc/dummy_cmd.h`）。

| 指示 | 意味 | 返事 |
|---|---|---|
| `ping` | つながっているかを確かめる | `pong` |
| `on <note> <vel>` | Note On（note は 0〜127、vel は 1〜127） | `ok`（USB MIDI の送る列に入れた）／`busy`（PC に MIDI 機器として認識されていない、または列が満杯） |
| `off <note>` | Note Off | `ok`／`busy` |
| 形や値が違う行、33 文字以上の行、NUL か 0x15 を含む行、途中の文字を失った行（受信の誤りを含む） | — | `err`（実行しない） |
| 空の行 | 区切り直し | 返事なし |

PC からは `send.ps1` で送ります。`send.ps1` は始めに、NUL と改行で機器に残った行を無効にし、`ping` に `pong` が返るまで待ってから指示を送ります。

```powershell
powershell -ExecutionPolicy Bypass -File firmware\f767-midi-dummy\send.ps1 COM5 ping
```

指示には `ping`、`on`、`off` のほかに `beat <回数> <間隔ms>` があります。`beat` は、ノート 60 を一定の間隔で押し、間隔の半分で離します。ただし、PC の待ちと返事の待ちを含むので、送る時刻の揺れを USB MIDI の受信の揺れとみなすことはできません。

## LED

| LED | 意味 |
|---|---|
| LD2（青） | 動いている印。0.5 秒ごとに点滅する |
| LD1（緑） | 送った音のうち、押されているものがある間つく |
| LD3（赤） | `err` を返したときに 0.2 秒つく（`send.ps1` は始めに区切り直しで `err` を1回受けるので、毎回1度つく） |

## 作り

- CubeMX（`f767-midi-dummy.ioc`）で生成しました。STM32Cube FW_F7 V1.17.4 を使い、形式は CMake です。
- クロックは HSE（ST-LINK の 8 MHz）から PLL で作ります。SYSCLK は 96 MHz、USB は 48 MHz です。
- 使う周辺は次のとおりです。
  - USART3（PD8/PD9。ST-LINK の仮想 COM ポート）
  - USB OTG FS（Device。PA11/PA12、VBUS は PA9）
  - B1（PC13。割込みでなく、繰り返しの中で見る）
  - LD1〜LD3
- RTOS は使いません。
  - UART の割込みで行うのは、受けた1文字を列に入れることと、受信をかけ直すことだけです。文字を失ったときは、次の改行まで捨てて、その行に `err` を返します。
  - USB の割込みでは、ST の USB Device ライブラリが列挙を行い、送り終えたら次の MIDI のイベントを送ります。
  - それ以外は main の繰り返し（`DummyAppPoll`）で行います。
- USB MIDI は次の3つで作ります。
  - ST の USB Device ライブラリの Core（V2.11.4）。`Middlewares/` に写しました。
  - ライブラリと HAL の PCD のつなぎ（`App/Src/usbd_conf.c`）。
  - 自分で書いた MIDI のクラス（`App/Src/usbd_midi.c`。記述子は USB MIDI 1.0 の付録 B の形）。
- VID/PID は、pid.codes の試験用の 0x1209 / 0x0001 です。手元の試験だけに使い、配りません。
- キャッシュは有効にしていません。USB OTG FS は DMA を使いません（HAL が FS では DMA を切る）。そのため、キャッシュの規則（REQ-EMB-002）は当てません（人間の承認、2026-11-27）。
- 自分で書いたものは次のとおりです。生成コードの識別子は変えていません。
  - `App/` の中
  - `Core/Src/main.c` の `USER CODE` の中
  - `CMakeLists.txt` の利用者の行
  - `build.ps1`、`send.ps1`
- 生成したときの手直しが2点あります。CubeMX の画面から生成し直したときは、この2点を確かめてください。
  - CubeMX をスクリプトで動かすと、`cmake/stm32cubemx/CMakeLists.txt` に PC の絶対パスが入りました。これを `${CMAKE_CURRENT_SOURCE_DIR}/../../` に直しました。
  - `syscalls.c` と `sysmem.c` が作られませんでした。標準入出力も `malloc` も使わないので、外しました。

## ライセンス

- `Drivers/STM32F7xx_HAL_Driver` と `Middlewares/ST/STM32_USB_Device_Library` は BSD-3-Clause です。
  - USB Device ライブラリの `LICENSE.txt` は、パッケージの条件に従うと書いています。パッケージの部品表（同じフォルダに写した `Package_license.md`）では、STM32 USB Device Library は BSD-3-Clause です。
- `Drivers/CMSIS` は Apache-2.0 です。
- 生成したファイルの扱いは、各ファイルの先頭にある ST の表示に従います。

## ビルド（AI か人間）

STM32CubeIDE に同梱の GCC、CMake、Ninja を使います（`C:\ST\STM32CubeIDE*` にあること）。

```powershell
powershell -ExecutionPolicy Bypass -File firmware\f767-midi-dummy\build.ps1
```

できるもの：`firmware\f767-midi-dummy\build\Release\f767-midi-dummy.elf`

## パーサの試験（PC の gcc。Dev Container の中）

```sh
cd firmware/f767-midi-dummy && mkdir -p build && gcc -std=c11 -Wall -Wextra -Werror -pedantic -IApp/Inc App/Src/dummy_cmd.c App/Test/test_dummy_cmd.c -o build/test_dummy_cmd && build/test_dummy_cmd
```

## 書き込みと確かめ（人間）

人間の手順は、非公開の Etudition-dev の `planning/f0-human-steps.md` にあります。書き込みは人間が STM32CubeProgrammer で行い、オプションバイトは変えません。
