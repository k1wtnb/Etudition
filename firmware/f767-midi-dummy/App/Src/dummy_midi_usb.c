/**
 * @file dummy_midi_usb.c
 * @brief MIDI の送り先：利用者用の USB 端子（OTG FS）の USB MIDI。
 */
#include "dummy_midi.h"

#include "main.h"
#include "usbd_core.h"
#include "usbd_desc.h"
#include "usbd_midi.h"

#define MIDI_CABLE_0_NOTE_OFF 0x08U /* ケーブル 0、CIN 0x8 */
#define MIDI_CABLE_0_NOTE_ON  0x09U /* ケーブル 0、CIN 0x9 */
#define MIDI_STATUS_NOTE_OFF  0x80U /* チャンネル 1 */
#define MIDI_STATUS_NOTE_ON   0x90U /* チャンネル 1 */

static USBD_HandleTypeDef s_hUsbDevice;

void DummyMidiInit(void)
{
    /* 失敗しても止めない（シリアルとボタンは動かし、送るときに busy を返す） */
    if (USBD_Init(&s_hUsbDevice, &g_stDummyUsbDesc, 0U) != USBD_OK)
    {
        return;
    }
    if (USBD_RegisterClass(&s_hUsbDevice, &g_stUsbdMidiClass) != USBD_OK)
    {
        return;
    }
    (void)USBD_Start(&s_hUsbDevice);
}

bool DummyMidiSend(const DummyCmd *pstCmd)
{
    uint8_t au8Event[4];

    if (pstCmd->eKind == DUMMY_CMD_KIND_NOTE_ON)
    {
        au8Event[0] = MIDI_CABLE_0_NOTE_ON;
        au8Event[1] = MIDI_STATUS_NOTE_ON;
        au8Event[3] = pstCmd->u8Velocity;
    }
    else if (pstCmd->eKind == DUMMY_CMD_KIND_NOTE_OFF)
    {
        au8Event[0] = MIDI_CABLE_0_NOTE_OFF;
        au8Event[1] = MIDI_STATUS_NOTE_OFF;
        au8Event[3] = 0x40U; /* 離す速さ（既定の 64） */
    }
    else
    {
        return false;
    }
    au8Event[2] = pstCmd->u8Note;
    return UsbdMidiQueue(&s_hUsbDevice, au8Event);
}
