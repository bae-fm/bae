  // ==== part: errors ====
  // Android's one snackbar host: the line, an optional action (Retry), the close X.
  function AndroidSnackbar(props) {
    return h('div', { className: 'bae-a-snack' }, h('span', { style: { flex: 1 } }, props.message), props.action ? h('span', { className: 'bae-a-snack-action' }, props.action) : null, props.close === false ? null : h('span', { className: 'bae-a-snack-x' }, h(Glyph, { name: 'close', size: 20 })));
  }
  // The danger banner above the album grid when a later page failed.
  function AndroidErrorBanner(props) {
    return h('div', { className: 'bae-a-errbanner' }, h(Glyph, { name: 'warning', size: 20, className: 'bae-a-errbanner-icon' }), h('div', { style: { flex: 1 } }, h(AndroidErrorDisclosure, { tone: 'neutral', message: props.message, fault: props.fault })), h(AndroidButton, { kind: 'text' }, 'Retry'));
  }
  // macOS's notice at the bottom of the main window: the step's title, the disclosure, the advice. fill is the launch-problem one.
  function MacWindowNotice(props) {
    return h('div', { className: cx('bae-m-winnotice', props.fill && 'bae-m-winnotice-fill') }, h('div', { className: 'bae-strong', style: { fontSize: 15 } }, props.title), h(MacErrorDisclosure, { message: props.message, fault: props.fault, icon: props.fill ? false : undefined }), props.advice ? h('div', { className: 'bae-muted' }, props.advice) : null);
  }
  // BaeKit's LoadFailureView: the disclosure and Retry, centred in the view it replaces.
  function MacLoadFailure(props) {
    return h('div', { className: 'bae-m-loadfail' }, h(MacErrorDisclosure, { message: props.message, fault: props.fault }), h(MacButton, null, 'Retry'));
  }
