// omariglol — Omarchy bar widget.
//
// STATUS: skeleton. This file does not yet match the real plugin contract.
//
// Omarchy gives each plugin kind a specific interface, and third-party plugins
// receive a reduced one scoped to their own service and lifecycle. Rather than
// guess at that interface, clone a built-in of the same kind and mirror its
// structure exactly:
//
//     omarchy plugin clone omarchy.clock --edit
//
// The clone lands in ~/.config/omarchy/plugins/ and reloads live on save.
//
// What this widget should become:
//   - a compact label showing the selected instrument (see Model.js)
//   - clicking it opens Panel.qml
//   - a measurement is triggered by running the `omariglol` binary, not by
//     speaking SCPI from QML, so the protocol stays in one place

import QtQuick

Item {
    id: root

    // The instrument this widget instance is bound to. Populated from the
    // shell's per-plugin settings once the real contract is in place.
    property string instrument: ""

    // Placeholder geometry; replace with the bar-widget contract.
    implicitWidth: 24
    implicitHeight: 24

    Rectangle {
        anchors.fill: parent
        radius: 4
        color: "transparent"
        border.width: 0

        Text {
            anchors.centerIn: parent
            text: "LXI"
            font.pixelSize: 10
            color: "transparent"
        }
    }
}
