/**
 * @file usbd_midi.c
 * @brief USB MIDI 1.0 のクラス（F0 のダミー機器）。
 *
 * 送る列は main（UsbdMidiQueue）が入れ、USB の割込み（DataIn）が取り出す。
 * main は、列と送信中の印を触る間だけ USB の割込みを止める（短い区間）。
 */
#include "usbd_midi.h"

#include "usbd_ctlreq.h"
#include "usbd_ioreq.h"

#define MIDI_EP_IN       0x81U
#define MIDI_EP_OUT      0x01U
#define MIDI_EP_SIZE     64U
#define MIDI_EVENT_SIZE  4U
#define MIDI_QUEUE_SIZE  32U /* 2 の累乗 */
#define MIDI_CONFIG_SIZE 101U

typedef struct
{
    bool bConfigured;
    bool bTxBusy;
    uint32_t u32Head; /* main が進める */
    uint32_t u32Tail; /* 割込みが進める */
    uint8_t au8Queue[MIDI_QUEUE_SIZE][MIDI_EVENT_SIZE];
} MidiState;

static MidiState s_stMidi;
static __ALIGN_BEGIN uint8_t s_au8Tx[MIDI_EVENT_SIZE] __ALIGN_END;
static __ALIGN_BEGIN uint8_t s_au8Rx[MIDI_EP_SIZE] __ALIGN_END;
static uint8_t s_u8AltSetting = 0U;

/* 構成の記述子（MIDI 1.0 の付録 B と同じ形。埋め込みの IN／OUT ジャックと外部の IN／OUT ジャック） */
static __ALIGN_BEGIN uint8_t s_au8ConfigDesc[MIDI_CONFIG_SIZE] __ALIGN_END = {
    /* 構成 */
    0x09U, USB_DESC_TYPE_CONFIGURATION, LOBYTE(MIDI_CONFIG_SIZE), HIBYTE(MIDI_CONFIG_SIZE),
    0x02U, /* インタフェースの数 */
    0x01U, /* bConfigurationValue */
    USBD_IDX_CONFIG_STR,
    0x80U, /* PC の USB から電気を取る（USBD_SELF_POWERED 0 と合わせる） */
    USBD_MAX_POWER,
    /* Audio Control インタフェース（標準） */
    0x09U, USB_DESC_TYPE_INTERFACE, 0x00U, 0x00U, 0x00U, 0x01U, 0x01U, 0x00U, 0x00U,
    /* Audio Control インタフェース（クラス固有のヘッダ） */
    0x09U, 0x24U, 0x01U, 0x00U, 0x01U, 0x09U, 0x00U, 0x01U, 0x01U,
    /* MIDI Streaming インタフェース（標準） */
    0x09U, USB_DESC_TYPE_INTERFACE, 0x01U, 0x00U, 0x02U, 0x01U, 0x03U, 0x00U, USBD_IDX_INTERFACE_STR,
    /* MIDI Streaming のヘッダ（クラス固有の部分の長さ 65） */
    0x07U, 0x24U, 0x01U, 0x00U, 0x01U, 0x41U, 0x00U,
    /* MIDI IN ジャック（埋め込み、ID 1） */
    0x06U, 0x24U, 0x02U, 0x01U, 0x01U, 0x00U,
    /* MIDI IN ジャック（外部、ID 2） */
    0x06U, 0x24U, 0x02U, 0x02U, 0x02U, 0x00U,
    /* MIDI OUT ジャック（埋め込み、ID 3。元は外部の IN ジャック 2）。PC へ向かう */
    0x09U, 0x24U, 0x03U, 0x01U, 0x03U, 0x01U, 0x02U, 0x01U, 0x00U,
    /* MIDI OUT ジャック（外部、ID 4。元は埋め込みの IN ジャック 1） */
    0x09U, 0x24U, 0x03U, 0x02U, 0x04U, 0x01U, 0x01U, 0x01U, 0x00U,
    /* バルク OUT のエンドポイント（標準。オーディオ用の 9 バイトの形） */
    0x09U, USB_DESC_TYPE_ENDPOINT, MIDI_EP_OUT, 0x02U, LOBYTE(MIDI_EP_SIZE), HIBYTE(MIDI_EP_SIZE),
    0x00U, 0x00U, 0x00U,
    /* バルク OUT（クラス固有。埋め込みの IN ジャック 1） */
    0x05U, 0x25U, 0x01U, 0x01U, 0x01U,
    /* バルク IN のエンドポイント（標準） */
    0x09U, USB_DESC_TYPE_ENDPOINT, MIDI_EP_IN, 0x02U, LOBYTE(MIDI_EP_SIZE), HIBYTE(MIDI_EP_SIZE),
    0x00U, 0x00U, 0x00U,
    /* バルク IN（クラス固有。埋め込みの OUT ジャック 3） */
    0x05U, 0x25U, 0x01U, 0x01U, 0x03U,
};

/** 列の先頭を送る。割込みの中か、USB の割込みを止めた main から呼ぶ。 */
static void SendNext(USBD_HandleTypeDef *phDev)
{
    if (s_stMidi.u32Tail == s_stMidi.u32Head)
    {
        s_stMidi.bTxBusy = false;
        return;
    }
    USBD_memcpy(s_au8Tx, s_stMidi.au8Queue[s_stMidi.u32Tail & (MIDI_QUEUE_SIZE - 1U)],
                MIDI_EVENT_SIZE);
    s_stMidi.u32Tail++;
    s_stMidi.bTxBusy = true;
    if (USBD_LL_Transmit(phDev, MIDI_EP_IN, s_au8Tx, MIDI_EVENT_SIZE) != USBD_OK)
    {
        s_stMidi.bTxBusy = false;
    }
}

