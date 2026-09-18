#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Build docs/QUICKSTART.pdf from QUICKSTART.txt for offline release bundles."""

from pathlib import Path

ROOT = Path(__file__).resolve().parent
SRC = ROOT / "QUICKSTART.txt"
OUT = ROOT / "QUICKSTART.pdf"


def main() -> None:
    text = SRC.read_text(encoding="utf-8")
    try:
        from fpdf import FPDF
    except ImportError as e:
        raise SystemExit(
            "fpdf2 is required: pip install fpdf2"
        ) from e

    pdf = FPDF()
    pdf.set_auto_page_break(auto=True, margin=15)
    pdf.set_margins(15, 15, 15)
    pdf.add_page()
    pdf.set_font("Courier", size=9)
    w = pdf.w - pdf.l_margin - pdf.r_margin
    for line in text.splitlines():
        safe = line.encode("latin-1", "replace").decode("latin-1")
        pdf.multi_cell(w, 4.5, safe)
    pdf.output(str(OUT))
    print(f"wrote {OUT}")


if __name__ == "__main__":
    main()
