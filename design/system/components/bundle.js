/* @ds-bundle: {"format":4,"namespace":"Bae","components":[{"name":"Button"},{"name":"TextField"},{"name":"Box"},{"name":"Row"},{"name":"Radio"},{"name":"Section"},{"name":"Sheet"},{"name":"SheetFooter"},{"name":"Disclosure"},{"name":"PopUp"},{"name":"Progress"},{"name":"ErrorText"},{"name":"Note"},{"name":"StatusChip"},{"name":"Notice"}]} */
(function () {
  var React = window.React;
  var h = React.createElement;

  function cx() {
    var out = [];
    for (var i = 0; i < arguments.length; i++) if (arguments[i]) out.push(arguments[i]);
    return out.join(' ');
  }

  // Drops the whitespace text a template leaves between child components.
  function kids(children) {
    return React.Children.toArray(children).filter(function (c) {
      return !(typeof c === 'string' && c.trim() === '');
    });
  }

  function CopyGlyph() {
    return h('svg', { width: 14, height: 14, viewBox: '0 0 24 24', 'aria-hidden': true, fill: 'none', stroke: 'currentColor', strokeWidth: 2, strokeLinecap: 'round', strokeLinejoin: 'round' },
      h('rect', { x: 9, y: 9, width: 11, height: 11, rx: 2 }),
      h('path', { d: 'M5 15V5a2 2 0 0 1 2-2h8' }));
  }

  function Button(props) {
    var kind = props.kind || 'secondary';
    var rest = {
      type: 'button',
      disabled: !!props.off,
      onClick: props.onClick,
      className: cx('bae-btn', 'bae-btn-' + kind, props.className)
    };
    if (kind === 'icon') {
      rest['aria-label'] = props.label || 'Copy';
      return h('button', rest, props.children || h(CopyGlyph));
    }
    return h('button', rest, props.children != null ? props.children : props.label);
  }

  function TextField(props) {
    var state = props.state || 'normal';
    var hidden = !!props.labelHidden;
    var lw = props.labelWidth != null ? props.labelWidth : 90;
    var isError = state === 'error';
    var line = isError ? props.error : props.note;
    var input = h('input', {
      className: cx('bae-input', state === 'focused' && 'bae-input-focused', isError && 'bae-input-error'),
      type: props.secret ? 'password' : 'text',
      defaultValue: props.value || '',
      placeholder: hidden ? props.label : props.placeholder,
      'aria-label': props.label,
      disabled: state === 'off',
      spellCheck: false
    });
    return h('div', { className: cx('bae-field', props.row && 'bae-field-row', state === 'off' && 'bae-off') },
      h('div', { className: 'bae-field-line' },
        hidden ? null : h('span', { className: 'bae-field-label', style: { width: lw } }, props.label),
        input),
      line ? h('div', { className: cx('bae-field-note', isError && 'bae-field-note-error'), style: { paddingLeft: hidden ? 0 : lw + 12 } }, line) : null);
  }

  function Box(props) {
    var list = kids(props.children);
    var out = [];
    list.forEach(function (c, i) {
      if (i > 0) out.push(h('div', { key: 'd' + i, className: 'bae-divider', 'aria-hidden': true }));
      out.push(c);
    });
    if (props.empty && list.length === 0) out.push(h('div', { key: 'e', className: 'bae-box-empty' }, props.empty));
    return h('div', { className: cx('bae-box', props.elevated && 'bae-box-elevated') }, out);
  }

  function Row(props) {
    var trail = kids(props.children);
    return h('div', { className: cx('bae-row', props.selected && 'bae-row-selected'), onClick: props.onClick },
      h('span', { className: 'bae-row-label' },
        props.label,
        props.note ? h('span', { className: 'bae-row-note' }, props.note) : null),
      props.value != null && props.value !== '' ? h('span', { className: 'bae-row-value' }, props.value) : null,
      trail.length ? h('span', { className: 'bae-row-trail' }, trail) : null,
      props.chevron ? h('span', { className: 'bae-row-chevron', 'aria-hidden': true }, '›') : null);
  }

  function Radio(props) {
    return h('div', { className: 'bae-row bae-radio', role: 'radio', 'aria-checked': !!props.checked, onClick: props.onClick },
      h('span', { className: cx('bae-radio-dot', props.checked && 'bae-radio-dot-on'), 'aria-hidden': true }),
      h('span', { className: 'bae-row-label' },
        props.label,
        props.note ? h('span', { className: 'bae-row-note' }, props.note) : null));
  }

  function Section(props) {
    return h('section', { className: 'bae-section' },
      props.title ? h('div', { className: 'bae-section-title' }, props.title) : null,
      props.children);
  }

  function Sheet(props) {
    var panel = h('div', { role: 'dialog', 'aria-label': props.title, className: 'bae-sheet', style: { width: props.width || 460 } },
      props.title ? h('div', { className: 'bae-sheet-title' }, props.title) : null,
      props.title ? h('div', { className: 'bae-divider', 'aria-hidden': true }) : null,
      h('div', { className: 'bae-sheet-body' }, props.children));
    if (!props.overlay) return panel;
    return h('div', { className: 'bae-scrim' }, panel);
  }

  function SheetFooter(props) {
    return h('div', { className: cx('bae-sheet-footer', props.split && 'bae-sheet-footer-split') }, props.children);
  }

  function Disclosure(props) {
    var s = React.useState(!!props.open);
    var open = s[0];
    return h('div', { className: 'bae-disclosure' },
      h('button', { type: 'button', className: 'bae-disclosure-toggle', 'aria-expanded': open, onClick: function () { s[1](!open); } },
        h('span', { 'aria-hidden': true, className: 'bae-disclosure-mark' }, open ? '▾' : '▸'),
        props.label || 'Advanced'),
      open ? props.children : null);
  }

  function PopUp(props) {
    return h('button', { type: 'button', className: 'bae-popup', disabled: !!props.off, style: props.width ? { width: props.width } : null },
      h('span', null, props.children != null ? props.children : props.value),
      h('span', { className: 'bae-popup-mark', 'aria-hidden': true }, '⌄'));
  }

  function Progress(props) {
    var known = typeof props.value === 'number';
    return h('div', { className: 'bae-progress' },
      props.label ? h('div', null, props.label) : null,
      h('div', { className: 'bae-progress-track', role: 'progressbar', 'aria-valuenow': known ? Math.round(props.value * 100) : undefined },
        h('div', { className: cx('bae-progress-fill', !known && 'bae-progress-moving'), style: known ? { width: (props.value * 100) + '%' } : null })));
  }

  function ErrorText(props) {
    return h('div', { className: 'bae-error' }, props.children);
  }

  function Note(props) {
    return h('div', { className: 'bae-note' }, props.children);
  }

  function StatusChip(props) {
    return h('span', { className: cx('bae-chip', 'bae-tone-' + (props.tone || 'neutral')) }, props.children);
  }

  function Notice(props) {
    return h('div', { className: cx('bae-notice', 'bae-tone-' + (props.tone || 'neutral')) }, props.children);
  }

  window.Bae = Object.assign(window.Bae || {}, {
    Button: Button, TextField: TextField, Box: Box, Row: Row, Radio: Radio, Section: Section,
    Sheet: Sheet, SheetFooter: SheetFooter, Disclosure: Disclosure, PopUp: PopUp, Progress: Progress,
    ErrorText: ErrorText, Note: Note, StatusChip: StatusChip, Notice: Notice
  });
})();
