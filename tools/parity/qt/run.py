"""The Qt side of the parity bench: the same scenarios as QWidgets, driven with
QTest's mouse and key simulation on the `offscreen` platform, with
accessibility read from Qt's own QAccessible interface.

    python run.py --out ../out/qt [scenario ids ...]
"""
import json
import os
import sys
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

from PySide6.QtCore import QPoint, QPointF, Qt
from PySide6.QtGui import QAccessible, QColor, QPalette, QWheelEvent
from PySide6.QtTest import QTest
from PySide6.QtWidgets import (
    QApplication, QButtonGroup, QCheckBox, QFrame, QHBoxLayout, QLabel, QLineEdit,
    QPushButton, QRadioButton, QScrollArea, QSizePolicy, QSlider, QVBoxLayout, QWidget,
)

# What Qt Widgets cannot say about one widget. Percent sizes and a per-widget
# margin have no equivalent; a scenario that needs them is marked unsupported
# and the property is ignored.
UNSUPPORTED = {"percent", "margin", "wrap", "align-content", "border-sides", "offset-end", "flex-detail", "layout-edge", "grid"}

HERE = Path(__file__).resolve().parent
args = sys.argv[1:]
out_at = args.index("--out") if "--out" in args else -1
OUT = Path(args[out_at + 1]) if out_at >= 0 else HERE.parent / "out" / "qt"
ONLY = [a for i, a in enumerate(args) if not a.startswith("--") and i != out_at + 1]
OUT.mkdir(parents=True, exist_ok=True)

app = QApplication(sys.argv[:1])
app.setStyle("Fusion")
dark = QPalette()
for role, colour in [
    (QPalette.Window, "#1b2230"), (QPalette.WindowText, "#e6ebf5"), (QPalette.Base, "#141a26"),
    (QPalette.Text, "#e6ebf5"), (QPalette.Button, "#3a4152"), (QPalette.ButtonText, "#e6ebf5"),
    (QPalette.Highlight, "#2f7d6d"), (QPalette.HighlightedText, "#ffffff"),
]:
    dark.setColor(role, QColor(colour))
app.setPalette(dark)
QAccessible.setActive(True)

KEYS = {
    "Tab": Qt.Key_Tab, "Enter": Qt.Key_Return, "Space": Qt.Key_Space, "Escape": Qt.Key_Escape,
    "ArrowUp": Qt.Key_Up, "ArrowDown": Qt.Key_Down, "ArrowLeft": Qt.Key_Left,
    "ArrowRight": Qt.Key_Right, "Home": Qt.Key_Home, "End": Qt.Key_End,
    "Backspace": Qt.Key_Backspace, "PageUp": Qt.Key_PageUp, "PageDown": Qt.Key_PageDown,
}
MODS = {"Shift": Qt.ShiftModifier, "Control": Qt.ControlModifier, "Alt": Qt.AltModifier}
CURSORS = {
    Qt.ArrowCursor: "default", Qt.IBeamCursor: "text", Qt.PointingHandCursor: "pointer",
    Qt.CrossCursor: "crosshair", Qt.SizeAllCursor: "move", Qt.ForbiddenCursor: "not-allowed",
}
ROLES = {
    QAccessible.Role.PushButton: "button", QAccessible.Role.CheckBox: "checkbox",
    QAccessible.Role.RadioButton: "radio", QAccessible.Role.EditableText: "textbox",
    QAccessible.Role.Slider: "slider", QAccessible.Role.StaticText: "text",
    QAccessible.Role.Grouping: "group", QAccessible.Role.Client: "generic",
    QAccessible.Role.Pane: "generic", QAccessible.Role.ScrollBar: "scrollbar",
}


