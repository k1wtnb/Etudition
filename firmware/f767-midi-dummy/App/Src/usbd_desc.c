/**
 * @file usbd_desc.c
 * @brief USB の機器の記述子と文字列（F0 のダミー機器）。
 *
 * VID/PID は pid.codes の試験用（0x1209 / 0x0001。手元の試験だけに使い、配らない）。
 */
#include "usbd_desc.h"

#include "usbd_core.h"

#define DESC_VID        0x1209U
#define DESC_PID        0x0001U
#define DESC_LANGID     0x0409U
#define DESC_STR_MAKER  "Etudition"
#define DESC_STR_PROD   "Etudition F0 Dummy MIDI"
#define DESC_STR_SERIAL "F0-0001"
#define DESC_STR_CONFIG "MIDI"
#define DESC_STR_IFACE  "MIDI"

static uint8_t *GetDevice(USBD_SpeedTypeDef eSpeed, uint16_t *pu16Length);
static uint8_t *GetLangId(USBD_SpeedTypeDef eSpeed, uint16_t *pu16Length);
static uint8_t *GetMaker(USBD_SpeedTypeDef eSpeed, uint16_t *pu16Length);
static uint8_t *GetProduct(USBD_SpeedTypeDef eSpeed, uint16_t *pu16Length);
static uint8_t *GetSerial(USBD_SpeedTypeDef eSpeed, uint16_t *pu16Length);
static uint8_t *GetConfig(USBD_SpeedTypeDef eSpeed, uint16_t *pu16Length);
static uint8_t *GetInterface(USBD_SpeedTypeDef eSpeed, uint16_t *pu16Length);

USBD_DescriptorsTypeDef g_stDummyUsbDesc = {
    GetDevice, GetLangId, GetMaker, GetProduct, GetSerial, GetConfig, GetInterface,
};

/* 記述子は 4 バイト境界に置く（ライブラリの例に合わせる） */
static __ALIGN_BEGIN uint8_t s_au8DeviceDesc[USB_LEN_DEV_DESC] __ALIGN_END = {
    0x12U,                 /* bLength */
    USB_DESC_TYPE_DEVICE,  /* bDescriptorType */
    0x00U, 0x02U,          /* bcdUSB 2.00 */
    0x00U,                 /* bDeviceClass（インタフェースで決める） */
    0x00U,                 /* bDeviceSubClass */
    0x00U,                 /* bDeviceProtocol */
    USB_MAX_EP0_SIZE,      /* bMaxPacketSize0 */
    LOBYTE(DESC_VID), HIBYTE(DESC_VID),
    LOBYTE(DESC_PID), HIBYTE(DESC_PID),
    0x00U, 0x01U,          /* bcdDevice 1.00 */
    USBD_IDX_MFC_STR,      /* iManufacturer */
    USBD_IDX_PRODUCT_STR,  /* iProduct */
    USBD_IDX_SERIAL_STR,   /* iSerialNumber */
    USBD_MAX_NUM_CONFIGURATION,
};

static __ALIGN_BEGIN uint8_t s_au8LangIdDesc[USB_LEN_LANGID_STR_DESC] __ALIGN_END = {
    USB_LEN_LANGID_STR_DESC, USB_DESC_TYPE_STRING, LOBYTE(DESC_LANGID), HIBYTE(DESC_LANGID),
};

static __ALIGN_BEGIN uint8_t s_au8StrDesc[USBD_MAX_STR_DESC_SIZ] __ALIGN_END;

static uint8_t *GetDevice(USBD_SpeedTypeDef eSpeed, uint16_t *pu16Length)
{
    (void)eSpeed;
    *pu16Length = (uint16_t)sizeof(s_au8DeviceDesc);
    return s_au8DeviceDesc;
}

static uint8_t *GetLangId(USBD_SpeedTypeDef eSpeed, uint16_t *pu16Length)
{
    (void)eSpeed;
    *pu16Length = (uint16_t)sizeof(s_au8LangIdDesc);
    return s_au8LangIdDesc;
}

static uint8_t *ToString(const char *szText, uint16_t *pu16Length)
{
    USBD_GetString((uint8_t *)szText, s_au8StrDesc, pu16Length);
    return s_au8StrDesc;
}

static uint8_t *GetMaker(USBD_SpeedTypeDef eSpeed, uint16_t *pu16Length)
{
    (void)eSpeed;
    return ToString(DESC_STR_MAKER, pu16Length);
}

static uint8_t *GetProduct(USBD_SpeedTypeDef eSpeed, uint16_t *pu16Length)
{
    (void)eSpeed;
    return ToString(DESC_STR_PROD, pu16Length);
}

static uint8_t *GetSerial(USBD_SpeedTypeDef eSpeed, uint16_t *pu16Length)
{
    (void)eSpeed;
    return ToString(DESC_STR_SERIAL, pu16Length);
}

static uint8_t *GetConfig(USBD_SpeedTypeDef eSpeed, uint16_t *pu16Length)
{
    (void)eSpeed;
    return ToString(DESC_STR_CONFIG, pu16Length);
}

static uint8_t *GetInterface(USBD_SpeedTypeDef eSpeed, uint16_t *pu16Length)
{
    (void)eSpeed;
    return ToString(DESC_STR_IFACE, pu16Length);
}
