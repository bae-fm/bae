  // ---- Android ----

  // A dialog or sheet among a screen's content is not content: the screen draws it last, as its own child, so its scrim covers the whole screen (bars and sheets included) and the dialog centres on the screen, however the content is wrapped.
  function splitOverlays(children) {
    var content = [], over = [];
    kids(children).forEach(function (c) {
      var t = inner(c) && inner(c).type;
      (t === AndroidDialog || t === AndroidSheet ? over : content).push(c);
    });
    return { content: content, over: over };
  }

  // A screen's bar, 64dp on the page's own colour: the back button (`back`), the title, and `menu`, a choice that applies to the whole screen (History's whose plays), at the bar's end, opening a menu of its choices.
  function AndroidTopBar(props) {
    return h('header', { className: 'bae-a-topbar' },
      props.back ? h(AndroidIconButton, { icon: 'back', label: props.backLabel || 'Back' }) : null,
      props.title ? h('h1', { className: props.back ? 'bae-a-topbar-title-after-back' : null }, props.title) : null,
      props.menu ? h('button', { type: 'button', 'aria-haspopup': 'menu', className: 'bae-a-topbar-menu' }, props.menu, h(Glyph, { name: 'down', size: 20 })) : null,
      props.more ? h('span', { className: 'bae-a-topbar-more' }, h(AndroidIconButton, { icon: 'more', label: props.more })) : null);
  }

  function AndroidScreen(props) {
    var split = splitOverlays(props.children);
    var top = [
      props.title ? h(AndroidTopBar, { key: 'top', title: props.title, back: props.back, menu: props.menu }) : null,
      h('main', { key: 'main', className: 'bae-a-main' }, split.content),
      split.over.length ? h(React.Fragment, { key: 'over' }, split.over) : null];
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

  // Each row in the card draws its own tile; `place` tells it where it sits (first, middle, last or alone) so it rounds the card's outer corners and only those. A row nested under the one above (`nested`) joins it into one piece; bundle.css finds the pair, so it holds however the rows are wrapped. Anything else a card holds sits in a tile of the card's own.
  function AndroidCard(props) {
    var list = kids(props.children);
    return h('div', { className: 'bae-a-card' }, list.map(function (c, i) {
      var place = list.length === 1 ? 'alone' : i === 0 ? 'first' : i === list.length - 1 ? 'last' : 'middle';
      var row = inner(c);
      if (row && row.type === AndroidRow) return rewrap(c, React.cloneElement(row, { place: place }));
      return h('div', { key: c && c.key != null ? c.key : i, className: cx('bae-a-card-tile', 'bae-a-card-tile-' + place) }, c);
    }));
  }

  // Material's disclosure: a row in its own card, expand_more at its end when closed and expand_less when open; the rows it shows join the card on the same text edge.
  function AndroidDisclosure(props) {
    var head = h(AndroidRow, { key: 'h', title: props.label, subtitle: props.summary, icon: props.open ? 'expand_less' : 'expand_more', expanded: !!props.open });
    return h(AndroidCard, null, [head].concat(props.open ? kids(props.children) : []));
  }
  // `place` is AndroidCard's: the row's place among the card's rows.
  function AndroidRow(props) {
    var trail = kids(props.children);
    // A row the person taps is a button, as Compose reads it, named by its words: on the whole row, or, when the row ends in controls of its own, on its words alone so a button never holds one.
    var tap = !!(props.onClick || props.icon || props.action || props.destructive || props.button);
    var press = tap ? { role: 'button', 'aria-disabled': props.off ? true : null, 'aria-expanded': props.expanded != null ? !!props.expanded : null } : {};
    var lines = h('div', Object.assign({ className: 'bae-a-row-lines' }, trail.length ? press : null),
      h('div', { className: cx('bae-a-row-title', props.action && 'bae-a-row-action', props.destructive && 'bae-a-row-destructive') }, props.title, props.tag ? h(StatusChip, null, props.tag) : null),
      props.subtitle ? h('div', { className: cx('bae-a-row-subtitle', props.tone && 'bae-tone-' + props.tone) },
        props.tone ? h('span', { className: 'bae-dot', 'aria-hidden': true }) : null, props.subtitle) : null);
    return h('div', { className: cx('bae-a-row', props.place && 'bae-a-card-tile bae-a-card-tile-' + props.place, props.nested && 'bae-a-row-nested', !props.subtitle && 'bae-a-row-one', props.selected && 'bae-a-row-selected', props.off && 'bae-off'), role: trail.length ? null : press.role, 'aria-disabled': trail.length ? null : press['aria-disabled'], 'aria-expanded': trail.length ? null : press['aria-expanded'], onClick: props.onClick },
      props.leadingIcon ? h(Material, { name: props.leadingIcon, className: cx('bae-a-row-lead', props.action && 'bae-a-row-lead-action') }) : null,
      // `avatar`: a person's name, whose picture (`avatarImage`) or initials lead the row.
      props.avatar != null ? h('span', { className: 'bae-a-row-lead bae-a-row-lead-node' }, h(Avatar, { name: props.avatar, image: props.avatarImage, size: 40 })) : null,
      lines,
      // `value`: the label's value at the row's far end, as MacRow's: “Joined · March 2026” in a member's details.
      props.value ? h('span', { className: 'bae-a-row-value' }, props.value) : null,
      trail.length ? h('span', { className: 'bae-a-row-trail' }, trail) : null,
      props.icon ? h(Material, { name: props.icon, size: props.icon === 'edit' ? 20 : 24, className: 'bae-a-row-icon' }) : null);
  }

  function AndroidButton(props) {
    var kind = props.kind || 'filled';
    return h('button', { type: 'button', disabled: !!props.off, onClick: props.onClick,
      className: cx('bae-a-btn', 'bae-a-btn-' + kind, props.icon && 'bae-a-btn-icon', props.large && 'bae-a-btn-large', props.destructive && 'bae-a-btn-destructive', props.neutral && 'bae-a-btn-neutral', props.alignTo && 'bae-a-btn-align-' + props.alignTo) },
      // `icon`: a glyph before the words, as the album page's Pin and Unpin.
      props.icon ? h(Glyph, { name: props.icon, size: 20 }) : null,
      props.children != null ? props.children : props.label);
  }

  // `tonal` is the small disc on the accent tint; `tone` colours the glyph (muted: secondary text, accent, warning, onfill: the text on the gallery's backdrop); `box` is the touch box's side in px when it isn't Material's 48. `className` is only for the caller's layout.
  function AndroidIconButton(props) {
    return h('button', { type: 'button', className: cx('bae-a-iconbtn', props.tonal && 'bae-a-iconbtn-tonal', props.tone && 'bae-a-iconbtn-' + props.tone, props.box && 'bae-a-iconbtn-box-' + props.box, props.off && 'bae-a-iconbtn-off', props.className), 'aria-label': props.label || props.icon, 'aria-haspopup': props.menu ? 'menu' : null, disabled: props.off ? true : null },
      h(Glyph, { name: props.icon || 'more', size: props.size || 24 }));
  }

  // An icon level with a line of text: it takes the line's room (20dp), and its 48dp touch area overhangs the row's padding. A notice's ×, a download's Cancel.
  function AndroidInlineIconButton(props) {
    return h('span', { className: 'bae-a-inlineicon' }, h(AndroidIconButton, { icon: props.icon || 'close', size: 20, label: props.label, tone: props.tone || 'muted' }));
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
          ? h('div', { role: 'textbox', 'aria-multiline': true, 'aria-label': props.label, 'aria-placeholder': props.placeholder || null, className: 'bae-a-input bae-a-codetext' },
              props.value ? groupCode(props.value) : h('span', { className: 'bae-codetext-placeholder', 'aria-hidden': true }, props.placeholder))
          : h('input', { className: 'bae-a-input', type: props.secret ? 'password' : 'text', defaultValue: props.value || '', placeholder: props.placeholder, 'aria-label': props.label, spellCheck: false })),
      line ? h('div', { className: 'bae-a-field-line' }, line) : null);
  }

  // A code the person reads off a receiver's screen and types: one box per digit (`digits`, which core states). The field outlines the box for the next digit in the accent while the field is `focused`, and shows each digit as the person types it. `state`: normal, focused, error, off. What's wrong goes under the boxes, in red.
  function AndroidPinField(props) {
    var state = props.state || 'normal';
    var n = props.digits || 4;
    var value = String(props.value || '');
    var isError = state === 'error';
    var cells = [];
    for (var i = 0; i < n; i++) {
      cells.push(h('span', { key: i, className: cx('bae-a-pin-cell', state === 'focused' && i === Math.min(value.length, n - 1) && 'bae-a-pin-cell-focused', isError && 'bae-a-pin-cell-error') }, value.charAt(i)));
    }
    return h('div', { className: cx('bae-a-pin', state === 'off' && 'bae-off') },
      h('div', { role: 'textbox', 'aria-label': props.label || 'PIN', 'aria-disabled': state === 'off' ? true : null, className: 'bae-a-pin-cells' }, cells),
      isError && props.error ? h('div', { className: 'bae-a-field-line bae-a-pin-note' }, props.error) : null);
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
        // `message`: the line under the title, tight to it, as Material sets a dialog's supporting text under its headline.
        props.title || props.message ? h('div', { className: 'bae-a-dialog-head' },
          props.title ? h('h2', { className: 'bae-a-dialog-title' }, props.title) : null,
          props.message ? h('p', { className: 'bae-a-dialog-message' }, props.message) : null) : null,
        props.children));
  }

  function AndroidDialogActions(props) {
    return h('div', { className: 'bae-a-dialog-actions' }, props.children);
  }

  function AndroidNote(props) {
    return h('div', { className: 'bae-a-note' }, props.children);
  }

  // Material's HorizontalDivider: a 1dp line across the screen or list that separates what's above from what's below.
  function AndroidDivider() { return h('div', { className: 'bae-a-divider', role: 'separator' }); }

  // An empty or unavailable screen's one sentence, centred in what's left of the screen, in secondary text, with no glyph above it ("No albums yet"). The children are its actions, under it.
  function AndroidContentUnavailable(props) {
    var actions = React.Children.toArray(props.children).filter(Boolean);
    return h('div', { className: 'bae-a-cu' }, h('div', { className: 'bae-a-cu-title' }, props.title), actions.length ? h('div', { className: 'bae-a-cu-actions' }, actions) : null);
  }
