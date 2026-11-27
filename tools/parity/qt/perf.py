"""How long Qt Widgets takes to show a long list: first frame, one label
changing, and a scroll step. The row is a label, a button and a check box.

    python perf.py --out ../out/qt

Each measurement ends with `QWidget.grab()`, which lays out and paints the
visible part on the calling thread.
"""
import json
import os
import sys
import time
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

from PySide6.QtWidgets import (
    QApplication, QCheckBox, QHBoxLayout, QLabel, QPushButton, QScrollArea, QVBoxLayout, QWidget,
)

HERE = Path(__file__).resolve().parent
args = sys.argv[1:]
OUT = Path(args[args.index("--out") + 1]) if "--out" in args else HERE.parent / "out" / "qt"
ROWS = [int(n) for n in os.environ.get("PARITY_ROWS", "100,1000,5000").split(",")]
ROUNDS = 20
OUT.mkdir(parents=True, exist_ok=True)

app = QApplication(sys.argv[:1])
app.setStyle("Fusion")


def ms(since):
    return (time.perf_counter() - since) * 1000


result = {"framework": "qt", "variants": {"QWidget rows": {}}}
for n in ROWS:
    window = QWidget()
    window.setFixedSize(420, 460)
    scroll = QScrollArea(window)
    scroll.setFixedSize(420, 400)
    scroll.setWidgetResizable(True)

    start = time.perf_counter()
    inner = QWidget()
    lay = QVBoxLayout(inner)
    lay.setContentsMargins(0, 0, 0, 0)
    lay.setSpacing(0)
    first_label = None
    for i in range(n):
        row = QWidget()
        row.setFixedHeight(36)
        h = QHBoxLayout(row)
        h.setContentsMargins(0, 0, 0, 0)
        label = QLabel(f"Item {i}: some text")
        h.addWidget(label)
        h.addWidget(QPushButton("Open"))
        h.addWidget(QCheckBox("Done"))
        h.addStretch(1)
        lay.addWidget(row)
        first_label = first_label or label
    scroll.setWidget(inner)
    window.show()
    app.processEvents()
    window.grab()
    first = ms(start)

    start = time.perf_counter()
    for k in range(ROUNDS):
        first_label.setText(f"Item 0: changed {k}")
        app.processEvents()
        window.grab()
    update = ms(start) / ROUNDS

    bar = scroll.verticalScrollBar()
    start = time.perf_counter()
    for k in range(1, ROUNDS + 1):
        bar.setValue(k * 120)
        app.processEvents()
        window.grab()
    scroll_ms = ms(start) / ROUNDS

    result["variants"]["QWidget rows"][n] = {"first": first, "update": update, "scroll": scroll_ms}
    print(f"qt: {n} rows  first {first:.1f} ms  update {update:.2f} ms  scroll {scroll_ms:.2f} ms")
    window.close()

(OUT / "perf.json").write_text(json.dumps(result, indent=2))
