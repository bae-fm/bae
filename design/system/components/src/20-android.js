  // ---- Android ----

  function AndroidScreen(props) {
    return h('div', { className: 'bae-a-screen', style: { width: Number(props.width || 412), height: Number(props.height || 916) } },
      props.title ? h('header', { className: 'bae-a-topbar' },
        props.back ? h(AndroidIconButton, { icon: 'back', label: 'Back' }) : null,
        h('h1', null, props.title)) : null,
      h('main', { className: 'bae-a-main' }, props.children));
  }

  function AndroidSection(props) {
    return h('h2', { className: 'bae-a-section' }, props.title || props.children);
  }

  function AndroidCard(props) {
    var list = kids(props.children);
    return h('div', { className: 'bae-a-card' }, divided(list));
  }

  // Material's disclosure: a row in its own card, expand_more at its end when closed and expand_less when open; the rows it shows join the card on the same text edge.
  function AndroidDisclosure(props) {
    var head = h(AndroidRow, { key: 'h', title: props.label, subtitle: props.summary }, h('span', { className: 'bae-a-disclosure-mark' }, h(MSymbol, { name: props.open ? 'expand_less' : 'expand_more', size: 24, fill: 'currentColor' })));
    return h(AndroidCard, null, [head].concat(props.open ? kids(props.children) : []));
  }
  function AndroidRow(props) {
    var trail = kids(props.children);
    var lines = h('div', { className: 'bae-a-row-lines' },
      h('div', { className: cx('bae-a-row-title', props.action && 'bae-a-row-action', props.destructive && 'bae-a-row-destructive') }, props.title, props.tag ? h(StatusChip, null, props.tag) : null),
      props.subtitle ? h('div', { className: cx('bae-a-row-subtitle', props.tone && 'bae-tone-' + props.tone) },
        props.tone ? h('span', { className: 'bae-dot', 'aria-hidden': true }) : null, props.subtitle) : null);
    return h('div', { className: cx('bae-a-row', props.nested && 'bae-a-row-nested', !props.subtitle && 'bae-a-row-one', props.selected && 'bae-a-row-selected'), role: props.onClick ? 'button' : null, onClick: props.onClick },
      props.leadingIcon ? h(Material, { name: props.leadingIcon, className: cx('bae-a-row-lead', props.action && 'bae-a-row-lead-action') }) : null,
      lines,
      trail.length ? h('span', { className: 'bae-a-row-trail' }, trail) : null,
      props.icon ? h(Material, { name: props.icon, size: props.icon === 'edit' ? 20 : 24, className: 'bae-a-row-icon' }) : null);
  }

  function AndroidButton(props) {
    var kind = props.kind || 'filled';
    return h('button', { type: 'button', disabled: !!props.off, onClick: props.onClick,
      className: cx('bae-a-btn', 'bae-a-btn-' + kind, props.large && 'bae-a-btn-large', props.destructive && 'bae-a-btn-destructive', props.alignTo && 'bae-a-btn-align-' + props.alignTo) },
      props.children != null ? props.children : props.label);
  }

  function AndroidIconButton(props) {
    return h('button', { type: 'button', className: cx('bae-a-iconbtn', props.className), 'aria-label': props.label || props.icon },
      h(Glyph, { name: props.icon || 'more', size: props.size || 24 }));
  }

  function AndroidSwitch(props) {
    var s = React.useState(!!props.on);
    var on = s[0];
    return h('button', { type: 'button', role: 'switch', 'aria-checked': on, 'aria-label': props.label, className: cx('bae-a-switch', on && 'bae-a-switch-on'), onClick: function () { s[1](!on); } },
      h('span', { className: 'bae-a-switch-thumb' }));
  }

  function AndroidTextField(props) {
    var state = props.state || 'normal';
    var isError = state === 'error';
    var line = isError ? props.error : props.helper;
    return h('div', { className: cx('bae-a-field', state === 'focused' && 'bae-a-field-focused', isError && 'bae-a-field-error') },
      h('label', { className: 'bae-a-field-box' },
        h('span', { className: 'bae-a-field-label' }, props.label),
        h('input', { className: 'bae-a-input', defaultValue: props.value || '', 'aria-label': props.label, spellCheck: false })),
      line ? h('div', { className: 'bae-a-field-line' }, line) : null);
  }

  function AndroidRadio(props) {
    return h('label', { className: 'bae-a-radio', role: 'radio', 'aria-checked': !!props.checked },
      h('span', { className: cx('bae-a-radio-dot', props.checked && 'bae-a-radio-dot-on'), 'aria-hidden': true }),
      props.label);
  }

  function AndroidDialog(props) {
    return h('div', { className: 'bae-a-scrim' },
      h('div', { role: 'dialog', 'aria-label': props.title, className: 'bae-a-dialog', style: { width: Number(props.width || 312) } },
        props.title ? h('h2', { className: 'bae-a-dialog-title' }, props.title) : null,
        props.children));
  }

  function AndroidDialogActions(props) {
    return h('div', { className: 'bae-a-dialog-actions' }, props.children);
  }

  function AndroidNote(props) {
    return h('div', { className: 'bae-a-note' }, props.children);
  }
