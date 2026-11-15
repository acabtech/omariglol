import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import "Model.js" as Model

// Instrument panel for omariglol bar widget.
// Sections: trigger + status, Detect (network) with discovered list, workspace instrument results, script card.
// Follows "Bench in focus" token semantics: dark surfaces, accent action, status dot+label+tint, mono data.
PopupWindow {
    id: panel
    property var widget: null
    property Item anchorItem: null
    signal closed()

    visible: false
    color: "transparent"
    implicitWidth: 380
    implicitHeight: Math.min(content.implicitHeight + 24, 560)

    anchor.item: anchorItem
    anchor.rect.x: 0
    anchor.rect.y: (anchorItem ? anchorItem.height : 28) + 6
    anchor.gravity: Edges.Bottom | Edges.Left

    function closePanel() { visible = false; closed() }

    // palette from widget (om-* aligned)
    readonly property color omBg: widget ? widget.omBg : "#14151a"
    readonly property color omSurface: widget ? widget.omSurface : "#1b1d24"
    readonly property color omRaised: widget ? widget.omRaised : "#23262f"
    readonly property color omBorder: widget ? widget.omBorder : "#2e323d"
    readonly property color omText: widget ? widget.omText : "#e6e8ee"
    readonly property color omMuted: widget ? widget.omMuted : "#8b8d95"
    readonly property color omAccent: widget ? widget.omAccent : "#6e9dff"
    readonly property color omOk: widget ? widget.omOk : "#4ade80"
    readonly property color omWarn: widget ? widget.omWarn : "#fbbf24"
    readonly property color omError: widget ? widget.omError : "#f87171"
    readonly property color omInfo: widget ? widget.omInfo : "#60a5fa"
    readonly property color omIdle: widget ? widget.omIdle : "#6b7280"

    Rectangle {
        anchors.fill: parent
        radius: 12
        color: omSurface
        border.color: omBorder
        border.width: 1

        ColumnLayout {
            id: content
            anchors.fill: parent
            anchors.margins: 12
            spacing: 10

            // Header
            RowLayout {
                Layout.fillWidth: true
                spacing: 8
                Text {
                    text: "omariglol"
                    color: omText
                    font.pixelSize: 14
                    font.bold: true
                    font.family: "JetBrains Mono"
                }
                Rectangle { Layout.fillWidth: true; height: 1; color: "transparent" }
                Text {
                    text: widget && widget.lastReport ? Model.summarize(widget.lastReport) : "no runs yet"
                    color: omMuted
                    font.pixelSize: 11
                }
            }

            // Trigger row
            RowLayout {
                Layout.fillWidth: true
                spacing: 8
                Button {
                    id: triggerBtn
                    Layout.fillWidth: true
                    enabled: widget && !widget.measuring
                    text: widget && widget.scriptPath ? ("Run " + Model.scriptName(widget.scriptPath)) : (widget && widget.measuring ? "Measuring…" : "Measure all")
                    onClicked: if (widget) widget.runMeasure()
                    background: Rectangle {
                        radius: 8
                        color: triggerBtn.down ? Qt.darker(omAccent, 1.2) : (triggerBtn.hovered ? Qt.lighter(omAccent, 1.1) : omAccent)
                    }
                    contentItem: Text {
                        text: triggerBtn.text
                        color: "#0b1020"
                        font.pixelSize: 12
                        font.bold: true
                        horizontalAlignment: Text.AlignHCenter
                        verticalAlignment: Text.AlignVCenter
                        elide: Text.ElideRight
                    }
                }
                Button {
                    id: detectBtn
                    text: widget && widget.discovering ? "Detecting…" : "Detect"
                    enabled: widget && !widget.discovering
                    onClicked: if (widget) widget.runDiscover()
                    background: Rectangle {
                        radius: 8
                        color: detectBtn.down ? omBorder : (detectBtn.hovered ? omRaised : omBg)
                        border.color: omBorder
                    }
                    contentItem: Text {
                        text: detectBtn.text
                        color: omText
                        font.pixelSize: 12
                        horizontalAlignment: Text.AlignHCenter
                        verticalAlignment: Text.AlignVCenter
                    }
                }
                Button {
                    text: "×"
                    onClicked: panel.closePanel()
                    background: Rectangle { radius: 8; color: "transparent" }
                    contentItem: Text { text: "×"; color: omMuted; font.pixelSize: 14; horizontalAlignment: Text.AlignHCenter }
                }
            }

            // Error/status line
            Text {
                visible: widget && widget.lastError && widget.lastError.length > 0
                text: "Error: " + (widget ? widget.lastError : "")
                color: omError
                font.pixelSize: 11
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }

            // Discovered instruments
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 4
                visible: widget && (widget.discovering || (widget.discovered && widget.discovered.length > 0) || (widget.discoverySummary && widget.discoverySummary.length > 0))
                RowLayout {
                    Layout.fillWidth: true
                    Text { text: "Discovered"; color: omMuted; font.pixelSize: 11; font.bold: true }
                    Rectangle { Layout.fillWidth: true; height: 1; color: "transparent" }
                    Text { text: widget ? widget.discoverySummary : ""; color: omMuted; font.pixelSize: 10 }
                }
                Rectangle {
                    Layout.fillWidth: true
                    implicitHeight: Math.min(discoveredCol.implicitHeight + 8, 140)
                    radius: 8
                    color: omBg
                    border.color: omBorder
                    ScrollView {
                        anchors.fill: parent
                        anchors.margins: 4
                        clip: true
                        Column {
                            id: discoveredCol
                            width: parent.width
                            spacing: 2
                            Repeater {
                                model: widget ? widget.discovered : []
                                delegate: Rectangle {
                                    width: discoveredCol.width
                                    height: 34
                                    radius: 6
                                    color: "transparent"
                                    RowLayout {
                                        anchors.fill: parent
                                        anchors.leftMargin: 6; anchors.rightMargin: 6
                                        spacing: 8
                                        Text { text: Model.discoveredGlyph(modelData); color: omAccent; font.pixelSize: 12; font.family: "JetBrains Mono" }
                                        ColumnLayout {
                                            Layout.fillWidth: true; spacing: 0
                                            Text { text: Model.discoveryName(modelData); color: omText; font.pixelSize: 11; elide: Text.ElideRight; Layout.fillWidth: true }
                                            Text { text: Model.discoveryMeta(modelData); color: omMuted; font.pixelSize: 10; elide: Text.ElideRight; Layout.fillWidth: true }
                                        }
                                    }
                                }
                            }
                            Text {
                                visible: widget && widget.discovering
                                text: "Scanning network…"
                                color: omInfo
                                font.pixelSize: 11
                                padding: 6
                            }
                        }
                    }
                }
            }

            // Workspace instrument results
            ColumnLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                spacing: 4
                Text { text: "Instruments"; color: omMuted; font.pixelSize: 11; font.bold: true }
                Rectangle {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    Layout.minimumHeight: 80
                    radius: 8
                    color: omBg
                    border.color: omBorder
                    ScrollView {
                        anchors.fill: parent
                        anchors.margins: 4
                        clip: true
                        Column {
                            id: resultsCol
                            width: parent.width
                            spacing: 2
                            Repeater {
                                model: (widget && widget.lastReport && widget.lastReport.results) ? widget.lastReport.results : []
                                delegate: Rectangle {
                                    width: resultsCol.width
                                    height: 36
                                    radius: 6
                                    color: "transparent"
                                    RowLayout {
                                        anchors.fill: parent
                                        anchors.leftMargin: 6; anchors.rightMargin: 6
                                        spacing: 8
                                        Rectangle {
                                            width: 7; height: 7; radius: 4
                                            color: modelData.error ? omError : (modelData.is_measurement ? omOk : omIdle)
                                        }
                                        Text { text: Model.instrumentGlyph(modelData.type); color: omMuted; font.pixelSize: 11; font.family: "JetBrains Mono" }
                                        ColumnLayout {
                                            Layout.fillWidth: true; spacing: 0
                                            Text {
                                                text: modelData.name || modelData.id || "Instrument"
                                                color: omText; font.pixelSize: 11; elide: Text.ElideRight; Layout.fillWidth: true
                                            }
                                            Text {
                                                text: modelData.command || modelData.type || ""
                                                color: omMuted; font.pixelSize: 10; elide: Text.ElideRight; Layout.fillWidth: true
                                            }
                                        }
                                        Text {
                                            text: Model.resultValue(modelData)
                                            color: modelData.error ? omError : omText
                                            font.pixelSize: 11
                                            font.family: "JetBrains Mono"
                                            horizontalAlignment: Text.AlignRight
                                            Layout.preferredWidth: 110
                                            elide: Text.ElideRight
                                        }
                                    }
                                }
                            }
                            Text {
                                visible: !(widget && widget.lastReport && widget.lastReport.results && widget.lastReport.results.length > 0)
                                text: widget && widget.measuring ? "Measuring instruments…" : "No results yet. Use Detect to find instruments, then Measure all."
                                color: omMuted
                                font.pixelSize: 11
                                wrapMode: Text.Wrap
                                width: resultsCol.width - 12
                                padding: 8
                            }
                        }
                    }
                }
            }

            // Script card
            Rectangle {
                Layout.fillWidth: true
                implicitHeight: scriptCol.implicitHeight + 12
                radius: 8
                color: omBg
                border.color: omBorder
                ColumnLayout {
                    id: scriptCol
                    anchors.fill: parent
                    anchors.margins: 8
                    spacing: 6
                    RowLayout {
                        Layout.fillWidth: true
                        Text { text: "Script"; color: omMuted; font.pixelSize: 11; font.bold: true }
                        Rectangle { Layout.fillWidth: true; height: 1; color: "transparent" }
                        Text {
                            text: (widget && widget.scriptPath) ? Model.scriptName(widget.scriptPath) : "NO SCRIPT"
                            color: (widget && widget.scriptPath) ? omAccent : omMuted
                            font.pixelSize: 11
                            font.family: "JetBrains Mono"
                            elide: Text.ElideMiddle
                            Layout.maximumWidth: 180
                        }
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 8
                        Button {
                            text: (widget && widget.scriptPath) ? "Run script" : "Pick script"
                            onClicked: {
                                if (!widget) return
                                if (widget.scriptPath) widget.runMeasure()
                                else widget.pickScript()
                            }
                            background: Rectangle { radius: 6; color: omRaised; border.color: omBorder }
                            contentItem: Text { text: parent.text; color: omText; font.pixelSize: 11; horizontalAlignment: Text.AlignHCenter }
                        }
                        Button {
                            visible: widget && widget.scriptPath && widget.scriptPath.length > 0
                            text: "Change"
                            onClicked: if (widget) widget.pickScript()
                            background: Rectangle { radius: 6; color: "transparent"; border.color: omBorder }
                            contentItem: Text { text: "Change"; color: omMuted; font.pixelSize: 11; horizontalAlignment: Text.AlignHCenter }
                        }
                        Button {
                            visible: widget && widget.scriptPath && widget.scriptPath.length > 0
                            text: "Clear"
                            onClicked: if (widget) widget.scriptPath = ""
                            background: Rectangle { radius: 6; color: "transparent"; border.color: omBorder }
                            contentItem: Text { text: "Clear"; color: omMuted; font.pixelSize: 11; horizontalAlignment: Text.AlignHCenter }
                        }
                    }
                }
            }

            Text {
                text: "Left click: panel · Middle: measure · Right: detect"
                color: omMuted
                font.pixelSize: 10
                Layout.alignment: Qt.AlignHCenter
            }
        }
    }
}
