/**
 * @file usbd_conf.h
 * @brief ST の USB Device ライブラリの設定（F0 のダミー機器、USB MIDI だけ）。
 *
 * 名前（USBD_*）はライブラリが決めたもので、変えない。
 */
#ifndef USBD_CONF_H
#define USBD_CONF_H

#include <string.h>

#include "stm32f7xx_hal.h"

#define USBD_MAX_NUM_INTERFACES       2U   /* Audio Control と MIDI Streaming */
#define USBD_MAX_NUM_CONFIGURATION    1U
#define USBD_MAX_STR_DESC_SIZ         0x100U
#define USBD_SELF_POWERED             0U   /* PC の USB から電気を取る扱い（構成の記述子と合わせる） */
#define USBD_MAX_POWER                0x32U /* 100 mA */
#define USBD_DEBUG_LEVEL              0U
#define USBD_LPM_ENABLED              0U
#define USBD_SUPPORT_USER_STRING_DESC 0U

/* ライブラリが使う道具。動的確保は使わない（クラスのデータは静的に持つ） */
#define USBD_memset memset
#define USBD_memcpy memcpy
#define USBD_Delay  HAL_Delay

#define USBD_UsrLog(...)
#define USBD_ErrLog(...)
#define USBD_DbgLog(...)

#endif /* USBD_CONF_H */
