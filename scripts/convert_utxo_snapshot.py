#!/usr/bin/env python3
"""Convert dumputxoset JSON output to genesis_utxos.h C++ header."""

import json
import sys
import os


def main():
    if len(sys.argv) < 2:
        print(f"Usage: {sys.argv[0]} <utxo_snapshot_NNN.json> [output.h]")
        sys.exit(1)

    input_path = sys.argv[1]
    output_path = (
        sys.argv[2]
        if len(sys.argv) > 2
        else os.path.join(
            os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
            "src",
            "genesis_utxos.h",
        )
    )

    with open(input_path, "r") as f:
        data = json.load(f)

    utxos = data["utxos"]
    height = data["height"]
    total_amount = sum(u["amount"] for u in utxos)

    print(f"Snapshot height: {height}")
    print(f"UTXO count: {len(utxos)}")
    print(f"Total amount: {total_amount} satoshis ({total_amount / 1e8:.8f} coins)")

    lines = []
    lines.append("#ifndef GENESIS_UTXOS_H")
    lines.append("#define GENESIS_UTXOS_H")
    lines.append("")
    lines.append("#include <vector>")
    lines.append("")
    lines.append("struct GenesisUTXO {")
    lines.append("    const char* scriptPubKeyHex;")
    lines.append("    int64_t amount;")
    lines.append("};")
    lines.append("")
    lines.append(f"// Generated from UTXO snapshot at height {height}")
    lines.append(f"// {len(utxos)} UTXOs, total {total_amount / 1e8:.8f} coins")
    lines.append("static const std::vector<GenesisUTXO> GENESIS_UTXOS = {")

    for i, utxo in enumerate(utxos):
        script = utxo["scriptPubKey"]
        amount = utxo["amount"]
        comma = "," if i < len(utxos) - 1 else ""
        lines.append(f'    {{"{script}", {amount}LL}}{comma}')

    lines.append("};")
    lines.append("")
    lines.append("#endif // GENESIS_UTXOS_H")
    lines.append("")

    with open(output_path, "w") as f:
        f.write("\n".join(lines))

    print(f"Wrote {output_path}")


if __name__ == "__main__":
    main()
