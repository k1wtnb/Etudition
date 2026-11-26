/**
 * @file dummy_midi_none.c
 * @brief USB MIDI がまだない段階の送り先（何も送らず、受けたことにする）。
 *
 * シリアルの指示とボタンの経路を、USB MIDI の前に実機で確かめるための仮の実装。
 * USB MIDI の実装を足したら、CMakeLists.txt でこのファイルと差し替える。
 */
#include "dummy_midi.h"

void DummyMidiInit(void)
{
}

bool DummyMidiSend(const DummyCmd *pstCmd)
{
    (void)pstCmd;
    return true;
}
