/**
 * @file dummy_app.h
 * @brief F0 のダミー機器の本体（シリアルの指示、ボタン、LED）。
 *
 * - シリアル：ST-LINK の仮想 COM ポート（USART3、115200 bps、8N1）で1行の指示を受け、
 *   `ok`（送った）、`busy`（送れなかった）、`pong`、`err`（読めない行）を1行で返す。
 * - ボタン（B1）：押すと `on 60 100`、離すと `off 60` と同じ。
 * - LED：LD2（青）は動いている印（0.5 秒ごとに点滅）。LD1（緑）は押されている音がある間つく。
 *   LD3（赤）は、読めない行か、受けた文字を失った行で 0.2 秒つく（その行は実行しない）。
 */
#ifndef DUMMY_APP_H
#define DUMMY_APP_H

/** 起動時に1回呼ぶ（周辺機器の初期化の後）。 */
void DummyAppInit(void);

/** main の繰り返しから呼ぶ（待たない）。 */
void DummyAppPoll(void);

#endif /* DUMMY_APP_H */
