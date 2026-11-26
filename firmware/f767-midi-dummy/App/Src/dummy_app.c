/**
 * @file dummy_app.c
 * @brief F0 のダミー機器の本体（シリアルの指示、ボタン、LED）。
 *
 * 割込みで行うのは、受けた1文字を列に入れて受信をかけ直すことだけ。
 * 行の組み立て、指示の実行、返事の送信は main の繰り返し（DummyAppPoll）で行う。
 */
#include "dummy_app.h"

#include <string.h>

#include "dummy_cmd.h"
#include "dummy_midi.h"
#include "main.h"
#include "usart.h"

/** 受けた文字の列の大きさ（2 の累乗）。 */
#define DUMMY_APP_RX_SIZE 256U
/** ボタンの値が続けて同じなら確定とみなす時間。 */
#define DUMMY_APP_DEBOUNCE_MS 20U
/** 動いている印の点滅の半周期。 */
#define DUMMY_APP_HEARTBEAT_MS 500U
/** 誤りの印（LD3）をつけておく時間。 */
#define DUMMY_APP_ERROR_LED_MS 200U
/** 返事を送るときの待ちの上限。 */
#define DUMMY_APP_TX_TIMEOUT_MS 10U
/** ボタンで送る音。 */
#define DUMMY_APP_BUTTON_NOTE 60U
#define DUMMY_APP_BUTTON_VELOCITY 100U
/** 文字を失ったことを main に知らせる印（列の中で、壊れた行の終わりの代わりに入る）。 */
#define DUMMY_APP_RX_LOST 0x15U

/* 割込み（書く側）と main（読む側）が1つずつの列。添字は 32 ビットで、読み書きは1命令で済む */
static uint8_t s_au8Rx[DUMMY_APP_RX_SIZE];
static volatile uint32_t s_u32RxHead = 0U; /**< 割込みだけが進める */
static volatile uint32_t s_u32RxTail = 0U; /**< main だけが進める */
static uint8_t s_u8RxByte = 0U;
static bool s_bRxDropping = false; /**< 割込みだけが使う。文字を失った後、次の改行まで捨てる */

static char s_szLine[DUMMY_CMD_LINE_MAX + 1U];
static uint32_t s_u32LineLength = 0U;
static bool s_bLineBad = false; /**< 長すぎる行か、NUL を含む行 */

static uint32_t s_au32Held[4]; /**< 押されている音（128 ビット） */
static bool s_bButtonDown = false;
static bool s_bButtonRaw = false;
static uint32_t s_u32ButtonChangedMs = 0U;
static uint32_t s_u32HeartbeatMs = 0U;
static uint32_t s_u32ErrorLedMs = 0U;
static bool s_bErrorLedOn = false;

static void StartReceive(void)
{
    (void)HAL_UART_Receive_IT(&huart3, &s_u8RxByte, 1U);
}

static void Reply(const char *szText)
{
    (void)HAL_UART_Transmit(&huart3, (const uint8_t *)szText, (uint16_t)strlen(szText),
                            DUMMY_APP_TX_TIMEOUT_MS);
}

static void ShowError(void)
{
    s_bErrorLedOn = true;
    s_u32ErrorLedMs = HAL_GetTick();
    HAL_GPIO_WritePin(LD3_GPIO_Port, LD3_Pin, GPIO_PIN_SET);
}

static void UpdateHeldLed(void)
{
    bool bAny = (s_au32Held[0] | s_au32Held[1] | s_au32Held[2] | s_au32Held[3]) != 0U;
    HAL_GPIO_WritePin(LD1_GPIO_Port, LD1_Pin, bAny ? GPIO_PIN_SET : GPIO_PIN_RESET);
}

/** 指示を実行する。送れたら true。 */
static bool Execute(const DummyCmd *pstCmd)
{
    uint32_t u32Word = (uint32_t)pstCmd->u8Note >> 5;
    uint32_t u32Bit = 1UL << (pstCmd->u8Note & 31U);

    if (!DummyMidiSend(pstCmd))
    {
        return false;
    }
    if (pstCmd->eKind == DUMMY_CMD_KIND_NOTE_ON)
    {
        s_au32Held[u32Word] |= u32Bit;
    }
    else
    {
        s_au32Held[u32Word] &= ~u32Bit;
    }
    UpdateHeldLed();
    return true;
}

static void HandleLine(void)
{
    DummyCmd stCmd;

    if (!s_bLineBad && (s_u32LineLength == 0U))
    {
        return; /* 空の行は区切り直しとして黙って捨てる */
    }
    if (s_bLineBad || !DummyCmdParse(s_szLine, &stCmd))
    {
        Reply("err\n");
        ShowError();
    }
    else if (stCmd.eKind == DUMMY_CMD_KIND_PING)
    {
        Reply("pong\n");
    }
    else
    {
        Reply(Execute(&stCmd) ? "ok\n" : "busy\n");
    }
    s_u32LineLength = 0U;
    s_bLineBad = false;
    s_szLine[0] = '\0';
}

