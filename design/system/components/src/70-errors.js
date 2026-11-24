  // ==== part: errors ====
  // Android's one snackbar host: the line, an optional action (Retry), the close X. Material's Snackbar is a pane TalkBack names Alert, and its action is a button.
  function AndroidSnackbar(props) {
    return h('div', { className: 'bae-a-snack', role: 'alert', 'aria-label': 'Alert' }, h('span', { style: { flex: 1 } }, props.message), props.action ? h('button', { type: 'button', className: 'bae-a-snack-action' }, props.action) : null, props.close === false ? null : h('span', { className: 'bae-a-snack-x' }, h(Glyph, { name: 'close', size: 'medium' })));
  }

