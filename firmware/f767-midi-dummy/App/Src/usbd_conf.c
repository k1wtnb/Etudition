/**
 * @file usbd_conf.c
 * @brief ST の USB Device ライブラリと HAL の PCD をつなぐ（F0 のダミー機器）。
 *
 * PCD の初期化とピン・クロック・割込みの設定は CubeMX の生成物（usb_otg.c の MX_USB_OTG_FS_PCD_Init と
 * HAL_PCD_MspInit）が行う。ここでは、ライブラリとハンドルを結び、FIFO を割り当て、呼び戻しを中継するだけ。
 * 中継の形は STM32Cube FW_F7 V1.17.4 の例（Projects/STM32F767ZI-Nucleo/Applications/USB_Device/
 * HID_Standalone/Src/usbd_conf.c。Copyright (c) 2016 STMicroelectronics、BSD-3-Clause）に倣った。
 * 関数名（HAL_PCD_*Callback、USBD_LL_*）はライブラリが決めたもので、変えない。
 */
#include "usbd_core.h"
#include "usb_otg.h"

/* ---- PCD（割込みの中）→ ライブラリ ---- */

void HAL_PCD_SetupStageCallback(PCD_HandleTypeDef *phPcd)
{
    (void)USBD_LL_SetupStage((USBD_HandleTypeDef *)phPcd->pData, (uint8_t *)phPcd->Setup);
}

void HAL_PCD_DataOutStageCallback(PCD_HandleTypeDef *phPcd, uint8_t u8EpNum)
{
    (void)USBD_LL_DataOutStage((USBD_HandleTypeDef *)phPcd->pData, u8EpNum,
                               phPcd->OUT_ep[u8EpNum].xfer_buff);
}

void HAL_PCD_DataInStageCallback(PCD_HandleTypeDef *phPcd, uint8_t u8EpNum)
{
    (void)USBD_LL_DataInStage((USBD_HandleTypeDef *)phPcd->pData, u8EpNum,
                              phPcd->IN_ep[u8EpNum].xfer_buff);
}

void HAL_PCD_SOFCallback(PCD_HandleTypeDef *phPcd)
{
    (void)USBD_LL_SOF((USBD_HandleTypeDef *)phPcd->pData);
}

void HAL_PCD_ResetCallback(PCD_HandleTypeDef *phPcd)
{
    (void)USBD_LL_Reset((USBD_HandleTypeDef *)phPcd->pData);
    (void)USBD_LL_SetSpeed((USBD_HandleTypeDef *)phPcd->pData, USBD_SPEED_FULL);
}

void HAL_PCD_SuspendCallback(PCD_HandleTypeDef *phPcd)
{
    (void)USBD_LL_Suspend((USBD_HandleTypeDef *)phPcd->pData);
}

void HAL_PCD_ResumeCallback(PCD_HandleTypeDef *phPcd)
{
    (void)USBD_LL_Resume((USBD_HandleTypeDef *)phPcd->pData);
}

void HAL_PCD_ISOOUTIncompleteCallback(PCD_HandleTypeDef *phPcd, uint8_t u8EpNum)
{
    (void)USBD_LL_IsoOUTIncomplete((USBD_HandleTypeDef *)phPcd->pData, u8EpNum);
}

void HAL_PCD_ISOINIncompleteCallback(PCD_HandleTypeDef *phPcd, uint8_t u8EpNum)
{
    (void)USBD_LL_IsoINIncomplete((USBD_HandleTypeDef *)phPcd->pData, u8EpNum);
}

void HAL_PCD_ConnectCallback(PCD_HandleTypeDef *phPcd)
{
    (void)USBD_LL_DevConnected((USBD_HandleTypeDef *)phPcd->pData);
}

void HAL_PCD_DisconnectCallback(PCD_HandleTypeDef *phPcd)
{
    (void)USBD_LL_DevDisconnected((USBD_HandleTypeDef *)phPcd->pData);
}

/* ---- ライブラリ → PCD ---- */

USBD_StatusTypeDef USBD_LL_Init(USBD_HandleTypeDef *phDev)
{
    /* PCD は MX_USB_OTG_FS_PCD_Init で初期化済み。結ぶのと FIFO の割り当てだけ（単位は 32 ビット語） */
    hpcd_USB_OTG_FS.pData = phDev;
    phDev->pData = &hpcd_USB_OTG_FS;
    (void)HAL_PCDEx_SetRxFiFo(&hpcd_USB_OTG_FS, 0x80U);
    (void)HAL_PCDEx_SetTxFiFo(&hpcd_USB_OTG_FS, 0U, 0x40U);
    (void)HAL_PCDEx_SetTxFiFo(&hpcd_USB_OTG_FS, 1U, 0x80U);
    return USBD_OK;
}

