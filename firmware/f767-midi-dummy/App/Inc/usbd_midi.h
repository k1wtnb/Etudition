/**
 * @file usbd_midi.h
 * @brief USB MIDI 1.0 のクラス（機器から PC への向きだけを使う。PC からの MIDI は受けて捨てる）。
 *
 * ST の USB Device ライブラリには MIDI のクラスがないので、自分で書いた（計画の F0）。
 * 記述子の形は USB Device Class Definition for MIDI Devices 1.0 の付録 B の例に倣った。
 */
#ifndef USBD_MIDI_H
#define USBD_MIDI_H

#include <stdbool.h>

#include "usbd_def.h"

/** ライブラリに登録するクラス。 */
extern USBD_ClassTypeDef g_stUsbdMidiClass;

/**
 * @brief USB MIDI のイベント（4 バイト）を1つ送る列に入れる。main から呼ぶ（割込みからは呼ばない）。
 * @return 入れられたら true。PC につながって構成済みでない、または列が満杯なら false
 */
bool UsbdMidiQueue(USBD_HandleTypeDef *phDev, const uint8_t au8Event[4]);

#endif /* USBD_MIDI_H */
