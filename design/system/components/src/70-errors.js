  // ==== part: errors ====
  // Android's one snackbar host: the line, an optional action (Retry), the close X. Material's Snackbar is a pane TalkBack names Alert, and its action is a button.
  function AndroidSnackbar(props) {
    return h('div', { className: 'bae-a-snack', role: 'alert', 'aria-label': 'Alert' }, h('span', { className: 'bae-a-snack-line' }, props.message), props.action ? h(AndroidButton, { kind: 'text', inverse: true }, props.action) : null, props.close === false ? null : h('span', { className: 'bae-a-snack-x' }, h(Glyph, { name: 'close', size: 'medium' })));
  }