static void PollSerial(void)
{
    while (s_u32RxTail != s_u32RxHead)
    {
        __DMB(); /* 添字を読んでから文字を読む */
        char cByte = (char)s_au8Rx[s_u32RxTail & (DUMMY_APP_RX_SIZE - 1U)];
        s_u32RxTail++;

        if ((uint8_t)cByte == DUMMY_APP_RX_LOST)
        {
            /* この行の途中の文字を割込みが失った。行を捨てて err を返す */
            s_bLineBad = true;
            HandleLine();
            continue;
        }
        if (cByte == '\r')
        {
            continue;
        }
        if (cByte == '\0')
        {
            s_bLineBad = true;
            continue;
        }
        if (cByte == '\n')
        {
            HandleLine();
            continue;
        }
        if (s_u32LineLength < DUMMY_CMD_LINE_MAX)
        {
            s_szLine[s_u32LineLength] = cByte;
            s_u32LineLength++;
            s_szLine[s_u32LineLength] = '\0';
        }
        else
        {
            s_bLineBad = true;
        }
    }
}

static void PollButton(uint32_t u32NowMs)
{
    bool bRaw = HAL_GPIO_ReadPin(USER_Btn_GPIO_Port, USER_Btn_Pin) == GPIO_PIN_SET;
    DummyCmd stCmd;

    if (bRaw != s_bButtonRaw)
    {
        s_bButtonRaw = bRaw;
        s_u32ButtonChangedMs = u32NowMs;
        return;
    }
    if ((bRaw == s_bButtonDown) || ((u32NowMs - s_u32ButtonChangedMs) < DUMMY_APP_DEBOUNCE_MS))
    {
        return;
    }
    stCmd.eKind = bRaw ? DUMMY_CMD_KIND_NOTE_ON : DUMMY_CMD_KIND_NOTE_OFF;
    stCmd.u8Note = DUMMY_APP_BUTTON_NOTE;
    stCmd.u8Velocity = bRaw ? DUMMY_APP_BUTTON_VELOCITY : 0U;
    /* 送れなかったときは確定させず、次の周回で送り直す */
    if (Execute(&stCmd))
    {
        s_bButtonDown = bRaw;
    }
}

static void PollLeds(void)
{
    /* 時刻は、この周回で ShowError が記録した時刻より前にならないよう、ここで取る */
    uint32_t u32NowMs = HAL_GetTick();

    if ((u32NowMs - s_u32HeartbeatMs) >= DUMMY_APP_HEARTBEAT_MS)
    {
        s_u32HeartbeatMs = u32NowMs;
        HAL_GPIO_TogglePin(LD2_GPIO_Port, LD2_Pin);
    }
    if (s_bErrorLedOn && ((u32NowMs - s_u32ErrorLedMs) >= DUMMY_APP_ERROR_LED_MS))
    {
        s_bErrorLedOn = false;
        HAL_GPIO_WritePin(LD3_GPIO_Port, LD3_Pin, GPIO_PIN_RESET);
    }
}

void DummyAppInit(void)
{
    uint32_t u32NowMs = HAL_GetTick();

    memset(s_au32Held, 0, sizeof(s_au32Held));
    s_bButtonRaw = HAL_GPIO_ReadPin(USER_Btn_GPIO_Port, USER_Btn_Pin) == GPIO_PIN_SET;
    s_bButtonDown = false;
    s_u32ButtonChangedMs = u32NowMs;
    s_u32HeartbeatMs = u32NowMs;
    DummyMidiInit();
    StartReceive();
}

void DummyAppPoll(void)
{
    uint32_t u32NowMs = HAL_GetTick();

    PollSerial();
    PollButton(u32NowMs);
    PollLeds();
}

/* 割込みの中：1文字を列に入れて、受信をかけ直す */
static void PushRx(uint8_t u8Byte)
{
    s_au8Rx[s_u32RxHead & (DUMMY_APP_RX_SIZE - 1U)] = u8Byte;
    __DMB(); /* 文字を書いてから添字を進める（コンパイラの並べ替えも止める） */
    s_u32RxHead++;
}

void HAL_UART_RxCpltCallback(UART_HandleTypeDef *phUart)
{
    bool bHasRoom;

    if (phUart != &huart3)
    {
        return;
    }
    bHasRoom = (s_u32RxHead - s_u32RxTail) < DUMMY_APP_RX_SIZE;
    if (s_bRxDropping)
    {
        /* 失った後は、次の改行までを捨て、その改行の代わりに印を入れて行を区切り直す */
        if ((s_u8RxByte == (uint8_t)'\n') && bHasRoom)
        {
            PushRx(DUMMY_APP_RX_LOST);
            s_bRxDropping = false;
        }
    }
    else if (bHasRoom)
    {
        PushRx(s_u8RxByte);
    }
    else
    {
        s_bRxDropping = true;
    }
    StartReceive();
}

/* 割込みの中：受信の誤り（オーバーランなど）。次の改行まで捨てる状態にして、受信をかけ直す */
void HAL_UART_ErrorCallback(UART_HandleTypeDef *phUart)
{
    if (phUart != &huart3)
    {
        return;
    }
    s_bRxDropping = true;
    StartReceive();
}
