  // ==== part: errors ====
  // Android's one snackbar host: the line, an optional action (Retry), the close X.
  function AndroidSnackbar(props) {
    return h('div', { className: 'bae-a-snack' }, h('span', { style: { flex: 1 } }, props.message), props.action ? h('span', { className: 'bae-a-snack-action' }, props.action) : null, props.close === false ? null : h('span', { className: 'bae-a-snack-x' }, h(Glyph, { name: 'close', size: 20 })));
  }
  // The error box above the album grid when a later page failed, Retry its action.
  function AndroidErrorBanner(props) {
    return h('div', { className: 'bae-a-errbanner' }, h(AndroidErrorDisclosure, { message: props.message, fault: props.fault, open: props.open, chain: props.chain, action: 'Retry' }));
  }
  // BaeKit's LoadFailureView: a view that failed whole, drawn quietly with no box, centred in the view it replaces. The message in secondary text, then one row: "Show details" (`chain` gives it causes) and Retry. Open, the causes sit under the row in the inset panel with Copy at its corner.
  function MacLoadFailure(props) {
    var panel = props.chain && props.open ? h('div', { className: 'bae-errdis-panel bae-loadfail-panel' }, h('div', { className: 'bae-errdis-chain' }, props.chain), h('span', { className: 'bae-errdis-copy', title: 'Copy details' }, h(Sym, { name: 'copy', size: 11 }))) : null;
    return h('div', { className: 'bae-m-loadfail' },
      h('div', { className: 'bae-m-loadfail-msg' }, props.message),
      h('div', { className: 'bae-m-loadfail-row' }, props.chain ? h('span', { className: 'bae-m-loadfail-link' }, props.open ? 'Hide details' : 'Show details') : null, h(MacButton, null, 'Retry')),
      panel);
  }
  // Android's LoadFailureView, the same layout in Android's type: Retry is an outlined button.
  function AndroidLoadFailure(props) {
    var panel = props.chain && props.open ? h('div', { className: 'bae-errdis-panel bae-loadfail-panel' }, h('div', { className: 'bae-errdis-chain' }, props.chain), h('span', { className: 'bae-errdis-copy' }, h(AndroidIconButton, { icon: 'copy', label: 'Copy details' }))) : null;
    return h('div', { className: 'bae-a-loadfail' },
      h('div', { className: 'bae-a-loadfail-msg' }, props.message),
      h('div', { className: 'bae-a-loadfail-row' }, props.chain ? h('span', { className: 'bae-a-loadfail-link' }, props.open ? 'Hide details' : 'Show details') : null, h(AndroidButton, { kind: 'outlined' }, 'Retry')),
      panel);
  }
