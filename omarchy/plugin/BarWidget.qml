pragma ComponentBehavior: Bound
import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui
import "Model.js" as Model

// omariglol — Omarchy bar widget.
//
// A compact mark showing how many instruments the last run measured, with a
// panel that lists the workspace's instruments and fires a simultaneous
// measurement of all of them. The widget never speaks SCPI: it shells out to
// the `riglol` binary, so the protocol stays in one place and this file stays
// small enough to audit (Omarchy plugins run unsandboxed).
//
// The panel owns the data (the report, the measuring flag, the error); this
// widget reads it through the loader, the same way omarchy.weather does.
BarWidget {
  id: root
  moduleName: "acabtech.omariglol"

  // The panel owns the report; the bar face is a read-out of it.
  readonly property var report: panelLoader.item ? panelLoader.item.report : null
  readonly property bool measuring: panelLoader.item ? panelLoader.item.measuring === true : false
  readonly property string lastError: panelLoader.item ? panelLoader.item.lastError : ""
  readonly property string barText: Model.barLabel(root.report, root.measuring, root.lastError)

  function injectPanel() {
    var target = panelLoader.item
    if (!target) return
    if ("bar" in target) target.bar = root.bar
    if ("settings" in target) target.settings = root.settings
    if ("anchorItem" in target) target.anchorItem = button
    if ("hostWidget" in target) target.hostWidget = root
  }

  function refresh() {
    if (panelLoader.item && panelLoader.item.refresh) panelLoader.item.refresh()
  }

  function measure() {
    if (panelLoader.item && panelLoader.item.measure) panelLoader.item.measure()
  }

  function togglePanel() {
    if (panelLoader.item && panelLoader.item.toggle) panelLoader.item.toggle()
  }

  // Shape contract for shell.summon/hide/toggle routing (Bar.findPanelWidget
  // requires open/close/opened on the bar-widget root). Open maps to the
  // panel's hotkey path so summoning suppresses the center hover reveal.
  readonly property bool opened: panelLoader.item ? panelLoader.item.opened === true : false

  function open() {
    if (panelLoader.item && panelLoader.item.openFromHotkey) panelLoader.item.openFromHotkey()
  }

  function close() {
    if (panelLoader.item && panelLoader.item.close) panelLoader.item.close()
  }

  // Forwarded so this widget can stand in for the panel as the bar's popout
  // identity: Bar.requestPopout prefers closeForPopoutSwitch over close, and
  // KeyboardPanel reads popoutSwitchClosing back off its owner.
  readonly property bool popoutSwitchClosing: panelLoader.item ? panelLoader.item.popoutSwitchClosing === true : false

  function closeForPopoutSwitch() {
    if (panelLoader.item) panelLoader.item.closeForPopoutSwitch()
  }

  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  onBarChanged: injectPanel()
  onSettingsChanged: injectPanel()

  Loader {
    id: panelLoader
    active: true
    source: Qt.resolvedUrl("Panel.qml")
    visible: false
    onLoaded: {
      root.injectPanel()
      Qt.callLater(root.injectPanel)
    }
  }

  IpcHandler {
    target: "acabtech.omariglol"

    function open(): void { root.open() }
    function close(): void { root.close() }
    function show(): void { root.open() }
    function hide(): void { root.close() }
    function toggle(): void { root.togglePanel() }
    function refresh(): void { root.refresh() }
    function measure(): void { root.measure() }
  }

  WidgetButton {
    id: button
    anchors.fill: parent
    bar: root.bar
    text: root.barText
    labelVisible: true
    // Tooltip suppressed: the panel is the detail view.
    tooltipText: ""

    onPressed: function(b) {
      if (b === Qt.MiddleButton) root.measure()
      else if (b === Qt.RightButton) root.refresh()
      else root.togglePanel()
    }
  }
}
