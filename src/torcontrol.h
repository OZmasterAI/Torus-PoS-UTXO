// Copyright (c) 2024 The TORUS developers
// Tor v3 control port integration
#ifndef BITCOIN_TORCONTROL_H
#define BITCOIN_TORCONTROL_H

void StartTorControl();
void StopTorControl();
void ThreadTorControl(void* parg);

#endif // BITCOIN_TORCONTROL_H
