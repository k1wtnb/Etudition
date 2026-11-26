/* dummy_cmd.c の試験（PC の gcc で動かす）。
 * 実行：gcc -std=c11 -Wall -Wextra -Werror -IApp/Inc App/Src/dummy_cmd.c App/Test/test_dummy_cmd.c -o build/test_dummy_cmd && build/test_dummy_cmd
 */
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>

#include "dummy_cmd.h"

static uint32_t s_u32Failures = 0U;

static void ExpectOk(const char *szLine, DummyCmdKind eKind, uint8_t u8Note, uint8_t u8Velocity)
{
    DummyCmd stCmd = {DUMMY_CMD_KIND_PING, 0xFFU, 0xFFU};
    if (!DummyCmdParse(szLine, &stCmd) || (stCmd.eKind != eKind) || (stCmd.u8Note != u8Note) ||
        (stCmd.u8Velocity != u8Velocity))
    {
        printf("NG（読めるはず）: \"%s\"\n", szLine);
        s_u32Failures++;
    }
}

static void ExpectError(const char *szLine)
{
    DummyCmd stCmd = {DUMMY_CMD_KIND_NOTE_ON, 1U, 2U};
    if (DummyCmdParse(szLine, &stCmd) || (stCmd.u8Note != 1U) || (stCmd.u8Velocity != 2U))
    {
        printf("NG（誤りのはず）: \"%s\"\n", szLine);
        s_u32Failures++;
    }
}

int main(void)
{
    ExpectOk("ping", DUMMY_CMD_KIND_PING, 0U, 0U);
    ExpectOk("on 60 100", DUMMY_CMD_KIND_NOTE_ON, 60U, 100U);
    ExpectOk("on  0  1", DUMMY_CMD_KIND_NOTE_ON, 0U, 1U);
    ExpectOk("on 127 127", DUMMY_CMD_KIND_NOTE_ON, 127U, 127U);
    ExpectOk("off 60", DUMMY_CMD_KIND_NOTE_OFF, 60U, 0U);

    ExpectError("");
    ExpectError("pong");
    ExpectError("ping ");
    ExpectError("on 60");
    ExpectError("on 60 0");     /* ベロシティ 0 は Note Off の意味になるので受けない */
    ExpectError("on 128 100");
    ExpectError("on 60 128");
    ExpectError("on 60 100 1");
    ExpectError("on60 100");
    ExpectError("on -1 100");
    ExpectError("off");
    ExpectError("off 60 0");
    ExpectError("onion 60 100");
    ExpectError("on 99999999999 1");
    ExpectError("on 00000000000000000000000060 100"); /* 長すぎる行（33 文字） */
    ExpectError(NULL);

    if (s_u32Failures != 0U)
    {
        printf("失敗 %" PRIu32 " 件\n", s_u32Failures);
        return EXIT_FAILURE;
    }
    printf("dummy_cmd: すべて通った\n");
    return EXIT_SUCCESS;
}