static uint8_t MidiInit(USBD_HandleTypeDef *phDev, uint8_t u8CfgIdx)
{
    (void)u8CfgIdx;
    (void)USBD_LL_OpenEP(phDev, MIDI_EP_IN, USBD_EP_TYPE_BULK, MIDI_EP_SIZE);
    phDev->ep_in[MIDI_EP_IN & 0x0FU].is_used = 1U;
    (void)USBD_LL_OpenEP(phDev, MIDI_EP_OUT, USBD_EP_TYPE_BULK, MIDI_EP_SIZE);
    phDev->ep_out[MIDI_EP_OUT & 0x0FU].is_used = 1U;
    (void)USBD_LL_PrepareReceive(phDev, MIDI_EP_OUT, s_au8Rx, MIDI_EP_SIZE);
    s_stMidi.u32Head = 0U;
    s_stMidi.u32Tail = 0U;
    s_stMidi.bTxBusy = false;
    s_stMidi.bConfigured = true;
    phDev->pClassData = &s_stMidi;
    return (uint8_t)USBD_OK;
}

static uint8_t MidiDeInit(USBD_HandleTypeDef *phDev, uint8_t u8CfgIdx)
{
    (void)u8CfgIdx;
    s_stMidi.bConfigured = false;
    s_stMidi.bTxBusy = false;
    (void)USBD_LL_CloseEP(phDev, MIDI_EP_IN);
    phDev->ep_in[MIDI_EP_IN & 0x0FU].is_used = 0U;
    (void)USBD_LL_CloseEP(phDev, MIDI_EP_OUT);
    phDev->ep_out[MIDI_EP_OUT & 0x0FU].is_used = 0U;
    phDev->pClassData = NULL;
    return (uint8_t)USBD_OK;
}

static uint8_t MidiSetup(USBD_HandleTypeDef *phDev, USBD_SetupReqTypedef *pstReq)
{
    static __ALIGN_BEGIN uint8_t s_au8Status[2] __ALIGN_END = {0U, 0U};

    if ((pstReq->bmRequest & USB_REQ_TYPE_MASK) != USB_REQ_TYPE_STANDARD)
    {
        USBD_CtlError(phDev, pstReq); /* クラス固有の要求は持たない */
        return (uint8_t)USBD_FAIL;
    }
    switch (pstReq->bRequest)
    {
    case USB_REQ_GET_STATUS:
        (void)USBD_CtlSendData(phDev, s_au8Status, 2U);
        return (uint8_t)USBD_OK;
    case USB_REQ_GET_INTERFACE:
        (void)USBD_CtlSendData(phDev, &s_u8AltSetting, 1U);
        return (uint8_t)USBD_OK;
    case USB_REQ_SET_INTERFACE:
        if (pstReq->wValue == 0U)
        {
            return (uint8_t)USBD_OK; /* 代替の設定は 0 だけ */
        }
        break;
    case USB_REQ_CLEAR_FEATURE:
        return (uint8_t)USBD_OK;
    default:
        break;
    }
    USBD_CtlError(phDev, pstReq);
    return (uint8_t)USBD_FAIL;
}

static uint8_t MidiDataIn(USBD_HandleTypeDef *phDev, uint8_t u8EpNum)
{
    if (u8EpNum == (MIDI_EP_IN & 0x0FU))
    {
        SendNext(phDev);
    }
    return (uint8_t)USBD_OK;
}

static uint8_t MidiDataOut(USBD_HandleTypeDef *phDev, uint8_t u8EpNum)
{
    if (u8EpNum == MIDI_EP_OUT)
    {
        /* PC からの MIDI は使わない。受けて捨て、次を受ける */
        (void)USBD_LL_PrepareReceive(phDev, MIDI_EP_OUT, s_au8Rx, MIDI_EP_SIZE);
    }
    return (uint8_t)USBD_OK;
}

static uint8_t *MidiGetConfigDesc(uint16_t *pu16Length)
{
    *pu16Length = (uint16_t)sizeof(s_au8ConfigDesc);
    return s_au8ConfigDesc;
}

USBD_ClassTypeDef g_stUsbdMidiClass = {
    MidiInit,
    MidiDeInit,
    MidiSetup,
    NULL, /* EP0_TxSent */
    NULL, /* EP0_RxReady */
    MidiDataIn,
    MidiDataOut,
    NULL, /* SOF */
    NULL, /* IsoINIncomplete */
    NULL, /* IsoOUTIncomplete */
    MidiGetConfigDesc, /* 高速は使わないが、同じものを返す */
    MidiGetConfigDesc,
    MidiGetConfigDesc,
    NULL, /* 高速を使わないので Device Qualifier は持たない */
};

bool UsbdMidiQueue(USBD_HandleTypeDef *phDev, const uint8_t au8Event[4])
{
    bool bQueued = false;

    HAL_NVIC_DisableIRQ(OTG_FS_IRQn);
    if (s_stMidi.bConfigured && (phDev->dev_state == USBD_STATE_CONFIGURED) &&
        ((s_stMidi.u32Head - s_stMidi.u32Tail) < MIDI_QUEUE_SIZE))
    {
        USBD_memcpy(s_stMidi.au8Queue[s_stMidi.u32Head & (MIDI_QUEUE_SIZE - 1U)], au8Event,
                    MIDI_EVENT_SIZE);
        s_stMidi.u32Head++;
        bQueued = true;
        if (!s_stMidi.bTxBusy)
        {
            SendNext(phDev);
        }
    }
    HAL_NVIC_EnableIRQ(OTG_FS_IRQn);
    return bQueued;
}