USBD_StatusTypeDef USBD_LL_DeInit(USBD_HandleTypeDef *phDev)
{
    return (HAL_PCD_DeInit((PCD_HandleTypeDef *)phDev->pData) == HAL_OK) ? USBD_OK : USBD_FAIL;
}

USBD_StatusTypeDef USBD_LL_Start(USBD_HandleTypeDef *phDev)
{
    return (HAL_PCD_Start((PCD_HandleTypeDef *)phDev->pData) == HAL_OK) ? USBD_OK : USBD_FAIL;
}

USBD_StatusTypeDef USBD_LL_Stop(USBD_HandleTypeDef *phDev)
{
    return (HAL_PCD_Stop((PCD_HandleTypeDef *)phDev->pData) == HAL_OK) ? USBD_OK : USBD_FAIL;
}

USBD_StatusTypeDef USBD_LL_OpenEP(USBD_HandleTypeDef *phDev, uint8_t u8EpAddr, uint8_t u8EpType,
                                  uint16_t u16EpMps)
{
    return (HAL_PCD_EP_Open((PCD_HandleTypeDef *)phDev->pData, u8EpAddr, u16EpMps, u8EpType) ==
            HAL_OK)
               ? USBD_OK
               : USBD_FAIL;
}

USBD_StatusTypeDef USBD_LL_CloseEP(USBD_HandleTypeDef *phDev, uint8_t u8EpAddr)
{
    return (HAL_PCD_EP_Close((PCD_HandleTypeDef *)phDev->pData, u8EpAddr) == HAL_OK) ? USBD_OK
                                                                                     : USBD_FAIL;
}

USBD_StatusTypeDef USBD_LL_FlushEP(USBD_HandleTypeDef *phDev, uint8_t u8EpAddr)
{
    return (HAL_PCD_EP_Flush((PCD_HandleTypeDef *)phDev->pData, u8EpAddr) == HAL_OK) ? USBD_OK
                                                                                     : USBD_FAIL;
}

USBD_StatusTypeDef USBD_LL_StallEP(USBD_HandleTypeDef *phDev, uint8_t u8EpAddr)
{
    return (HAL_PCD_EP_SetStall((PCD_HandleTypeDef *)phDev->pData, u8EpAddr) == HAL_OK)
               ? USBD_OK
               : USBD_FAIL;
}

USBD_StatusTypeDef USBD_LL_ClearStallEP(USBD_HandleTypeDef *phDev, uint8_t u8EpAddr)
{
    return (HAL_PCD_EP_ClrStall((PCD_HandleTypeDef *)phDev->pData, u8EpAddr) == HAL_OK)
               ? USBD_OK
               : USBD_FAIL;
}

uint8_t USBD_LL_IsStallEP(USBD_HandleTypeDef *phDev, uint8_t u8EpAddr)
{
    PCD_HandleTypeDef *phPcd = (PCD_HandleTypeDef *)phDev->pData;

    if ((u8EpAddr & 0x80U) == 0x80U)
    {
        return phPcd->IN_ep[u8EpAddr & 0x7FU].is_stall;
    }
    return phPcd->OUT_ep[u8EpAddr & 0x7FU].is_stall;
}

USBD_StatusTypeDef USBD_LL_SetUSBAddress(USBD_HandleTypeDef *phDev, uint8_t u8DevAddr)
{
    return (HAL_PCD_SetAddress((PCD_HandleTypeDef *)phDev->pData, u8DevAddr) == HAL_OK)
               ? USBD_OK
               : USBD_FAIL;
}

USBD_StatusTypeDef USBD_LL_Transmit(USBD_HandleTypeDef *phDev, uint8_t u8EpAddr, uint8_t *pu8Buf,
                                    uint32_t u32Size)
{
    return (HAL_PCD_EP_Transmit((PCD_HandleTypeDef *)phDev->pData, u8EpAddr, pu8Buf, u32Size) ==
            HAL_OK)
               ? USBD_OK
               : USBD_FAIL;
}

USBD_StatusTypeDef USBD_LL_PrepareReceive(USBD_HandleTypeDef *phDev, uint8_t u8EpAddr,
                                          uint8_t *pu8Buf, uint32_t u32Size)
{
    return (HAL_PCD_EP_Receive((PCD_HandleTypeDef *)phDev->pData, u8EpAddr, pu8Buf, u32Size) ==
            HAL_OK)
               ? USBD_OK
               : USBD_FAIL;
}

uint32_t USBD_LL_GetRxDataSize(USBD_HandleTypeDef *phDev, uint8_t u8EpAddr)
{
    return HAL_PCD_EP_GetRxCount((PCD_HandleTypeDef *)phDev->pData, u8EpAddr);
}

void USBD_LL_Delay(uint32_t u32DelayMs)
{
    HAL_Delay(u32DelayMs);
}
