  // ==== part: errors ====
  // Android's one snackbar host: the line, an optional action (Retry), the Close button. Material's Snackbar is a pane TalkBack names Alert, and its action and close are buttons.
  function AndroidSnackbar(props) {
    var closes = props.close !== false;
    return h('div', { className: cx('bae-a-snack', closes && 'bae-a-snack-closes'), role: 'alert', 'aria-label': 'Alert' }, h('span', { className: 'bae-a-snack-line' }, props.message), props.action ? h(AndroidButton, { kind: 'text', inverse: true }, props.action) : null, closes ? h(AndroidIconButton, { icon: 'close', size: 'medium', label: 'Close', tone: 'inverse', box: 32 }) : null);
  }

