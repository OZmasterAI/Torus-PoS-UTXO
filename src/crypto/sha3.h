// SHA3-256 implementation (FIPS 202)
// Minimal header-only implementation for Tor v3 onion address checksum verification.
// Based on the public domain Keccak reference implementation.
#ifndef CRYPTO_SHA3_H
#define CRYPTO_SHA3_H

#include <cstdint>
#include <cstring>
#include <vector>

class SHA3_256 {
private:
    uint64_t state[25];
    unsigned char buf[136]; // rate for SHA3-256 = 1088 bits = 136 bytes
    size_t buf_len;
    bool finalized;

    static const uint64_t RC[24];

    static uint64_t rotl64(uint64_t x, unsigned n) {
        return (x << n) | (x >> (64 - n));
    }

    void keccak_f() {
        for (int round = 0; round < 24; round++) {
            // theta
            uint64_t C[5], D[5];
            for (int x = 0; x < 5; x++)
                C[x] = state[x] ^ state[x+5] ^ state[x+10] ^ state[x+15] ^ state[x+20];
            for (int x = 0; x < 5; x++) {
                D[x] = C[(x+4)%5] ^ rotl64(C[(x+1)%5], 1);
                for (int y = 0; y < 25; y += 5)
                    state[y+x] ^= D[x];
            }
            // rho and pi
            uint64_t tmp = state[1];
            static const int piln[24] = {
                10,  7, 11, 17, 18,  3,  5, 16,  8, 21, 24,  4,
                15, 23, 19, 13, 12,  2, 20, 14, 22,  9,  6,  1
            };
            static const int rotc[24] = {
                 1,  3,  6, 10, 15, 21, 28, 36, 45, 55,  2, 14,
                27, 41, 56,  8, 25, 43, 62, 18, 39, 44, 20, 61
            };
            for (int i = 0; i < 24; i++) {
                int j = piln[i];
                uint64_t t = state[j];
                state[j] = rotl64(tmp, rotc[i]);
                tmp = t;
            }
            // chi
            for (int y = 0; y < 25; y += 5) {
                uint64_t t[5];
                for (int x = 0; x < 5; x++)
                    t[x] = state[y+x];
                for (int x = 0; x < 5; x++)
                    state[y+x] = t[x] ^ (~t[(x+1)%5] & t[(x+2)%5]);
            }
            // iota
            state[0] ^= RC[round];
        }
    }

    void absorb_block() {
        for (size_t i = 0; i < 136/8; i++) {
            uint64_t lane;
            memcpy(&lane, buf + i*8, 8);
            state[i] ^= lane;
        }
        keccak_f();
    }

public:
    SHA3_256() : buf_len(0), finalized(false) {
        memset(state, 0, sizeof(state));
        memset(buf, 0, sizeof(buf));
    }

    SHA3_256& Write(const unsigned char* data, size_t len) {
        while (len > 0) {
            size_t space = 136 - buf_len;
            size_t copy = len < space ? len : space;
            memcpy(buf + buf_len, data, copy);
            buf_len += copy;
            data += copy;
            len -= copy;
            if (buf_len == 136) {
                absorb_block();
                buf_len = 0;
            }
        }
        return *this;
    }

    void Finalize(unsigned char hash[32]) {
        // SHA3 padding: 0x06...0x80
        memset(buf + buf_len, 0, 136 - buf_len);
        buf[buf_len] = 0x06;
        buf[135] |= 0x80;
        absorb_block();

        // squeeze 32 bytes (256 bits)
        for (int i = 0; i < 4; i++)
            memcpy(hash + i*8, &state[i], 8);

        finalized = true;
    }

    static void Hash(const unsigned char* data, size_t len, unsigned char hash[32]) {
        SHA3_256 ctx;
        ctx.Write(data, len);
        ctx.Finalize(hash);
    }
};

const uint64_t SHA3_256::RC[24] = {
    0x0000000000000001ULL, 0x0000000000008082ULL, 0x800000000000808aULL,
    0x8000000080008000ULL, 0x000000000000808bULL, 0x0000000080000001ULL,
    0x8000000080008081ULL, 0x8000000000008009ULL, 0x000000000000008aULL,
    0x0000000000000088ULL, 0x0000000080008009ULL, 0x000000008000000aULL,
    0x000000008000808bULL, 0x800000000000008bULL, 0x8000000000008089ULL,
    0x8000000000008003ULL, 0x8000000000008002ULL, 0x8000000000000080ULL,
    0x000000000000800aULL, 0x800000008000000aULL, 0x8000000080008081ULL,
    0x8000000000008080ULL, 0x0000000080000001ULL, 0x8000000080008008ULL,
};

#endif // CRYPTO_SHA3_H