class Built:
    """One scenario's widgets, by id, and what happened to them."""

    def __init__(self, spec):
        self.spec = spec
        self.widgets = {}
        self.log = []
        self.groups = {}
        self.hidden = set()
        self.root = QWidget()
        self.root.setFixedSize(*spec["size"])
        self.root.setAutoFillBackground(True)
        layout = QVBoxLayout(self.root)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(0)
        top = self.build(spec["ui"])
        layout.addWidget(top, 0, Qt.AlignLeft | Qt.AlignTop)
        top.setFixedSize(*spec["size"])

    def fixed(self, w, node):
        grows = node.get("grow", 0) > 0
        if node.get("width") is not None and not grows and node.get("minWidth") is None and node.get("maxWidth") is None:
            w.setFixedWidth(node["width"])
        elif node.get("width") is not None:
            w.setMinimumWidth(node["width"])
        if node.get("minWidth") is not None:
            w.setMinimumWidth(node["minWidth"])
        if node.get("maxWidth") is not None:
            w.setMaximumWidth(node["maxWidth"])
        if node.get("height") is not None:
            w.setFixedHeight(node["height"])

    ALIGN_COLUMN = {"start": Qt.AlignLeft, "center": Qt.AlignHCenter, "end": Qt.AlignRight, "stretch": Qt.Alignment()}
    ALIGN_ROW = {"start": Qt.AlignTop, "center": Qt.AlignVCenter, "end": Qt.AlignBottom, "stretch": Qt.Alignment()}

    def fill(self, lay, node, kind):
        """Add the children of a box layout: cross-axis alignment on each, and
        stretches for what the main axis does with spare room."""
        align = node.get("align", "start")
        cross = (self.ALIGN_COLUMN if kind == "column" else self.ALIGN_ROW)[align]
        main = Qt.AlignTop if kind == "column" else Qt.AlignLeft
        kids = [c for c in node.get("children", [])]
        justify = node.get("justify", "start")
        growing = any(c.get("grow", 0) for c in kids)
        stretch = lambda weight: lay.addStretch(weight)
        if not growing:
            if justify in ("end", "center", "space-around", "space-evenly"):
                stretch(1)
            for index, child in enumerate(kids):
                if index and justify in ("space-between", "space-around", "space-evenly"):
                    stretch(2 if justify == "space-around" else 1)
                self.add(lay, child, cross, kind)
            if justify in ("start", "center", "space-around", "space-evenly"):
                stretch(1)
        else:
            for child in kids:
                self.add(lay, child, cross, kind)

    def add(self, lay, child, cross, kind):
        w = self.build(child)
        grow = child.get("grow", 0)
        if grow:
            policy = w.sizePolicy()
            if kind == "row":
                policy.setHorizontalPolicy(QSizePolicy.Expanding)
            else:
                policy.setVerticalPolicy(QSizePolicy.Expanding)
            w.setSizePolicy(policy)
        lay.addWidget(w, grow, cross)

    def build(self, node):
        kind, ident = node["type"], node.get("id", "")
        if kind in ("column", "row"):
            w = QWidget()
            lay = (QVBoxLayout if kind == "column" else QHBoxLayout)(w)
            # An omitted gap or padding is Qt's own default, which is the point.
            if node.get("gap") is not None:
                lay.setSpacing(node["gap"])
            if node.get("padding") is not None:
                p = node["padding"]
                if isinstance(p, list):
                    lay.setContentsMargins(p[3], p[0], p[1], p[2])
                else:
                    lay.setContentsMargins(p, p, p, p)
            self.fill(lay, node, kind)
        elif kind == "stack":
            w = QWidget()
            order = sorted(enumerate(node.get("children", [])), key=lambda it: (it[1].get("z", 0), it[0]))
            for _, child in order:
                c = self.build(child)
                c.setParent(w)
                at = child.get("position", {"x": 0, "y": 0})
                if not (child.get("width") or child.get("height")):
                    c.adjustSize()
                c.move(at["x"], at["y"])
                c.show()
                c.raise_()
        elif kind == "scroll":
            w = QScrollArea()
            w.setFrameShape(QFrame.NoFrame)
            w.setWidgetResizable(True)
            inner = QWidget()
            lay = QVBoxLayout(inner)
            lay.setContentsMargins(0, 0, 0, 0)
            lay.setSpacing(0)
            for child in node.get("children", []):
                lay.addWidget(self.build(child), 0, Qt.AlignLeft | Qt.AlignTop)
            lay.addStretch(1)
            w.setWidget(inner)
        elif kind in ("panel", "spacer"):
            w = QWidget()
            if node.get("background"):
                w.setAutoFillBackground(True)
                pal = w.palette()
                pal.setColor(QPalette.Window, QColor(node["background"]))
                w.setPalette(pal)
        elif kind == "label":
            w = QLabel(node["text"])
        elif kind == "button":
            w = QPushButton(node["text"])
            w.clicked.connect(lambda _=False, i=ident: self.log.append(f"click:{i}"))
        elif kind in ("checkbox", "switch"):
            # Qt Widgets has no switch; a check box is what it offers.
            w = QCheckBox(node["text"])
            w.setChecked(bool(node.get("checked")))
            w.toggled.connect(lambda on, i=ident: self.log.append(f"toggle:{i}:{str(on).lower()}"))
        elif kind == "radiogroup":
            w = QWidget()
            lay = QVBoxLayout(w)
            lay.setContentsMargins(0, 0, 0, 0)
            group = QButtonGroup(w)
            self.groups[ident] = group
            for index, option in enumerate(node["options"]):
                radio = QRadioButton(option["text"])
                radio.setObjectName(option["id"])
                group.addButton(radio, index)
                radio.setChecked(index == node.get("value"))
                radio.toggled.connect(
                    lambda on, i=ident, n=index: on and self.log.append(f"select:{i}:{n}")
                )
                self.widgets[option["id"]] = radio
                lay.addWidget(radio, 0, Qt.AlignLeft)
        elif kind == "input":
            w = QLineEdit(node.get("value", ""))
            w.textChanged.connect(lambda t, i=ident: self.log.append(f"input:{i}:{t}"))
        elif kind == "slider":
            w = QSlider(Qt.Horizontal)
            w.setRange(node["min"], node["max"])
            w.setSingleStep(node["step"])
            w.setValue(node["value"])
            w.valueChanged.connect(lambda v, i=ident: self.log.append(f"slide:{i}:{v}"))
        else:
            raise ValueError(f"unknown widget type {kind}")
        self.fixed(w, node)
        if node.get("label"):
            w.setAccessibleName(node["label"])
        if node.get("border") is not None:
            # A style sheet's border sits inside the widget, as border-box does.
            w.setAttribute(Qt.WA_StyledBackground, True)
            extra = f"background:{node['background']};" if node.get("background") else ""
            w.setStyleSheet(f"#{ident or 'x'} {{ {extra} border:{node['border']}px solid #8aa4c8; }}")
        if node.get("disabled"):
            w.setEnabled(False)
        if node.get("visible") is False:
            w.hide()
            self.hidden.add(ident)
        if ident:
            w.setObjectName(ident)
            self.widgets[ident] = w
        return w

    # What a person aims at.
    def rect(self, ident):
        if ident in self.hidden:
            return [0, 0, 0, 0]
        w = self.widgets[ident]
        p = w.mapTo(self.root, QPoint(0, 0))
        return [p.x(), p.y(), w.width(), w.height()]

    def centre(self, ident, fx=0.5, fy=0.5):
        x, y, w, h = self.rect(ident)
        return QPoint(int(x + w * fx), int(y + h * fy))

    def owner(self, widget):
        known = {id(w): i for i, w in self.widgets.items()}
        while widget is not None:
            if id(widget) in known:
                return known[id(widget)]
            widget = widget.parentWidget()
        return None

    def focus(self):
        return self.owner(QApplication.focusWidget())

    def values(self):
        out = {}
        for ident, w in self.widgets.items():
            if isinstance(w, QCheckBox):
                out[ident] = w.isChecked()
            elif isinstance(w, QLineEdit):
                out[ident] = w.text()
            elif isinstance(w, QSlider):
                out[ident] = w.value()
        for ident, group in self.groups.items():
            out[ident] = group.checkedId()
        return out

    def a11y(self, ident):
        if ident in self.hidden:
            return {"role": "none", "name": ""}
        iface = QAccessible.queryAccessibleInterface(self.widgets[ident])
        if iface is None:
            return {"role": "n/a", "name": ""}
        state = iface.state()
        out = {"role": ROLES.get(iface.role(), str(iface.role()).split(".")[-1]),
               "name": iface.text(QAccessible.Text.Name)}
        if state.disabled:
            out["disabled"] = True
        if state.checkable:
            out["checked"] = bool(state.checked)
        if isinstance(self.widgets[ident], (QLineEdit, QSlider)):
            value = iface.text(QAccessible.Text.Value)
            if not value and isinstance(self.widgets[ident], QSlider):
                value = str(self.widgets[ident].value())
            out["value"] = value
        return out

    def scrolls(self):
        return {i: w.verticalScrollBar().value() for i, w in self.widgets.items() if isinstance(w, QScrollArea)}


