  // ---- Android ----

  function AndroidScreen(props) {
    var top = [
      props.title ? h('header', { key: 'top', className: 'bae-a-topbar' },
        props.back ? h(AndroidIconButton, { icon: 'back', label: 'Back' }) : null,
        h('h1', { className: props.back ? 'bae-a-topbar-title-after-back' : null }, props.title)) : null,
      h('main', { key: 'main', className: 'bae-a-main' }, props.children)];
    // keyboard: a field has focus, so the keyboard is up and the screen, a dialog over it included, fits in the space above it.
    return h('div', { className: 'bae-a-screen', style: { width: Number(props.width || 412), height: Number(props.height || 916) } },
      props.keyboard ? h(React.Fragment, null, h('div', { className: 'bae-a-screen-above' }, top), h(AndroidKeyboard)) : top);
  }

  // The on-screen keyboard, drawn plainly: three rows of letters, the bottom row with the space bar, and the gesture bar under it.
  function AndroidKeyboard() {
    function key(label, kind, grow) {
      return h('span', { key: label, className: cx('bae-a-key', kind && 'bae-a-key-' + kind), style: grow ? { flex: grow } : null }, label);
    }
    function row(keys, inset) {
      return h('div', { className: 'bae-a-keys-row', style: inset ? { padding: '0 ' + inset + 'px' } : null }, keys);
    }
    var letters = function (s) { return s.split('').map(function (c) { return key(c); }); };
    return h('div', { className: 'bae-a-keyboard', 'aria-hidden': true },
      row(letters('qwertyuiop')),
      row(letters('asdfghjkl'), 18),
      row([key('\u21e7', 'mod', 1.5)].concat(letters('zxcvbnm'), [key('\u232b', 'mod', 1.5)])),
      row([key('?123', 'mod', 1.5), key(',', 'mod'), key('', null, 5), key('.', 'mod'), key('\u21b5', 'enter', 1.5)]),
      h('div', { className: 'bae-a-keyboard-gesture' }));
  }

  function AndroidSection(props) {
    return h('h2', { className: 'bae-a-section' }, props.title || props.children);
  }

  // Each row in the card draws its own tile; `place` tells it where it sits (first, middle, last or alone) so it rounds the card's outer corners and only those. A row that opens the group's next row (`nested`) tells the one above it to join. Anything else a card holds sits in a tile of the card's own.
  function AndroidCard(props) {
    var list = kids(props.children);
    return h('div', { className: 'bae-a-card' }, list.map(function (c, i) {
      var place = list.length === 1 ? 'alone' : i === 0 ? 'first' : i === list.length - 1 ? 'last' : 'middle';
      if (c && c.type === AndroidRow) {
        var next = list[i + 1];
        return React.cloneElement(c, { place: place, joinsNext: !!(next && next.type === AndroidRow && next.props.nested) });
      }
      return h('div', { key: c && c.key != null ? c.key : i, className: cx('bae-a-card-tile', 'bae-a-card-tile-' + place) }, c);
    }));
  }

  // Material's disclosure: a row in its own card, expand_more at its end when closed and expand_less when open; the rows it shows join the card on the same text edge.
  function AndroidDisclosure(props) {
    var head = h(AndroidRow, { key: 'h', title: props.label, subtitle: props.summary }, h('span', { className: 'bae-a-disclosure-mark' }, h(MSymbol, { name: props.open ? 'expand_less' : 'expand_more', size: 24, fill: 'currentColor' })));
    return h(AndroidCard, null, [head].concat(props.open ? kids(props.children) : []));
  }
  // `place` and `joinsNext` are AndroidCard's: the row's place among the card's rows, and whether the next row is nested under it.
  function AndroidRow(props) {
    var trail = kids(props.children);
    var lines = h('div', { className: 'bae-a-row-lines' },
      h('div', { className: cx('bae-a-row-title', props.action && 'bae-a-row-action', props.destructive && 'bae-a-row-destructive') }, props.title, props.tag ? h(StatusChip, null, props.tag) : null),
      props.subtitle ? h('div', { className: cx('bae-a-row-subtitle', props.tone && 'bae-tone-' + props.tone) },
        props.tone ? h('span', { className: 'bae-dot', 'aria-hidden': true }) : null, props.subtitle) : null);
    return h('div', { className: cx('bae-a-row', props.place && 'bae-a-card-tile bae-a-card-tile-' + props.place, props.nested && 'bae-a-row-nested', props.joinsNext && 'bae-a-row-joins-next', !props.subtitle && 'bae-a-row-one', props.selected && 'bae-a-row-selected'), role: props.onClick ? 'button' : null, onClick: props.onClick },
      props.leadingIcon ? h(Material, { name: props.leadingIcon, className: cx('bae-a-row-lead', props.action && 'bae-a-row-lead-action') }) : null,
      lines,
      trail.length ? h('span', { className: 'bae-a-row-trail' }, trail) : null,
      props.icon ? h(Material, { name: props.icon, size: props.icon === 'edit' ? 20 : 24, className: 'bae-a-row-icon' }) : null);
  }

  function AndroidButton(props) {
    var kind = props.kind || 'filled';
    return h('button', { type: 'button', disabled: !!props.off, onClick: props.onClick,
      className: cx('bae-a-btn', 'bae-a-btn-' + kind, props.large && 'bae-a-btn-large', props.destructive && 'bae-a-btn-destructive', props.neutral && 'bae-a-btn-neutral', props.alignTo && 'bae-a-btn-align-' + props.alignTo) },
      props.children != null ? props.children : props.label);
  }

  // `tonal` is the small disc on the accent tint; `tone` colours the glyph (muted: secondary text, accent, warning); `box` is the touch box's side in px when it isn't Material's 48. `className` is only for the caller's layout.
  function AndroidIconButton(props) {
    return h('button', { type: 'button', className: cx('bae-a-iconbtn', props.tonal && 'bae-a-iconbtn-tonal', props.tone && 'bae-a-iconbtn-' + props.tone, props.box && 'bae-a-iconbtn-box-' + props.box, props.className), 'aria-label': props.label || props.icon },
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
        props.code
          ? h('div', { role: 'textbox', 'aria-multiline': true, 'aria-label': props.label, className: 'bae-a-input bae-a-codetext' },
              props.value ? groupCode(props.value) : h('span', { className: 'bae-codetext-placeholder' }, props.placeholder))
          : h('input', { className: 'bae-a-input', defaultValue: props.value || '', placeholder: props.placeholder, 'aria-label': props.label, spellCheck: false })),
      line ? h('div', { className: 'bae-a-field-line' }, line) : null);
  }

  // A screen that walks through one step, centred in the space above the keyboard or the screen's foot: a headline, an optional line under it, then the step's own content and its buttons, a section apart. The buttons follow what they act on rather than sitting at the bottom.
  // `top`: a step to read or fill in (a name to type, an invite that ended) sits at the top, left-aligned, as a page or form reads, its button right under it. Without it the step is centred: a step whose content is one thing to look at, such as a code.
  function AndroidStep(props) {
    return h('div', { className: cx('bae-a-step', props.top && 'bae-a-step-top') },
      h('div', { className: 'bae-a-step-head' },
        h('h2', { className: 'bae-a-step-title' }, props.title),
        props.text ? h('p', { className: 'bae-a-step-text' }, props.text) : null),
      props.children);
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
