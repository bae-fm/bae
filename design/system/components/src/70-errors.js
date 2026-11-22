  // ==== part: errors ====
  // Android's one snackbar host: the line, an optional action (Retry), the close X.
  function AndroidSnackbar(props) {
    return h('div', { className: 'bae-a-snack' }, h('span', { style: { flex: 1 } }, props.message), props.action ? h('span', { className: 'bae-a-snack-action' }, props.action) : null, props.close === false ? null : h('span', { className: 'bae-a-snack-x' }, h(Glyph, { name: 'close', size: 'medium' })));
  }

