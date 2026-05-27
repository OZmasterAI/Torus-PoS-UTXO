// Copyright (c) 2024 The TORUS developers
// Tor v3 control port integration — auto-creates a hidden service on startup
// Uses synchronous I/O in a dedicated thread (matches existing codebase pattern).

#include "torcontrol.h"
#include "util.h"
#include "net.h"
#include "netbase.h"

#include <string>
#include <vector>
#include <fstream>

#ifndef WIN32
#include <unistd.h>
#include <fcntl.h>
#endif

static const int TOR_CONTROL_PORT = 9051;
static const int TOR_MAX_LINE = 8192;

static bool fTorControlRunning = false;

static bool TorRecvLine(SOCKET hSocket, std::string& line, int timeout_ms = 10000)
{
    line.clear();
    int64_t nStart = GetTimeMillis();
    while (GetTimeMillis() - nStart < timeout_ms)
    {
        char ch;
        int n = recv(hSocket, &ch, 1, 0);
        if (n == 1) {
            if (ch == '\n') {
                if (!line.empty() && line.back() == '\r')
                    line.pop_back();
                return true;
            }
            line += ch;
            if ((int)line.size() > TOR_MAX_LINE)
                return false;
        } else if (n == 0) {
            return false;
        } else {
#ifdef WIN32
            if (WSAGetLastError() == WSAEWOULDBLOCK) {
#else
            if (errno == EAGAIN || errno == EWOULDBLOCK) {
#endif
                MilliSleep(50);
                continue;
            }
            return false;
        }
    }
    return false;
}

static bool TorSendCommand(SOCKET hSocket, const std::string& cmd, std::string& reply)
{
    std::string full = cmd + "\r\n";
    if (send(hSocket, full.c_str(), full.size(), MSG_NOSIGNAL) != (int)full.size())
        return false;

    reply.clear();
    while (true)
    {
        std::string line;
        if (!TorRecvLine(hSocket, line))
            return false;

        if (line.size() < 4)
            return false;

        reply += line + "\n";

        // Status lines: "250-..." means more data, "250 " means last line
        if (line[3] == ' ')
            return true;
        if (line[3] != '-')
            return true;
    }
}

static int ParseReplyCode(const std::string& reply)
{
    if (reply.size() < 3)
        return -1;
    return atoi(reply.substr(0, 3).c_str());
}

static std::string GetPrivateKeyPath()
{
    return GetDataDir().string() + "/onion_v3_private_key";
}

void ThreadTorControl(void* parg)
{
    RenameThread("tor-control");
    printf("ThreadTorControl started\n");

    vnThreadsRunning[THREAD_TORCONTROL]++;

    std::string torcontrol = GetArg("-torcontrol", "127.0.0.1:9051");
    std::string torpassword = GetArg("-torpassword", "");
    unsigned short listenPort = GetListenPort();

    int backoff = 1;
    static const int MAX_BACKOFF = 60;

    while (!fShutdown)
    {
        // Parse and connect to Tor control port
        CService addrControl;
        if (!LookupNumeric(torcontrol.c_str(), addrControl, TOR_CONTROL_PORT)) {
            printf("torcontrol: invalid address %s\n", torcontrol.c_str());
            break;
        }

        SOCKET hSocket = INVALID_SOCKET;
        if (!ConnectSocket(addrControl, hSocket, 5000))
        {
            if (fShutdown) break;
            printf("torcontrol: failed to connect to %s, retrying in %ds\n", addrControl.ToString().c_str(), backoff);
            for (int i = 0; i < backoff && !fShutdown; i++)
                MilliSleep(1000);
            backoff = std::min(backoff * 2, MAX_BACKOFF);
            continue;
        }

        // Set non-blocking for recv timeouts
#ifdef WIN32
        u_long fNonblock = 1;
        ioctlsocket(hSocket, FIONBIO, &fNonblock);
#else
        int flags = fcntl(hSocket, F_GETFL, 0);
        fcntl(hSocket, F_SETFL, flags | O_NONBLOCK);
#endif

        printf("torcontrol: connected to %s\n", addrControl.ToString().c_str());
        backoff = 1;

        std::string reply;

        // Authenticate — try cookie first, then password, then NULL
        bool fAuth = false;

        // Try cookie auth: read the cookie file and send as hex
        if (!fAuth) {
            std::vector<std::string> cookiePaths;
            cookiePaths.push_back("/run/tor/control.authcookie");
            cookiePaths.push_back("/var/run/tor/control.authcookie");
            cookiePaths.push_back("/var/lib/tor/control_auth_cookie");
            for (size_t i = 0; i < cookiePaths.size() && !fAuth; i++) {
                std::ifstream cookieFile(cookiePaths[i].c_str(), std::ios::binary);
                if (cookieFile.good()) {
                    std::vector<unsigned char> cookie(32);
                    cookieFile.read((char*)&cookie[0], 32);
                    if (cookieFile.gcount() == 32) {
                        std::string hex;
                        for (int j = 0; j < 32; j++) {
                            char tmp[3];
                            snprintf(tmp, sizeof(tmp), "%02X", cookie[j]);
                            hex += tmp;
                        }
                        std::string cmd = "AUTHENTICATE " + hex;
                        if (TorSendCommand(hSocket, cmd, reply) && ParseReplyCode(reply) == 250)
                            fAuth = true;
                    }
                }
            }
        }

        if (!fAuth && !torpassword.empty()) {
            std::string cmd = "AUTHENTICATE \"" + torpassword + "\"";
            if (TorSendCommand(hSocket, cmd, reply) && ParseReplyCode(reply) == 250)
                fAuth = true;
        }
        if (!fAuth) {
            if (TorSendCommand(hSocket, "AUTHENTICATE", reply) && ParseReplyCode(reply) == 250)
                fAuth = true;
        }
        if (!fAuth) {
            printf("torcontrol: authentication failed\n");
            closesocket(hSocket);
            for (int i = 0; i < 30 && !fShutdown; i++)
                MilliSleep(1000);
            continue;
        }

        printf("torcontrol: authenticated\n");

        // Build ADD_ONION command
        std::string addOnionCmd;
        std::string privKeyPath = GetPrivateKeyPath();
        std::string cachedKey;

        {
            std::ifstream keyFile(privKeyPath.c_str());
            if (keyFile.good())
                std::getline(keyFile, cachedKey);
        }

        if (!cachedKey.empty()) {
            addOnionCmd = "ADD_ONION " + cachedKey + " Port=" +
                          strprintf("%u,127.0.0.1:%u", listenPort, listenPort);
        } else {
            addOnionCmd = "ADD_ONION NEW:ED25519-V3 Port=" +
                          strprintf("%u,127.0.0.1:%u", listenPort, listenPort);
        }

        if (!TorSendCommand(hSocket, addOnionCmd, reply) || ParseReplyCode(reply) != 250)
        {
            printf("torcontrol: ADD_ONION failed: %s\n", reply.c_str());
            closesocket(hSocket);
            for (int i = 0; i < 30 && !fShutdown; i++)
                MilliSleep(1000);
            continue;
        }

        // Parse response for ServiceID and PrivateKey
        std::string serviceID;
        std::string privateKey;
        std::istringstream iss(reply);
        std::string line;
        while (std::getline(iss, line))
        {
            if (line.find("250-ServiceID=") == 0)
                serviceID = line.substr(14);
            else if (line.find("250-PrivateKey=") == 0)
                privateKey = line.substr(15);
        }

        if (serviceID.empty()) {
            printf("torcontrol: no ServiceID in ADD_ONION response\n");
            closesocket(hSocket);
            continue;
        }

        // Trim trailing whitespace
        while (!serviceID.empty() && (serviceID.back() == '\r' || serviceID.back() == '\n' || serviceID.back() == ' '))
            serviceID.pop_back();
        while (!privateKey.empty() && (privateKey.back() == '\r' || privateKey.back() == '\n' || privateKey.back() == ' '))
            privateKey.pop_back();

        // Cache private key for persistent address
        if (!privateKey.empty()) {
            std::ofstream keyFile(privKeyPath.c_str());
            if (keyFile.good()) {
                keyFile << privateKey;
                printf("torcontrol: cached private key to %s\n", privKeyPath.c_str());
            }
        }

        // Register onion address
        std::string onionAddr = serviceID + ".onion";
        CService onionService(onionAddr, listenPort);
        if (onionService.IsValid()) {
            AddLocal(onionService, LOCAL_MANUAL);
            SetReachable(NET_TOR);
            SetReachable(NET_TORV3);
            printf("torcontrol: published %s:%u\n", onionAddr.c_str(), listenPort);
        } else {
            printf("torcontrol: failed to parse onion address %s\n", onionAddr.c_str());
        }

        fTorControlRunning = true;

        // Keep alive — periodically check if Tor is still responsive
        while (!fShutdown)
        {
            MilliSleep(300000); // 5 minutes
            if (fShutdown) break;

            if (!TorSendCommand(hSocket, "GETINFO version", reply) || ParseReplyCode(reply) != 250)
            {
                printf("torcontrol: connection lost, reconnecting\n");
                break;
            }
        }

        // Cleanup
        TorSendCommand(hSocket, "DEL_ONION " + serviceID, reply);
        closesocket(hSocket);
        fTorControlRunning = false;
    }

    vnThreadsRunning[THREAD_TORCONTROL]--;
    printf("ThreadTorControl exited\n");
}

void StartTorControl()
{
    if (!GetBoolArg("-listenonion", true))
    {
        printf("torcontrol: disabled by -listenonion=0\n");
        return;
    }

    if (!NewThread(ThreadTorControl, NULL))
        printf("Error: NewThread(ThreadTorControl) failed\n");
}

void StopTorControl()
{
    // Thread exits via fShutdown flag
}
