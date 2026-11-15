import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import "Model.js" as Model

// omariglol bar widget — compact face in the Omarchy/Quickshell bar.
// Left click: toggle panel. Middle click: measure all. Right click: refresh/discover context.
// Uses Model.js for argv, parsing and labels; runs `riglol` via Process (no SCPI in QML).
Rectangle {
    id: root

    // --- settings (from manifest defaults / Omarchy settings UI) ---
    property string binary: "riglol"
    property string workspace: ""
    property string scriptPath: ""
    property string measureCommand: ""

    // --- runtime state ---
    property bool measuring: false
    property var lastReport: null
    property string lastError: ""
    property var discovered: []
    property bool discovering: false
    property string discoverySummary: ""
    property bool panelOpen: false

    // --- om-* aligned palette (semantic, theme-friendly neutrals) ---
    readonly property color omBg: "#14151a"
    readonly property color omSurface: "#1b1d24"
    readonly property color omRaised: "#23262f"
    readonly property color omBorder: "#2e323d"
    readonly property color omText: "#e6e8ee"
    readonly property color omMuted: "#8b8d95"
    readonly property color omAccent: "#6e9dff"
    readonly property color omOk: "#4ade80"
    readonly property color omWarn: "#fbbf24"
    readonly property color omError: "#f87171"
    readonly property color omInfo: "#60a5fa"
    readonly property color omIdle: "#6b7280"

    implicitWidth: faceRow.implicitWidth + 16
    implicitHeight: 28
    radius: 6
    color: mouse.containsMouse ? omRaised : "transparent"
    border.color: panelOpen ? omAccent : "transparent"
    border.width: 1

    function stateColor() {
        var s = Model.widgetState(measuring, lastError)
        if (s === "error") return omError
        if (s === "measuring") return omInfo
        if (!lastReport) return omIdle
        // measured vs total heuristic
        var total = lastReport.results ? lastReport.results.length : 0
        var measured = 0
        if (lastReport.results) {
            for (var i = 0; i < lastReport.results.length; i++) {
                var r = lastReport.results[i]
                if (r.is_measurement && !r.error) measured++
            }
        }
        if (total === 0) return omIdle
        if (measured === total) return omOk
        if (measured === 0) return omWarn
        return omWarn
    }

    function runMeasure() {
        if (measuring) return
        lastError = ""
        measuring = true
        measureProc.running = false
        measureProc.buffer = ""
        measureProc.command = scriptPath && scriptPath.length > 0
            ? Model.scriptCommand(binary, scriptPath)
            : Model.measureCommand(binary, workspace, measureCommand)
        measureProc.running = true
    }

    function runDiscover() {
        if (discovering) return
        discovering = true
        discoverySummary = ""
        discoverProc.running = false
        discoverProc.buffer = ""
        discoverProc.command = Model.discoverCommand(binary, workspace)
        discoverProc.running = true
    }

    function pickScript() {
        pickerProc.running = false
        pickerProc.buffer = ""
        pickerProc.command = Model.filePickerCommand()
        pickerProc.running = true
    }

    RowLayout {
        id: faceRow
        anchors.centerIn: parent
        spacing: 6

        Rectangle {
            width: 7; height: 7; radius: 4
            color: root.stateColor()
            Layout.alignment: Qt.AlignVCenter
        }
        Text {
            text: Model.barLabel(lastReport, measuring, lastError)
            color: lastError ? omError : omText
            font.pixelSize: 12
            font.family: "JetBrains Mono"
            Layout.alignment: Qt.AlignVCenter
        }
    }

    MouseArea {
        id: mouse
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton | Qt.MiddleButton | Qt.RightButton
        cursorShape: Qt.PointingHandCursor
        onClicked: function(ev) {
            if (ev.button === Qt.LeftButton) {
                panelOpen = !panelOpen
            } else if (ev.button === Qt.MiddleButton) {
                runMeasure()
            } else if (ev.button === Qt.RightButton) {
                runDiscover()
                panelOpen = true
            }
        }
    }

    // --- panel popup ---
    Loader {
        id: panelLoader
        active: panelOpen
        source: "Panel.qml"
        onLoaded: {
            item.anchorItem = root
            item.widget = root
            item.open = true
        }
    }
    Connections {
        target: panelLoader.item
        ignoreUnknownSignals: true
        function onClosed() { panelOpen = false }
    }

    // --- processes ---
    Process {
        id: measureProc
        property string buffer: ""
        stdout: SplitParser {
            onRead: data => { measureProc.buffer += data + "\n" }
        }
        stderr: SplitParser {
            onRead: data => { measureProc.buffer += data + "\n" }
        }
        onExited: (code, status) => {
            root.measuring = false
            var report = Model.parseReport(measureProc.buffer)
            if (report) {
                root.lastReport = report
                root.lastError = ""
            } else if (code !== 0) {
                root.lastError = Model.firstLine(measureProc.buffer) || ("exit " + code)
            } else {
                // script run or non-report output
                root.lastError = ""
            }
            measureProc.buffer = ""
        }
    }

    Process {
        id: discoverProc
        property string buffer: ""
        stdout: SplitParser {
            onRead: data => { discoverProc.buffer += data + "\n" }
        }
        stderr: SplitParser {
            onRead: data => { discoverProc.buffer += data + "\n" }
        }
        onExited: (code, status) => {
            root.discovering = false
            var report = Model.parseDiscovery(discoverProc.buffer)
            if (report) {
                root.discovered = report.found || []
                root.discoverySummary = Model.discoverySummary(report)
                root.lastError = ""
            } else if (code !== 0) {
                root.lastError = Model.firstLine(discoverProc.buffer) || ("discover exit " + code)
            }
            discoverProc.buffer = ""
        }
    }

    Process {
        id: pickerProc
        property string buffer: ""
        stdout: SplitParser {
            onRead: data => { pickerProc.buffer += data + "\n" }
        }
        onExited: (code, status) => {
            if (code === 0) {
                var path = String(pickerProc.buffer || "").trim().split("\n")[0]
                if (path) root.scriptPath = path
            }
            pickerProc.buffer = ""
        }
    }
}
