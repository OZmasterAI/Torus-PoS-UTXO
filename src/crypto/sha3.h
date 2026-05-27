// SHA3-256 wrapper using OpenSSL EVP (FIPS 202)
// Used for Tor v3 onion address checksum verification.
#ifndef CRYPTO_SHA3_H
#define CRYPTO_SHA3_H

#include <cstring>
#include <openssl/evp.h>

class SHA3_256 {
private:
    EVP_MD_CTX *ctx;

public:
    SHA3_256() {
        ctx = EVP_MD_CTX_new();
        EVP_DigestInit_ex(ctx, EVP_sha3_256(), NULL);
    }

    ~SHA3_256() {
        EVP_MD_CTX_free(ctx);
    }

    SHA3_256& Write(const unsigned char* data, size_t len) {
        if (data && len > 0)
            EVP_DigestUpdate(ctx, data, len);
        return *this;
    }

    void Finalize(unsigned char hash[32]) {
        unsigned int len = 32;
        EVP_DigestFinal_ex(ctx, hash, &len);
    }

    static void Hash(const unsigned char* data, size_t len, unsigned char hash[32]) {
        SHA3_256 hasher;
        hasher.Write(data, len);
        hasher.Finalize(hash);
    }
};

#endif // CRYPTO_SHA3_H
