import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Commons as Commons
import qs.Ui
import "Model.js" as Model

// omariglol — the bar widget's panel.
//
// A small menu with three things:
//   1. a trigger button that measures every instrument in the workspace at
//      once — unless a Python script is configured, in which case it runs the
//      script instead;
//   2. the list of instruments, with each one's last reading or error;
//   3. a small button indicating the loaded Python script (optional).
//
// The panel owns the data; BarWidget.qml reads it for the bar face. The only
// side effect is a `Process` call to the `riglol` binary — no network code, no
// dynamic evaluation (Omarchy plugins run unsandboxed).
Panel {
  id: root
  moduleName: "acabtech.omariglol"
  ipcTarget: "acabtech.omariglol"
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
  property bool runningScript: false
  property string scriptStatus: ""

  readonly property color contentForeground: bar ? bar.foreground : Commons.Color.foreground
  readonly property string contentFontFamily: bar ? bar.fontFamily : Style.font.family

  readonly property bool hasScript: root.scriptPath !== ""
  readonly property bool busy: root.measuring || root.runningScript
  readonly property string triggerLabel: root.hasScript ? "RUN SCRIPT" : "MEASURE ALL"
  readonly property var results: root.report && Array.isArray(root.report.results) ? root.report.results : []

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

  function measure() {
    if (root.busy) return
    root.measuring = true
    root.lastError = ""
    measureProc.running = true
  }

  // The trigger: a script wins when one is configured, otherwise measure all.
  function trigger() {
    if (root.hasScript) root.runScript()
    else root.measure()
  }

  function runScript() {
    if (!root.hasScript || root.busy) return
    root.runningScript = true
    root.scriptStatus = ""
    scriptProc.running = true
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

          // ---- Hero: title, workspace, error.
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

              Text {
                text: "OMARIGLOL"
                color: root.contentForeground
                font.family: root.contentFontFamily
                font.pixelSize: Style.font.title
                font.bold: true
                font.letterSpacing: 1
              }

              Text {
                text: root.report
                  ? (root.report.workspace || "workspace")
                  : "no workspace loaded"
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

          // ---- Trigger: measure all, or run the script when one is loaded.
          Rectangle {
            id: triggerButton
            width: parent.width - Style.space(32)
            anchors.horizontalCenter: parent.horizontalCenter
            height: Style.space(40)
            radius: Style.cornerRadius
            color: triggerMouse.containsMouse && !root.busy
              ? Style.hoverFillFor(root.contentForeground, Commons.Color.accent)
              : Qt.rgba(root.contentForeground.r, root.contentForeground.g, root.contentForeground.b, 0.08)
            border.width: Style.spacing.hairline
            border.color: Style.normalBorderFor(root.contentForeground, Commons.Color.accent)

            Text {
              anchors.centerIn: parent
              text: root.measuring
                ? "MEASURING\u2026"
                : (root.runningScript ? "RUNNING\u2026" : root.triggerLabel)
              color: root.contentForeground
              font.family: root.contentFontFamily
              font.pixelSize: Style.font.body
              font.bold: true
              font.letterSpacing: 1
            }

            MouseArea {
              id: triggerMouse
              anchors.fill: parent
              hoverEnabled: true
              cursorShape: root.busy ? Qt.ArrowCursor : Qt.PointingHandCursor
              enabled: !root.busy
              onClicked: root.trigger()
            }
          }

          // ---- Instrument list.
          Column {
            width: parent.width
            spacing: Style.space(2)

            Text {
              leftPadding: Style.space(16)
              text: "INSTRUMENTS"
              color: Qt.darker(root.contentForeground, 1.5)
              font.family: root.contentFontFamily
              font.pixelSize: Style.font.bodySmall
              font.letterSpacing: 1
            }

            Text {
              visible: root.results.length === 0
              leftPadding: Style.space(16)
              text: root.measuring
                ? "Measuring\u2026"
                : "No instruments. Add some in the desktop app."
              color: Qt.darker(root.contentForeground, 1.5)
              font.family: root.contentFontFamily
              font.pixelSize: Style.font.bodySmall
              font.italic: true
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

          // ---- Script indicator / runner.
          Rectangle {
            id: scriptButton
            width: parent.width - Style.space(32)
            anchors.horizontalCenter: parent.horizontalCenter
            height: Style.space(30)
            radius: Style.cornerRadius
            color: scriptMouse.containsMouse && root.hasScript
              ? Style.hoverFillFor(root.contentForeground, Commons.Color.accent)
              : "transparent"
            border.width: Style.spacing.hairline
            border.color: Qt.rgba(root.contentForeground.r, root.contentForeground.g, root.contentForeground.b, 0.15)

            Text {
              anchors.centerIn: parent
              text: root.hasScript
                ? ("SCRIPT: " + Model.scriptName(root.scriptPath))
                : "NO SCRIPT"
              color: root.hasScript
                ? root.contentForeground
                : Qt.darker(root.contentForeground, 1.8)
              font.family: root.contentFontFamily
              font.pixelSize: Style.font.bodySmall
              font.letterSpacing: 1
            }

            MouseArea {
              id: scriptMouse
              anchors.fill: parent
              hoverEnabled: true
              cursorShape: root.hasScript ? Qt.PointingHandCursor : Qt.ArrowCursor
              enabled: root.hasScript && !root.busy
              onClicked: root.runScript()
            }
          }

          // ---- Footer: summary and script status.
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

              Text {
                visible: root.scriptStatus !== ""
                width: parent.width
                text: root.scriptStatus
                color: Qt.darker(root.contentForeground, 1.4)
                font.family: root.contentFontFamily
                font.pixelSize: Style.font.caption
                elide: Text.ElideRight
              }
            }
          }
        }
      }
    }
  }
}
