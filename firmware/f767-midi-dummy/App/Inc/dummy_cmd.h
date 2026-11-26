/**
 * @file dummy_cmd.h
 * @brief シリアルの指示（1行）を読む。HAL に依存しない（PC の試験でも動く）。
 *
 * 指示は ASCII の1行（改行で終わる。CR は無視する）：
 * - `ping`               つながっているかを確かめる（`pong` を返す）
 * - `on <note> <vel>`    Note On を送る（note は 0〜127、vel は 1〜127）
 * - `off <note>`         Note Off を送る（note は 0〜127）
 * チャンネルは 1 に固定する。空の行には返事をしない（PC が区切り直すのに使う。dummy_app.c）。
 */
#ifndef DUMMY_CMD_H
#define DUMMY_CMD_H

#include <stdbool.h>
#include <stdint.h>

/** 1行の長さの上限（改行を除く文字数）。超えた行は誤りにする。 */
#define DUMMY_CMD_LINE_MAX 32U

/** 指示の種類。 */
typedef enum
{
    DUMMY_CMD_KIND_PING = 0,
    DUMMY_CMD_KIND_NOTE_ON,
    DUMMY_CMD_KIND_NOTE_OFF
} DummyCmdKind;

/** 読んだ指示。 */
typedef struct
{
    DummyCmdKind eKind;
    uint8_t u8Note;     /**< ノート番号（0〜127） */
    uint8_t u8Velocity; /**< ベロシティ（Note On は 1〜127、Note Off は 0） */
} DummyCmd;

/**
 * @brief NUL 終端の1行を指示として読む。
 * @param szLine 改行を除いた1行
 * @param pstCmd 読めたときの指示の書き先
 * @return 読めたら true。形や値の範囲が違えば false（pstCmd は変えない）
 */
bool DummyCmdParse(const char *szLine, DummyCmd *pstCmd);

#endif /* DUMMY_CMD_H */