def at(root, pt):
    """The widget under a point of the window and the point in its own
    coordinates: QTest does not hit-test, a real mouse does."""
    widget = root.childAt(pt) or root
    return widget, widget.mapFrom(root, pt)


def mouse(root, action, pt, grab=None):
    widget, local = at(root, pt)
    if grab is not None:  # a pressed widget keeps the mouse until release
        widget, local = grab, grab.mapFrom(root, pt)
    if action == "move":
        QTest.mouseMove(widget, local)
    elif action == "press":
        QTest.mousePress(widget, Qt.LeftButton, Qt.NoModifier, local)
    else:
        QTest.mouseRelease(widget, Qt.LeftButton, Qt.NoModifier, local)
    return widget


def send_key(target, spec):
    parts = spec.split("+")
    mods = Qt.NoModifier
    for m in parts[:-1]:
        mods |= MODS[m]
    key = parts[-1]
    if key in KEYS:
        QTest.keyClick(target, KEYS[key], mods)
    else:
        QTest.keyClick(target, key, mods)


def run(spec):
    built = Built(spec)
    root = built.root
    root.show()
    QTest.qWaitForWindowExposed(root)
    root.activateWindow()
    app.processEvents()
    records, shots = [], []

    def shot():
        name = f"{spec['id']}--{len(records)}.png"
        root.grab().save(str(OUT / name))
        shots.append(name)

    def target_widget():
        return QApplication.focusWidget() or root

    for step in spec["steps"]:
        do = step["do"]
        if do == "observe":
            ids = list(built.widgets)
            records.append({
                "type": "observe", "label": step["label"], "focus": built.focus(),
                # Qt draws the focus frame from the style; it cannot be asked.
                "focusVisible": None,
                "values": built.values(), "scroll": built.scrolls(), "log": list(built.log),
                "a11y": {i: built.a11y(i) for i in ids},
                "rects": {i: built.rect(i) for i in ids},
            })
            shot()
        elif do in ("click", "clickAt"):
            pt = built.centre(step["target"], step.get("fx", 0.5), step.get("fy", 0.5))
            mouse(root, "move", pt)
            pressed = mouse(root, "press", pt)
            mouse(root, "release", pt, grab=pressed)
        elif do == "hover":
            mouse(root, "move", built.centre(step["target"]))
        elif do == "dragOff":
            pt = built.centre(step["target"])
            away = QPoint(pt.x() + 150, pt.y())
            mouse(root, "move", pt)
            pressed = mouse(root, "press", pt)
            mouse(root, "move", away, grab=pressed)
            mouse(root, "release", away, grab=pressed)
        elif do == "wheel":
            pt = built.centre(step["target"])
            mouse(root, "move", pt)
            target = built.widgets[step["target"]]
            view = target.viewport() if isinstance(target, QScrollArea) else target
            event = QWheelEvent(
                QPointF(view.rect().center()), QPointF(view.mapToGlobal(view.rect().center())),
                QPoint(0, 0), QPoint(0, -int(step["dy"])), Qt.NoButton, Qt.NoModifier,
                Qt.NoScrollPhase, False,
            )
            QApplication.sendEvent(view, event)
        elif do == "press":
            send_key(target_widget(), step["key"])
        elif do == "type":
            QTest.keyClicks(target_widget(), step["text"])
        elif do == "cursor":
            cursor = {}
            for ident in step["targets"]:
                mouse(root, "move", built.centre(ident))
                shape = built.widgets[ident].cursor().shape()
                cursor[ident] = CURSORS.get(shape, str(shape))
            mouse(root, "move", QPoint(0, 0))
            records.append({"type": "cursor", "label": step["label"], "cursor": cursor})
        elif do == "tabSequence":
            focus = []
            for _ in range(step["count"]):
                send_key(target_widget(), "Shift+Tab" if step.get("shift") else "Tab")
                focus.append(built.focus())
            records.append({"type": "tabs", "label": step["label"], "focus": focus})
        else:
            raise ValueError(f"unknown step {do}")
        app.processEvents()

    root.close()
    return {"scenario": spec["id"], "framework": "qt", "title": spec["title"], "size": spec["size"],
            "records": records, "screenshots": shots,
            "unsupported": sorted(set(spec.get("features", [])) & UNSUPPORTED)}


for path in sorted((HERE.parent / "scenarios").glob("*.json")):
    spec = json.loads(path.read_text())
    if ONLY and spec["id"] not in ONLY:
        continue
    result = run(spec)
    (OUT / f"{spec['id']}.json").write_text(json.dumps(result, indent=2))
    print(f"qt: {spec['id']} ({len(result['records'])} records)")
