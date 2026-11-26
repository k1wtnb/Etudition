# F0 ダミー機器（NUCLEO-F767ZI）

楽器が届くまで MIDI の入力の代わりにする、試験用のファームです（計画の F0）。

- PC から ST-LINK の仮想 COM ポート（USART3、115200 bps、8N1）で1行の指示を受けます。
- ボタン（B1）でも音を出せます。押すとノート 60、ベロシティ 100 の Note On、離すと Note Off です。
- **USB MIDI の送信はまだ入っていません。** 使う USB の部品が決まるまで、`App/Src/dummy_midi_none.c`（何も送らず、受けたことにする）を使います。今の段階で確かめられるのは、シリアルの指示とボタンと LED の経路だけです。

## 指示と返事

指示は1行ずつ送ります（形は `App/Inc/dummy_cmd.h`）。チャンネルは 1 に固定です。

| 指示 | 意味 | 返事 |
|---|---|---|
| `ping` | つながっているかを確かめる | `pong` |
| `on <note> <vel>` | Note On（note は 0〜127、vel は 1〜127） | `ok`（送った）／`busy`（送れなかった） |
| `off <note>` | Note Off | `ok`／`busy` |
| 形や値が違う行、33 文字以上の行 | — | `err` |
| 空の行 | 区切り直し | 返事なし |

PC からは `etudition-cli midi-send <ポート> <指示>` で送ります（`crates/cli`）。指示は `ping`、`on`、`off` のほかに `beat <回数> <間隔ms>` があります。`beat` はノート 60 を一定の間隔で押し、間隔の半分で離します。S2b で、揺れを記録するために使います。

## LED

| LED | 意味 |
|---|---|
| LD2（青） | 動いている印。0.5 秒ごとに点滅する |
| LD1（緑） | 押されている音がある間つく |
| LD3（赤） | 読めない行を受けたとき、または受けた文字があふれたときに 0.2 秒つく |

## 作り

- CubeMX（`f767-midi-dummy.ioc`）で生成しました。STM32Cube FW_F7 V1.17.4 を使い、生成する形式は CMake です。
- クロックは HSE（ST-LINK の 8 MHz）から PLL で作ります。SYSCLK は 96 MHz、USB は 48 MHz です。
- 使う周辺：
  - USART3（PD8/PD9。ST-LINK の仮想 COM ポート）
  - USB OTG FS（Device、PA11/PA12、VBUS は PA9）
  - B1（PC13。割込みでなく、繰り返しの中で見る）
  - LD1〜LD3
- RTOS は使いません。割込みで行うのは、受けた1文字を列に入れて受信をかけ直すことだけです。それ以外は main の繰り返し（`DummyAppPoll`）で行います。
- キャッシュは有効にしていません。USB OTG FS は DMA を使いません（HAL が FS では DMA を切る）。そのため、キャッシュの規則（REQ-EMB-002）は当てません（人間の承認、2026-11-27）。
- 自分で書いたコードは `App/` と、`Core/Src/main.c` の `USER CODE` の中だけです。生成コードの識別子は変えていません。
- 生成したときの手直し：CubeMX をスクリプトで動かすと、`cmake/stm32cubemx/CMakeLists.txt` に PC の絶対パスが入りました。また、`syscalls.c` と `sysmem.c` が作られませんでした。そこで、パスを `${CMAKE_CURRENT_SOURCE_DIR}/../../` に直し、その2つのファイルは外しました（標準入出力も `malloc` も使わない）。CubeMX の画面から生成し直したときは、この2点を確かめてください。

## ライセンス

- `Drivers/STM32F7xx_HAL_Driver` は BSD-3-Clause、`Drivers/CMSIS` は Apache-2.0 です（それぞれの `LICENSE.txt`）。
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
