import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Commons as Commons
import qs.Ui
import "Model.js" as Model

// omariglol — the bar widget's panel.
//
// A compact instrument console:
//   1. a trigger that measures every instrument in the workspace, or runs the
//      loaded Python script;
//   2. a Detect action that scans the network from the shell itself and lists
//      what it finds, without involving the desktop app;
//   3. the workspace instrument list, with each one's last reading or error;
//   4. a clickable script card: NO SCRIPT opens a file picker, a loaded script
//      runs, and CHANGE replaces it.
//
// The panel owns the data; BarWidget.qml reads it for the bar face. Side
// effects stay limited to `Process` calls to the `riglol` binary and zenity —
// no network code, no dynamic evaluation (Omarchy plugins run unsandboxed).
Panel {
  id: root
  moduleName: "tech.alatcerdas.omariglol"
  ipcTarget: "tech.alatcerdas.omariglol"
  manageIpc: false

  property var anchorItem: null

  // The bar tracks the widget mounted in its slot — BarWidget.qml — not this
  // nested panel. Everything the bar identifies a panel by has to be that
  // widget: the popout coordinator compares against `slot.activeItem`, and
  // switchPanelFrom looks the slot up the same way.
  property var hostWidget: null
  readonly property var barIdentity: hostWidget || root

  // ---- Settings (mirrored from the host widget's shell.json entry).
  readonly property string binary: setting("binary", "riglol")
  readonly property string workspace: setting("workspace", "")
  readonly property string scriptPath: setting("scriptPath", "")
  readonly property string measureCommand: setting("measureCommand", "")

  // ---- State.
  property var report: null
  property bool measuring: false
  property string lastError: ""

  property bool autoscaling: false
  property string autoscaleStatus: ""

  property var discovery: null
  property bool detecting: false
  property string discoveryError: ""

  property bool runningScript: false
  property bool pickingScript: false
  property string scriptStatus: ""

  readonly property color contentForeground: bar ? bar.foreground : Commons.Color.foreground
  readonly property string contentFontFamily: bar ? bar.fontFamily : Style.font.family

  readonly property bool hasScript: root.scriptPath !== ""
  readonly property bool busy: root.measuring || root.runningScript
  readonly property bool anyBusy: root.busy || root.autoscaling || root.detecting || root.pickingScript
  readonly property string triggerLabel: root.hasScript ? "RUN SCRIPT" : "MEASURE ALL"
  readonly property string detectLabel: root.detecting
    ? "SCANNING\u2026"
    : (root.discovery ? "RESCAN" : "DETECT")

  readonly property var results: root.report && Array.isArray(root.report.results) ? root.report.results : []
  readonly property var discovered: root.discovery && Array.isArray(root.discovery.found) ? root.discovery.found : []
  readonly property string workspaceLabel: root.report
    ? (root.report.workspace || "workspace")
    : "no workspace loaded"
  readonly property string scriptCardTitle: root.hasScript ? "SCRIPT LOADED" : "NO SCRIPT"
  readonly property string scriptCardDetail: root.hasScript
    ? root.scriptPath
    : (root.pickingScript ? "Waiting for the file picker\u2026" : "Click to choose a Python script")

  // ---- Lifecycle. Opening the panel populates it if it has never run.
  function open() {
    setCenterHoverRevealSuppressed(false)
    root.controller.show()
    if (!root.report && !root.busy) root.measure()
  }

  function openFromHotkey() {
    root.controller.show()
    if (!root.report && !root.busy) root.measure()
    // Set after showing, not before: showing hands the popout coordinator
    // over, which closes whichever panel was open, and that close clears the
    // shared flag. Deferring means the panel taking over always wins.
    Qt.callLater(function() {
      if (root.opened) setCenterHoverRevealSuppressed(true)
    })
  }

  function close() {
    setCenterHoverRevealSuppressed(false)
    root.controller.hide()
  }

  function toggle() {
    if (root.opened) root.close()
    else root.openFromHotkey()
  }

  function switchPanel(direction) {
    if (root.bar && typeof root.bar.switchPanelFrom === "function")
      return root.bar.switchPanelFrom(root.barIdentity, direction)
    return false
  }

  function setCenterHoverRevealSuppressed(value) {
    if (root.bar && typeof root.bar.setCenterHoverRevealSuppressed === "function")
      root.bar.setCenterHoverRevealSuppressed(value)
    else if (root.bar && "centerHoverRevealSuppressed" in root.bar)
      root.bar.centerHoverRevealSuppressed = value
  }

  // ---- Actions.
  function refresh() {
    root.measure()
  }

  // Ask every instrument that supports it to fit its display to the signal.
  function autoscale() {
    if (root.anyBusy) return
    root.autoscaling = true
    root.autoscaleStatus = ""
    autoscaleProc.running = true
  }

  function measure() {
    if (root.anyBusy) return
    root.measuring = true
    root.lastError = ""
    measureProc.running = true
  }

  // The trigger: a script wins when one is configured, otherwise measure all.
  function trigger() {
    if (root.anyBusy) return
    if (root.hasScript) root.runScript()
    else root.measure()
  }

  function runScript() {
    if (!root.hasScript || root.anyBusy) return
    root.runningScript = true
    root.scriptStatus = ""
    scriptProc.running = true
  }

  // Scan the network without going through the desktop app. The CLI owns the
  // network policy; the panel only renders what comes back.
  function detect() {
    if (root.anyBusy) return
    root.detecting = true
    root.discoveryError = ""
    discoveryProc.running = true
  }

  function chooseScript() {
    if (root.anyBusy) return
    root.pickingScript = true
    root.scriptStatus = ""
    pickerProc.running = true
  }

  function handleScriptTap() {
    if (root.anyBusy) return
    if (root.hasScript) root.runScript()
    else root.chooseScript()
  }

  function loadScript(path) {
    var picked = String(path || "").trim()
    if (!picked) return
    root.persistScriptPath(picked)
    root.scriptStatus = "Loaded " + Model.scriptName(picked)
  }

  // Mirror the choice locally and write it back to shell.json. Picking a
  // script should survive a shell restart, matching the clock widget's
  // format cycling.
  function persistScriptPath(path) {
    if (root.hostWidget && typeof root.hostWidget.setScriptPath === "function") {
      root.hostWidget.setScriptPath(path)
      return
    }

    var entry = { id: root.moduleName }
    for (var key in root.settings)
      if (key !== "id") entry[key] = root.settings[key]
    entry.scriptPath = path
    root.settings = entry
    if (root.bar && root.bar.shell && typeof root.bar.shell.updateEntryInline === "function")
      root.bar.shell.updateEntryInline(root.moduleName, entry)
  }

  Process {
    id: measureProc
    command: Model.measureCommand(root.binary, root.workspace, root.measureCommand)
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        root.measuring = false
        var parsed = Model.parseReport(text)
        if (parsed) {
          root.report = parsed
          root.lastError = ""
        } else {
          root.lastError = Model.firstLine(text) || "measure failed"
        }
      }
    }
    onExited: function(code) {
      if (code !== 0 && root.measuring) {
        root.measuring = false
        if (!root.lastError) root.lastError = root.binary + " exited " + code
      }
    }
  }

  Process {
    id: autoscaleProc
    command: Model.autoscaleCommand(root.binary, root.workspace)
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        root.autoscaling = false
        var parsed = Model.parseReport(text)
        if (parsed) {
          root.autoscaleStatus = Model.autoscaleSummary(parsed)
        } else {
          root.autoscaleStatus = Model.firstLine(text) || "autoset failed"
        }
      }
    }
    onExited: function(code) {
      if (code !== 0 && root.autoscaling) {
        root.autoscaling = false
        if (!root.autoscaleStatus) root.autoscaleStatus = root.binary + " exited " + code
      }
    }
  }

  Process {
    id: discoveryProc
    command: Model.discoverCommand(root.binary, root.workspace)
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        root.detecting = false
        var parsed = Model.parseDiscovery(text)
        if (parsed) {
          root.discovery = parsed
          root.discoveryError = ""
        } else {
          root.discoveryError = Model.firstLine(text) || "detection failed"
        }
      }
    }
    stderr: StdioCollector {
      id: discoveryStderr
      waitForEnd: true
    }
    onExited: function(code) {
      if (code !== 0 && root.detecting) {
        root.detecting = false
        if (!root.discoveryError)
          root.discoveryError = Model.firstLine(discoveryStderr.text)
            || (root.binary + " discover exited " + code)
      }
    }
  }

  Process {
    id: scriptProc
    command: Model.scriptCommand(root.binary, root.scriptPath)
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        root.runningScript = false
        root.scriptStatus = Model.firstLine(text) || "done"
      }
    }
    onExited: function(code) {
      root.runningScript = false
      if (code !== 0) root.scriptStatus = "script exited " + code
    }
  }

  Process {
    id: pickerProc
    command: Model.filePickerCommand()
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        root.pickingScript = false
        var picked = String(text || "").trim()
        if (picked) root.loadScript(picked)
      }
    }
    onExited: function(code) {
      root.pickingScript = false
      // zenity exits 1 when the dialog is dismissed; that is not an error.
      if (code !== 0 && code !== 1)
        root.scriptStatus = "file picker exited " + code
    }
  }

  KeyboardPanel {
    id: panel
    anchorItem: root.anchorItem
    owner: root.barIdentity
    bar: root.bar
    open: root.opened
    centerOnBar: true
    focusTarget: keyCatcher
    contentWidth: panel.fittedContentWidth(Style.space(460))
    contentHeight: panel.fittedContentHeight(contentColumn.implicitHeight)

    PanelKeyCatcher {
      id: keyCatcher
      anchors.fill: parent
      onCloseRequested: root.close()
      onReturnRequested: root.trigger()
      onTabRequested: function(direction) { root.switchPanel(direction) }

      Flickable {
        id: scroll
        anchors.fill: parent
        contentWidth: width
        contentHeight: contentColumn.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        interactive: contentHeight > height

        Column {
          id: contentColumn
          width: scroll.width
          spacing: Style.space(14)

          // ---- Hero: title, workspace, count, error.
          Item {
            width: parent.width
            height: heroColumn.implicitHeight + Style.space(8)

            Column {
              id: heroColumn
              anchors.left: parent.left
              anchors.leftMargin: Style.space(16)
              anchors.right: parent.right
              anchors.rightMargin: Style.space(16)
              anchors.verticalCenter: parent.verticalCenter
              spacing: Style.space(3)

              Row {
                width: parent.width

                Text {
                  id: heroTitle
                  text: "OMARIGLOL"
                  color: root.contentForeground
                  font.family: root.contentFontFamily
                  font.pixelSize: Style.font.title
                  font.bold: true
                  font.letterSpacing: 1
                }

                Item {
                  width: Math.max(0, parent.width - heroTitle.width - heroCount.width)
                  height: 1
                }

                Text {
                  id: heroCount
                  text: root.results.length + " IN WORKSPACE"
                  color: Qt.darker(root.contentForeground, 1.6)
                  font.family: root.contentFontFamily
                  font.pixelSize: Style.font.caption
                  font.letterSpacing: 1
                }
              }

              Text {
                text: root.workspaceLabel
                color: Qt.darker(root.contentForeground, 1.4)
                font.family: root.contentFontFamily
                font.pixelSize: Style.font.bodySmall
                font.letterSpacing: 1
              }

              Text {
                visible: root.lastError !== ""
                width: parent.width
                text: root.lastError
                color: Commons.Color.urgent
                font.family: root.contentFontFamily
                font.pixelSize: Style.font.bodySmall
                wrapMode: Text.WordWrap
              }
            }
          }

          // ---- Actions: trigger (measure / run script) and network detection.
          Row {
            id: actionRow
            width: parent.width - Style.space(32)
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: Style.space(8)

            Rectangle {
              id: triggerButton
              width: Math.max(0, parent.width - autosetButton.width - detectButton.width - parent.spacing * 2)
              height: Style.space(40)
              radius: Style.cornerRadius
              color: triggerMouse.containsMouse && !root.anyBusy
                ? Style.hoverFillFor(root.contentForeground, Commons.Color.accent)
                : Qt.rgba(root.contentForeground.r, root.contentForeground.g, root.contentForeground.b, 0.08)
              border.width: Style.spacing.hairline
              border.color: Style.normalBorderFor(root.contentForeground, Commons.Color.accent)

              Text {
                anchors.centerIn: parent
                text: root.measuring
                  ? "MEASURING\u2026"
                  : (root.runningScript ? "RUNNING\u2026" : root.triggerLabel)
                color: root.anyBusy
                  ? Qt.darker(root.contentForeground, 1.6)
                  : root.contentForeground
                font.family: root.contentFontFamily
                font.pixelSize: Style.font.body
                font.bold: true
                font.letterSpacing: 1
              }

              MouseArea {
                id: triggerMouse
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: root.anyBusy ? Qt.ArrowCursor : Qt.PointingHandCursor
                enabled: !root.anyBusy
                onClicked: root.trigger()
              }
            }

            Rectangle {
              id: autosetButton
              width: autosetText.implicitWidth + Style.space(26)
              height: triggerButton.height
              radius: Style.cornerRadius
              color: autosetMouse.containsMouse && !root.anyBusy
                ? Style.hoverFillFor(root.contentForeground, Commons.Color.accent)
                : "transparent"
              border.width: Style.spacing.hairline
              border.color: root.autoscaling
                ? Style.normalBorderFor(root.contentForeground, Commons.Color.accent)
                : Qt.rgba(root.contentForeground.r, root.contentForeground.g, root.contentForeground.b, 0.22)

              Text {
                id: autosetText
                anchors.centerIn: parent
                text: root.autoscaling ? "AUTOSET\u2026" : "AUTOSET"
                color: root.autoscaling
                  ? Qt.darker(root.contentForeground, 1.5)
                  : root.contentForeground
                font.family: root.contentFontFamily
                font.pixelSize: Style.font.bodySmall
                font.bold: true
                font.letterSpacing: 1
              }

              MouseArea {
                id: autosetMouse
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: root.anyBusy ? Qt.ArrowCursor : Qt.PointingHandCursor
                enabled: !root.anyBusy
                onClicked: root.autoscale()
              }
            }

            Rectangle {
              id: detectButton
              width: detectText.implicitWidth + Style.space(26)
              height: triggerButton.height
              radius: Style.cornerRadius
              color: detectMouse.containsMouse && !root.anyBusy
                ? Style.hoverFillFor(root.contentForeground, Commons.Color.accent)
                : "transparent"
              border.width: Style.spacing.hairline
              border.color: root.discovery
                ? Style.normalBorderFor(root.contentForeground, Commons.Color.accent)
                : Qt.rgba(root.contentForeground.r, root.contentForeground.g, root.contentForeground.b, 0.22)

              Text {
                id: detectText
                anchors.centerIn: parent
                text: root.detectLabel
                color: root.detecting
                  ? Qt.darker(root.contentForeground, 1.5)
                  : root.contentForeground
                font.family: root.contentFontFamily
                font.pixelSize: Style.font.bodySmall
                font.bold: true
                font.letterSpacing: 1
              }

              MouseArea {
                id: detectMouse
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: root.anyBusy ? Qt.ArrowCursor : Qt.PointingHandCursor
                enabled: !root.anyBusy
                onClicked: root.detect()
              }
            }
          }

          Text {
            visible: root.autoscaleStatus !== ""
            width: parent.width - Style.space(32)
            anchors.horizontalCenter: parent.horizontalCenter
            text: root.autoscaleStatus
            color: Qt.darker(root.contentForeground, 1.4)
            font.family: root.contentFontFamily
            font.pixelSize: Style.font.caption
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
          }

          // ---- Workspace instrument list.
          Column {
            width: parent.width
            spacing: Style.space(2)

            PanelSectionHeader {
              leftPadding: Style.space(16)
              rightPadding: Style.space(16)
              width: parent.width
              text: "INSTRUMENTS"
              foreground: root.contentForeground
              fontFamily: root.contentFontFamily
            }

            Text {
              visible: root.results.length === 0
              leftPadding: Style.space(16)
              rightPadding: Style.space(16)
              width: parent.width
              text: root.measuring
                ? "Measuring\u2026"
                : "No instruments in the workspace. Try Detect above."
              color: Qt.darker(root.contentForeground, 1.5)
              font.family: root.contentFontFamily
              font.pixelSize: Style.font.bodySmall
              font.italic: true
              wrapMode: Text.WordWrap
            }

            Repeater {
              model: root.results

              Item {
                required property var modelData
                width: parent.width
                height: Math.max(labelColumn.implicitHeight, Style.space(28)) + Style.space(8)

                Text {
                  id: glyph
                  anchors.left: parent.left
                  anchors.leftMargin: Style.space(16)
                  anchors.verticalCenter: parent.verticalCenter
                  text: Model.instrumentGlyph(modelData.instrument_type)
                  color: modelData.error ? Commons.Color.urgent : root.contentForeground
                  font.family: root.contentFontFamily
                  font.pixelSize: Style.font.body
                }

                Text {
                  id: value
                  anchors.right: parent.right
                  anchors.rightMargin: Style.space(16)
                  anchors.verticalCenter: parent.verticalCenter
                  width: Style.space(96)
                  horizontalAlignment: Text.AlignRight
                  elide: Text.ElideRight
                  text: Model.resultValue(modelData)
                  color: modelData.error ? Commons.Color.urgent : root.contentForeground
                  font.family: root.contentFontFamily
                  font.pixelSize: Style.font.body
                }

                Column {
                  id: labelColumn
                  anchors.left: glyph.right
                  anchors.leftMargin: Style.space(10)
                  anchors.right: value.left
                  anchors.rightMargin: Style.space(10)
                  anchors.verticalCenter: parent.verticalCenter
                  spacing: Style.space(1)

                  Text {
                    width: parent.width
                    text: modelData.label
                    color: root.contentForeground
                    font.family: root.contentFontFamily
                    font.pixelSize: Style.font.body
                    elide: Text.ElideRight
                  }

                  Text {
                    width: parent.width
                    text: modelData.instrument_type + " \u00B7 " + modelData.command
                    color: Qt.darker(root.contentForeground, 1.5)
                    font.family: root.contentFontFamily
                    font.pixelSize: Style.font.caption
                    elide: Text.ElideRight
                  }
                }
              }
            }
          }

          // ---- Network detection results.
          Column {
            width: parent.width
            spacing: Style.space(2)
            visible: root.detecting || root.discovery !== null || root.discoveryError !== ""

            Item {
              width: parent.width
              height: Math.max(detectedTitle.implicitHeight, detectedSummary.implicitHeight) + Style.space(4)

              PanelSectionHeader {
                id: detectedTitle
                anchors.left: parent.left
                anchors.leftMargin: Style.space(16)
                text: "DETECTED ON NETWORK"
                foreground: root.contentForeground
                fontFamily: root.contentFontFamily
              }

              Text {
                id: detectedSummary
                anchors.right: parent.right
                anchors.rightMargin: Style.space(16)
                anchors.top: parent.top
                anchors.topMargin: Style.space(2)
                text: root.detecting
                  ? "scanning\u2026"
                  : Model.discoverySummary(root.discovery)
                color: Qt.darker(root.contentForeground, 1.5)
                font.family: root.contentFontFamily
                font.pixelSize: Style.font.caption
              }
            }

            Text {
              visible: root.discoveryError !== ""
              leftPadding: Style.space(16)
              rightPadding: Style.space(16)
              width: parent.width
              text: root.discoveryError
              color: Commons.Color.urgent
              font.family: root.contentFontFamily
              font.pixelSize: Style.font.bodySmall
              wrapMode: Text.WordWrap
            }

            Text {
              visible: !root.detecting
                && root.discoveryError === ""
                && root.discovered.length === 0
              leftPadding: Style.space(16)
              rightPadding: Style.space(16)
              width: parent.width
              text: "No instruments detected."
              color: Qt.darker(root.contentForeground, 1.5)
              font.family: root.contentFontFamily
              font.pixelSize: Style.font.bodySmall
              font.italic: true
            }

            Repeater {
              model: root.discovered

              Item {
                required property var modelData
                width: parent.width
                height: Math.max(detectedLabelColumn.implicitHeight, Style.space(28)) + Style.space(8)

                Text {
                  id: detectedGlyph
                  anchors.left: parent.left
                  anchors.leftMargin: Style.space(16)
                  anchors.verticalCenter: parent.verticalCenter
                  text: Model.discoveredGlyph(modelData)
                  color: root.contentForeground
                  font.family: root.contentFontFamily
                  font.pixelSize: Style.font.body
                }

                Text {
                  id: detectedMethod
                  anchors.right: parent.right
                  anchors.rightMargin: Style.space(16)
                  anchors.verticalCenter: parent.verticalCenter
                  width: Style.space(96)
                  horizontalAlignment: Text.AlignRight
                  elide: Text.ElideRight
                  text: Model.discoveryMethod(modelData)
                  color: Qt.darker(root.contentForeground, 1.5)
                  font.family: root.contentFontFamily
                  font.pixelSize: Style.font.caption
                }

                Column {
                  id: detectedLabelColumn
                  anchors.left: detectedGlyph.right
                  anchors.leftMargin: Style.space(10)
                  anchors.right: detectedMethod.left
                  anchors.rightMargin: Style.space(10)
                  anchors.verticalCenter: parent.verticalCenter
                  spacing: Style.space(1)

                  Text {
                    width: parent.width
                    text: Model.discoveryName(modelData)
                    color: root.contentForeground
                    font.family: root.contentFontFamily
                    font.pixelSize: Style.font.body
                    elide: Text.ElideRight
                  }

                  Text {
                    width: parent.width
                    text: Model.discoveryMeta(modelData)
                    color: Qt.darker(root.contentForeground, 1.5)
                    font.family: root.contentFontFamily
                    font.pixelSize: Style.font.caption
                    elide: Text.ElideRight
                  }
                }
              }
            }
          }

          // ---- Script card: choose when empty, run when loaded, change always.
          Rectangle {
            id: scriptButton
            width: parent.width - Style.space(32)
            anchors.horizontalCenter: parent.horizontalCenter
            height: Style.space(52)
            radius: Style.cornerRadius
            color: (scriptMouse.containsMouse || changeMouse.containsMouse) && !root.anyBusy
              ? Style.hoverFillFor(root.contentForeground, Commons.Color.accent)
              : "transparent"
            border.width: Style.spacing.hairline
            border.color: root.hasScript
              ? Style.normalBorderFor(root.contentForeground, Commons.Color.accent)
              : Qt.rgba(root.contentForeground.r, root.contentForeground.g, root.contentForeground.b, 0.15)

            MouseArea {
              id: scriptMouse
              anchors.fill: parent
              hoverEnabled: true
              cursorShape: root.anyBusy ? Qt.ArrowCursor : Qt.PointingHandCursor
              enabled: !root.anyBusy
              onClicked: root.handleScriptTap()
            }

            Text {
              id: scriptGlyph
              anchors.left: parent.left
              anchors.leftMargin: Style.space(14)
              anchors.verticalCenter: parent.verticalCenter
              text: "PY"
              color: root.hasScript
                ? Commons.Color.accent
                : Qt.darker(root.contentForeground, 1.8)
              font.family: root.contentFontFamily
              font.pixelSize: Style.font.bodySmall
              font.bold: true
              font.letterSpacing: 1
            }

            Column {
              anchors.left: scriptGlyph.right
              anchors.leftMargin: Style.space(10)
              anchors.right: changeText.left
              anchors.rightMargin: Style.space(12)
              anchors.verticalCenter: parent.verticalCenter
              spacing: Style.space(2)

              Text {
                width: parent.width
                text: root.scriptCardTitle
                color: root.hasScript
                  ? root.contentForeground
                  : Qt.darker(root.contentForeground, 1.4)
                font.family: root.contentFontFamily
                font.pixelSize: Style.font.bodySmall
                font.bold: true
                font.letterSpacing: 1
              }

              Text {
                width: parent.width
                text: root.scriptCardDetail
                color: Qt.darker(root.contentForeground, 1.6)
                font.family: root.contentFontFamily
                font.pixelSize: Style.font.caption
                elide: Text.ElideRight
              }
            }

            Text {
              id: changeText
              anchors.right: parent.right
              anchors.rightMargin: Style.space(14)
              anchors.verticalCenter: parent.verticalCenter
              text: root.hasScript ? "CHANGE" : "BROWSE"
              color: changeMouse.containsMouse && !root.anyBusy
                ? Commons.Color.accent
                : Qt.darker(root.contentForeground, 1.5)
              font.family: root.contentFontFamily
              font.pixelSize: Style.font.caption
              font.bold: true
              font.letterSpacing: 1
            }

            MouseArea {
              id: changeMouse
              anchors.fill: changeText
              hoverEnabled: true
              cursorShape: root.anyBusy ? Qt.ArrowCursor : Qt.PointingHandCursor
              enabled: !root.anyBusy
              onClicked: root.chooseScript()
            }
          }

          Text {
            visible: root.scriptStatus !== ""
            width: parent.width - Style.space(32)
            anchors.horizontalCenter: parent.horizontalCenter
            text: root.scriptStatus
            color: Qt.darker(root.contentForeground, 1.5)
            font.family: root.contentFontFamily
            font.pixelSize: Style.font.caption
            elide: Text.ElideRight
          }

          // ---- Footer: measurement summary.
          Item {
            width: parent.width
            height: footerColumn.implicitHeight + Style.space(8)

            Column {
              id: footerColumn
              anchors.left: parent.left
              anchors.leftMargin: Style.space(16)
              anchors.right: parent.right
              anchors.rightMargin: Style.space(16)
              anchors.verticalCenter: parent.verticalCenter
              spacing: Style.space(2)

              Text {
                visible: root.report !== null
                text: Model.summarize(root.report)
                color: Qt.darker(root.contentForeground, 1.5)
                font.family: root.contentFontFamily
                font.pixelSize: Style.font.caption
              }
            }
          }
        }
      }
    }
  }
}
