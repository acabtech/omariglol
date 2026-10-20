// omariglol — details panel for the bar widget.
//
// STATUS: skeleton. Loaded by BarWidget.qml once the real contract is known.
//
// Intended behaviour:
//   - show the instrument's *IDN? and current status
//   - offer a small set of saved measurements (frequency, Vpp, ...)
//   - run them via the `omariglol` CLI and show the result inline
//
// Deliberately no network code and no dynamic evaluation: Omarchy plugins run
// unsandboxed inside the long-lived shell process, so this stays small enough
// to audit at a glance.

import QtQuick

Item {
    id: panel

    implicitWidth: 320
    implicitHeight: 200

    Rectangle {
        anchors.fill: parent
        radius: 8
        color: "transparent"

        Text {
            anchors.centerIn: parent
            text: "omariglol"
            color: "transparent"
        }
    }
}
