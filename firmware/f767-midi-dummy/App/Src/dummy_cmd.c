/**
 * @file dummy_cmd.c
 * @brief シリアルの指示（1行）を読む。
 */
#include "dummy_cmd.h"

#include <string.h>

/** 10 進の数を読む。読めた桁がなければ false。値が u32Max を超えたら false。 */
static bool ReadNumber(const char **pszCursor, uint32_t u32Max, uint32_t *pu32Value)
{
    const char *szCursor = *pszCursor;
    uint32_t u32Value = 0U;
    bool bHasDigit = false;

    while ((*szCursor >= '0') && (*szCursor <= '9'))
    {
        u32Value = (u32Value * 10U) + (uint32_t)(*szCursor - '0');
        if (u32Value > u32Max)
        {
            return false;
        }
        bHasDigit = true;
        szCursor++;
    }
    if (!bHasDigit)
    {
        return false;
    }
    *pszCursor = szCursor;
    *pu32Value = u32Value;
    return true;
}

/** 1個以上の空白を読み飛ばす。空白がなければ false。 */
static bool SkipSpaces(const char **pszCursor)
{
    const char *szCursor = *pszCursor;

    if (*szCursor != ' ')
    {
        return false;
    }
    while (*szCursor == ' ')
    {
        szCursor++;
    }
    *pszCursor = szCursor;
    return true;
}

bool DummyCmdParse(const char *szLine, DummyCmd *pstCmd)
{
    const char *szCursor = szLine;
    uint32_t u32Note = 0U;
    uint32_t u32Velocity = 0U;
    DummyCmd stCmd;

    if ((szLine == NULL) || (pstCmd == NULL) || (strlen(szLine) > DUMMY_CMD_LINE_MAX))
    {
        return false;
    }

    if (strcmp(szCursor, "ping") == 0)
    {
        stCmd.eKind = DUMMY_CMD_KIND_PING;
        stCmd.u8Note = 0U;
        stCmd.u8Velocity = 0U;
        *pstCmd = stCmd;
        return true;
    }

    if (strncmp(szCursor, "on", 2U) == 0)
    {
        szCursor += 2;
        if (!SkipSpaces(&szCursor) || !ReadNumber(&szCursor, 127U, &u32Note) ||
            !SkipSpaces(&szCursor) || !ReadNumber(&szCursor, 127U, &u32Velocity) ||
            (u32Velocity == 0U) || (*szCursor != '\0'))
        {
            return false;
        }
        stCmd.eKind = DUMMY_CMD_KIND_NOTE_ON;
    }
    else if (strncmp(szCursor, "off", 3U) == 0)
    {
        szCursor += 3;
        if (!SkipSpaces(&szCursor) || !ReadNumber(&szCursor, 127U, &u32Note) ||
            (*szCursor != '\0'))
        {
            return false;
        }
        stCmd.eKind = DUMMY_CMD_KIND_NOTE_OFF;
    }
    else
    {
        return false;
    }

    stCmd.u8Note = (uint8_t)u32Note;
    stCmd.u8Velocity = (uint8_t)u32Velocity;
    *pstCmd = stCmd;
    return true;
}
