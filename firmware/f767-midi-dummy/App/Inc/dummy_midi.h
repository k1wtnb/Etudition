/**
 * @file dummy_midi.h
 * @brief MIDI の送り先（USB MIDI）。実装は USB の方式が決まってから足す。
 */
#ifndef DUMMY_MIDI_H
#define DUMMY_MIDI_H

#include <stdbool.h>

#include "dummy_cmd.h"

/** 送り先を準備する（起動時に1回）。 */
void DummyMidiInit(void);

/**
 * @brief Note On か Note Off を1つ送る（チャンネル 1）。待たない。
 * @return 送る列に入れられたら true。PC につながっていない、または列が満杯なら false
 */
bool DummyMidiSend(const DummyCmd *pstCmd);

#endif /* DUMMY_MIDI_H */
