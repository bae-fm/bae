/* @ds-bundle: {"format":4,"namespace":"Bae","components":[{"name":"MacButton"},{"name":"MacTextField"},{"name":"MacRadio"},{"name":"MacPopUpButton"},{"name":"MacSwitch"},{"name":"MacAccentPicker"},{"name":"MacMenuItem"},{"name":"MacMenu"},{"name":"MacSegmentedControl"},{"name":"MacDisclosure"},{"name":"MacProgress"},{"name":"MacSpinner"},{"name":"MacErrorText"},{"name":"MacNote"},{"name":"MacRow"},{"name":"MacFieldRow"},{"name":"MacBox"},{"name":"MacSection"},{"name":"MacSheetFooter"},{"name":"MacSheet"},{"name":"MacAlert"},{"name":"MacSettingsWindow"},{"name":"MacWindow"},{"name":"AndroidButton"},{"name":"AndroidIconButton"},{"name":"AndroidSwitch"},{"name":"AndroidAccentPicker"},{"name":"AndroidTextField"},{"name":"AndroidRadio"},{"name":"AndroidNote"},{"name":"AndroidProgress"},{"name":"AndroidSpinner"},{"name":"AndroidRow"},{"name":"AndroidCard"},{"name":"AndroidSection"},{"name":"AndroidDialogActions"},{"name":"AndroidDialog"},{"name":"AndroidScreen"},{"name":"StatusChip"},{"name":"Notice"},{"name":"MacScanCode"},{"name":"AndroidScanCode"}]} */
(function () {
  var React = window.React;
  var h = React.createElement;

  function cx() {
    var out = [];
    for (var i = 0; i < arguments.length; i++) if (arguments[i]) out.push(arguments[i]);
    return out.join(' ');
  }

  // Drops the whitespace text a template leaves between child components.
  // Fragments a template's loops and conditions leave are opened, so a box sees each row.
  function kids(children) {
    var out = [];
    React.Children.toArray(children).forEach(function (c) {
      if (c && c.type === React.Fragment) out.push.apply(out, kids(c.props.children));
      else if (!(typeof c === 'string' && c.trim() === '')) out.push(c);
    });
    return out;
  }

  // A hairline between children.
  function divided(list) {
    var out = [];
    list.forEach(function (c, i) {
      if (i > 0 && !(c && c.props && c.props.nested)) out.push(h('div', { key: 'd' + i, className: 'bae-divider', 'aria-hidden': true }));
      out.push(c);
    });
    return out;
  }

  // A right-pointing chevron; a class turns it to point down.
  function Chevron(props) {
    return h('svg', { className: props.className, width: 7, height: 12, viewBox: '0 0 7 12', 'aria-hidden': true, fill: 'none', stroke: 'currentColor', strokeWidth: 1.6, strokeLinecap: 'round', strokeLinejoin: 'round' },
      h('path', { d: 'M1.5 1.5 5.5 6l-4 4.5' }));
  }

  // Glyphs: the few the components draw. Stroke icons for macOS, Material paths for Android.
  function CopyGlyph() {
    return h('svg', { width: 14, height: 14, viewBox: '0 0 24 24', 'aria-hidden': true, fill: 'none', stroke: 'currentColor', strokeWidth: 2, strokeLinecap: 'round', strokeLinejoin: 'round' },
      h('rect', { x: 9, y: 9, width: 11, height: 11, rx: 2 }),
      h('path', { d: 'M5 15V5a2 2 0 0 1 2-2h8' }));
  }
  function CheckGlyph() {
    return h('svg', { width: 14, height: 14, viewBox: '0 0 24 24', 'aria-hidden': true, fill: 'none', stroke: 'currentColor', strokeWidth: 2, strokeLinecap: 'round', strokeLinejoin: 'round' },
      h('path', { d: 'M5 12l5 5L20 7' }));
  }
  var MATERIAL = {
    back: 'M20 11H7.83l5.59-5.59L12 4l-8 8 8 8 1.41-1.41L7.83 13H20v-2z',
    edit: 'M3 17.25V21h3.75L17.81 9.94l-3.75-3.75L3 17.25zM20.71 7.04a1 1 0 000-1.41l-2.34-2.34a1 1 0 00-1.41 0l-1.83 1.83 3.75 3.75 1.83-1.83z',
    chevron: 'M10 6L8.59 7.41 13.17 12l-4.58 4.59L10 18l6-6z',
    expand: 'M7.41 8.59L12 13.17l4.59-4.58L18 10l-6 6-6-6 1.41-1.41z',
    more: 'M12 8a2 2 0 100-4 2 2 0 000 4zm0 2a2 2 0 100 4 2 2 0 000-4zm0 6a2 2 0 100 4 2 2 0 000-4z',
    search: 'M15.5 14h-.79l-.28-.27A6.47 6.47 0 0016 9.5 6.5 6.5 0 109.5 16c1.61 0 3.09-.59 4.23-1.57l.27.28v.79l5 4.99L20.49 19l-4.99-5zm-6 0C7.01 14 5 11.99 5 9.5S7.01 5 9.5 5 14 7.01 14 9.5 11.99 14 9.5 14z',
    add: 'M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z',
    refresh: 'M17.65 6.35A7.95 7.95 0 0012 4a8 8 0 108 8h-2a6 6 0 11-1.76-4.24L13 11h7V4l-2.35 2.35z'
  };
  function Material(props) {
    return h('svg', { width: props.size || 24, height: props.size || 24, viewBox: '0 0 24 24', 'aria-hidden': true, className: props.className },
      h('path', { d: MATERIAL[props.name] }));
  }

  // ---- macOS ----

  var PANES = ['Appearance', 'Library', 'Playback', 'Transfers', 'Import', 'Formats', 'Casting', 'Subsonic', 'Automation', 'About'];

  function MacWindow(props) {
    return h('div', { className: 'bae-m-window bae-m-window-plain', style: { width: Number(props.width || 690), height: Number(props.height || 600) } },
      h('div', { className: 'bae-m-lights', 'aria-hidden': true }, h('span', null), h('span', null), h('span', null)),
      props.children);
  }

  // A macOS alert: centred title and message, its buttons stacked full width, the default one first.
  function MacAlert(props) {
    return h('div', { className: 'bae-m-scrim' },
      h('div', { role: 'alertdialog', 'aria-label': props.title, className: 'bae-m-alert' },
        h('div', { className: 'bae-m-alert-title' }, props.title),
        props.message ? h('div', { className: 'bae-m-alert-message' }, props.message) : null,
        h('div', { className: 'bae-m-alert-buttons' }, props.children)));
  }

  function MacSettingsWindow(props) {
    return h('div', { className: 'bae-m-window', style: { width: Number(props.width || 690), height: Number(props.height || 600) } },
      h('nav', { className: 'bae-m-sidebar', 'aria-label': 'Settings panes' },
        h('div', { className: 'bae-m-lights', 'aria-hidden': true }, h('span', null), h('span', null), h('span', null)),
        PANES.map(function (p) {
          var flagged = props.status && p === props.statusPane;
          return h('button', { key: p, type: 'button', className: cx('bae-m-pane', p === props.pane && 'bae-m-pane-on') },
            h('span', null, p),
            flagged ? h(StatusChip, { tone: props.statusTone || 'warning' }, props.status) : null);
        })),
      h('main', { className: 'bae-m-main' },
        props.title ? h('div', { className: 'bae-m-heading' },
          props.back ? h('button', { type: 'button', className: 'bae-m-back', 'aria-label': 'Back to ' + props.back },
            h('svg', { width: 10, height: 10, viewBox: '0 0 24 24', 'aria-hidden': true }, h('path', { d: 'M15.41 16.59L10.83 12l4.58-4.59L14 6l-6 6 6 6 1.41-1.41z' }))) : null,
          h('h1', null, props.title)) : null,
        props.children));
  }

  function MacSection(props) {
    return h('section', { className: 'bae-m-section' },
      props.title ? h('div', { className: 'bae-m-section-title' }, props.title) : null,
      props.children);
  }

  function MacBox(props) {
    var list = kids(props.children);
    var out = divided(list);
    if (props.empty && list.length === 0) out.push(h('div', { key: 'e', className: 'bae-m-box-empty' }, props.empty));
    return h('div', { className: cx('bae-m-box', props.elevated && 'bae-m-box-elevated') }, out);
  }

  function MacRow(props) {
    var trail = kids(props.children);
    return h('div', { className: cx('bae-m-row', props.nested && 'bae-m-row-nested', props.selected && 'bae-m-row-selected', props.action && 'bae-m-row-action', props.destructive && 'bae-m-row-destructive'), onClick: props.onClick },
      props.label == null ? null : h('span', { className: 'bae-m-row-label' },
        h('span', { className: 'bae-m-row-title' }, h('span', { className: props.strong ? 'bae-m-row-strong' : null }, props.label), props.tag ? h(StatusChip, null, props.tag) : null),
        props.note ? h('span', { className: cx('bae-m-row-note', props.noteTone && 'bae-tone-' + props.noteTone) }, props.note) : null,
        props.waiting ? h('div', { className: 'bae-m-row-waiting bae-progress-track' }, h('div', { className: 'bae-progress-fill bae-progress-moving' })) : null),
      props.value != null && props.value !== '' ? h('span', { className: cx('bae-m-row-value', props.tone && 'bae-tone-' + props.tone) }, props.tone ? h('span', { className: 'bae-dot', 'aria-hidden': true }) : null, props.value) : null,
      trail.length ? h('span', { className: 'bae-m-row-trail' }, trail) : null,
      props.chevron ? h(Chevron, { className: 'bae-m-row-chevron' }) : null);
  }

  // A row of a MacBox whose label takes a column labelWidth wide and whose field fills the rest.
  function MacFieldRow(props) {
    return h('div', { className: cx('bae-m-row', 'bae-m-fieldrow', props.alignText && 'bae-m-fieldrow-aligned') },
      props.label == null ? null : h('span', { className: 'bae-m-fieldrow-label', style: { width: Number(props.labelWidth || 90) } }, props.label),
      h('div', { className: 'bae-m-fieldrow-content' }, kids(props.children)));
  }

  function MacButton(props) {
    var kind = props.kind || 'secondary';
    var rest = {
      type: 'button',
      disabled: !!props.off,
      onClick: props.onClick,
      className: cx('bae-m-btn', 'bae-m-btn-' + kind, props.size && props.size !== 'regular' && 'bae-m-btn-' + props.size, props.destructive && 'bae-m-btn-destructive')
    };
    if (kind === 'icon') {
      var icon = props.icon || 'copy';
      rest['aria-label'] = props.label || icon;
      return h('button', rest, icon === 'check' ? h(CheckGlyph) : h(CopyGlyph));
    }
    return h('button', rest, props.children != null ? props.children : props.label);
  }

  function MacTextField(props) {
    var state = props.state || 'normal';
    var hidden = !!props.labelHidden;
    var bare = hidden || props.label == null;
    var lw = props.labelWidth != null ? Number(props.labelWidth) : 90;
    var isError = state === 'error';
    var line = isError ? props.error : props.note;
    var input = h('input', {
      className: cx('bae-m-input', state === 'focused' && 'bae-m-input-focused', isError && 'bae-m-input-error'),
      type: props.secret ? 'password' : 'text',
      defaultValue: props.value || '',
      placeholder: hidden ? props.label : props.placeholder,
      'aria-label': props.label,
      disabled: state === 'off',
      spellCheck: false
    });
    return h('div', { className: cx('bae-m-field', state === 'off' && 'bae-off') },
      h('div', { className: 'bae-m-field-line' },
        bare ? null : h('span', { className: 'bae-m-field-label', style: { width: lw } }, props.label),
        input,
        props.children),
      line ? h('div', { className: cx('bae-m-field-note', isError && 'bae-m-field-note-error'), style: { paddingLeft: bare ? 0 : lw + 12 } }, line) : null);
  }

  function MacRadio(props) {
    return h('div', { className: 'bae-m-row bae-m-radio', role: 'radio', 'aria-checked': !!props.checked, onClick: props.onClick },
      h('span', { className: cx('bae-m-radio-dot', props.checked && 'bae-m-radio-dot-on'), 'aria-hidden': true }),
      h('span', { className: 'bae-m-row-label' },
        props.label,
        props.note ? h('span', { className: 'bae-m-row-note' }, props.note) : null));
  }

  function MacPopUpButton(props) {
    return h('button', { type: 'button', className: 'bae-m-popupbutton', disabled: !!props.off, style: props.width ? { width: Number(props.width) } : null },
      h('span', null, props.children != null ? props.children : props.value),
      h('svg', { className: 'bae-m-popupbutton-mark', width: 8, height: 12, viewBox: '0 0 8 12', 'aria-hidden': true, fill: 'none', stroke: 'currentColor', strokeWidth: 1.5, strokeLinecap: 'round', strokeLinejoin: 'round' }, h('path', { d: 'M1.5 4.5 4 2l2.5 2.5M1.5 7.5 4 10l2.5-2.5' })));
  }

  function MacSegmentedControl(props) {
    var options = String(props.options || '').split(',').map(function (o) { return o.trim(); }).filter(Boolean);
    return h('span', { className: 'bae-m-segmented', role: 'radiogroup' },
      options.map(function (o) {
        return h('span', { key: o, role: 'radio', 'aria-checked': o === props.selected, className: cx('bae-m-segment', o === props.selected && 'bae-m-segment-on') }, o);
      }));
  }

  function MacDisclosure(props) {
    var s = React.useState(!!props.open);
    var open = s[0];
    return h('div', { className: 'bae-m-disclosure' },
      h('button', { type: 'button', className: 'bae-m-disclosure-toggle', 'aria-expanded': open, onClick: function () { s[1](!open); } },
        h(Chevron, { className: cx('bae-m-disclosure-mark', open && 'bae-m-disclosure-mark-open') }),
        props.label || 'Advanced'),
      open ? props.children : null);
  }

  function MacSheet(props) {
    var panel = h('div', { role: 'dialog', 'aria-label': props.title, className: 'bae-m-sheet', style: { width: Number(props.width || 460) } },
      props.title ? h('div', { className: 'bae-m-sheet-title' }, props.title) : null,
      props.title ? h('div', { className: 'bae-divider', 'aria-hidden': true }) : null,
      h('div', { className: 'bae-m-sheet-body' }, props.children));
    if (!props.overlay) return panel;
    return h('div', { className: 'bae-m-scrim' }, panel);
  }

  // split sets the first child apart on the left, in its own box so a wrapper around it can't swallow the spacing.
  function MacSheetFooter(props) {
    var list = kids(props.children);
    if (!props.split || list.length < 2) return h('div', { className: 'bae-m-sheet-footer' }, list);
    return h('div', { className: 'bae-m-sheet-footer' },
      h('div', { className: 'bae-m-sheet-footer-lead' }, list[0]), list.slice(1));
  }

  function Progress(prefix) {
    return function (props) {
      var known = typeof props.value === 'number';
      return h('div', { className: prefix + '-progress' },
        props.label ? h('div', null, props.label) : null,
        h('div', { className: 'bae-progress-track', role: 'progressbar', 'aria-valuenow': known ? Math.round(props.value * 100) : undefined },
          h('div', { className: cx('bae-progress-fill', !known && 'bae-progress-moving'), style: known ? { width: (props.value * 100) + '%' } : null })));
    };
  }
  function Spinner(prefix, size) {
    return function (props) {
      var s = Number(props.size || size);
      var spin = h('svg', { className: 'bae-spinner', width: s, height: s, viewBox: '0 0 24 24', role: 'progressbar', 'aria-label': props.label || 'Loading', fill: 'none' },
        h('circle', { cx: 12, cy: 12, r: 9, stroke: 'currentColor', strokeOpacity: 0.2, strokeWidth: 2.5 }),
        h('path', { d: 'M12 3a9 9 0 0 1 9 9', stroke: 'currentColor', strokeWidth: 2.5, strokeLinecap: 'round' }));
      return props.label ? h('span', { className: prefix + '-spinner' }, spin, h('span', null, props.label)) : h('span', { className: prefix + '-spinner' }, spin);
    };
  }
  var MacSpinner = Spinner('bae-m', 16);
  var AndroidSpinner = Spinner('bae-a', 24);
  var MacProgress = Progress('bae-m');
  var AndroidProgress = Progress('bae-a');

  function MacErrorText(props) {
    return h('div', { className: 'bae-m-error' }, props.children);
  }

  function MacNote(props) {
    return h('div', { className: 'bae-m-note' }, props.children);
  }

  function MacSwitch(props) {
    var s = React.useState(!!props.on);
    var on = s[0];
    return h('button', { type: 'button', role: 'switch', 'aria-checked': on, 'aria-label': props.label, className: cx('bae-m-switch', on && 'bae-m-switch-on'), onClick: function () { s[1](!on); } },
      h('span', { className: 'bae-m-switch-knob' }));
  }

  // The accents in the order the appearance settings offer them.
  var ACCENTS = ['blue', 'indigo', 'purple', 'pink', 'red', 'amber', 'green', 'teal'];

  function accentPicker(className) {
    return function (props) {
      var s = React.useState(props.selected || 'pink');
      var chosen = s[0];
      return h('div', { className: className, role: 'radiogroup', 'aria-label': props.label || 'Accent' }, ACCENTS.map(function (a) {
        return h('button', { key: a, type: 'button', role: 'radio', 'aria-checked': a === chosen, 'aria-label': a.charAt(0).toUpperCase() + a.slice(1),
          className: cx('bae-swatch', a === chosen && 'bae-swatch-on'), style: { background: 'var(--accent-' + a + '-fill)' }, onClick: function () { s[1](a); } });
      }));
    };
  }
  var MacAccentPicker = accentPicker('bae-m-accents');
  var AndroidAccentPicker = accentPicker('bae-a-accents');

  function MacMenu(props) {
    return h('div', { className: 'bae-m-menu', role: 'menu' }, props.children);
  }

  function MacMenuItem(props) {
    return h('div', { className: cx('bae-m-menu-item', props.highlighted && 'bae-m-menu-item-on'), role: 'menuitemradio', 'aria-checked': !!props.checked },
      h('span', { className: 'bae-m-menu-check', 'aria-hidden': true }, props.checked ? h('svg', { width: 11, height: 11, viewBox: '0 0 24 24', fill: 'none', stroke: 'currentColor', strokeWidth: 2.6, strokeLinecap: 'round', strokeLinejoin: 'round' }, h('path', { d: 'M5 12.5l4.5 4.5L19 7.5' })) : null),
      props.children);
  }

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
    return h('button', { type: 'button', className: 'bae-a-iconbtn', 'aria-label': props.label || props.icon },
      h(Material, { name: props.icon || 'more' }));
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

  // ---- Both ----

  // A stand-in QR code: the three finder squares and a fixed scatter of
  // modules. The apps draw the real code (QRCode in BaeKit, QRCodeImage on
  // Android).
  var QR_N = 25;
  var QR_FILL = (function () {
    var seed = 7, out = [];
    for (var i = 0; i < QR_N * QR_N; i++) { seed = (seed * 1103515245 + 12345) & 0x7fffffff; out.push(seed % 100 < 46); }
    return out;
  })();
  function qrModule(x, y) {
    function finder(fx, fy) {
      var dx = x - fx, dy = y - fy;
      if (dx < 0 || dy < 0 || dx > 6 || dy > 6) return null;
      return dx === 0 || dy === 0 || dx === 6 || dy === 6 || (dx >= 2 && dx <= 4 && dy >= 2 && dy <= 4);
    }
    var f = finder(0, 0); if (f === null) f = finder(QR_N - 7, 0); if (f === null) f = finder(0, QR_N - 7);
    if (f !== null) return f;
    if ((x === 7 && (y < 8 || y > QR_N - 9)) || (y === 7 && (x < 8 || x > QR_N - 9)) || (x === QR_N - 8 && y < 8) || (y === QR_N - 8 && x < 8)) return false;
    return QR_FILL[y * QR_N + x];
  }
  function QrPicture(props) {
    var size = Number(props.size || 200), cells = [];
    for (var y = 0; y < QR_N; y++) for (var x = 0; x < QR_N; x++) if (qrModule(x, y)) cells.push(h('rect', { key: x + '-' + y, x: x, y: y, width: 1.02, height: 1.02 }));
    return h('div', { className: 'bae-scan-qr' },
      h('svg', { width: size, height: size, viewBox: '0 0 ' + QR_N + ' ' + QR_N, role: 'img', 'aria-label': 'QR code', shapeRendering: 'crispEdges' }, cells));
  }
  function ScanCode(prefix) {
    return function (props) {
      return h('div', { className: 'bae-scan ' + prefix + '-scan' },
        props.instruction ? h('div', { className: 'bae-scan-text' }, props.instruction) : null,
        h(QrPicture, { size: props.size }),
        h('div', { className: 'bae-scan-buttons' }, kids(props.children)),
        props.note ? h('div', { className: 'bae-scan-note' }, props.note) : null);
    };
  }
  var MacScanCode = ScanCode('bae-m');
  var AndroidScanCode = ScanCode('bae-a');

  function StatusChip(props) {
    return h('span', { className: cx('bae-chip', 'bae-tone-' + (props.tone || 'neutral')) }, props.children);
  }

  function Notice(props) {
    return h('div', { className: cx('bae-notice', 'bae-tone-' + (props.tone || 'neutral')) }, props.children);
  }

  window.Bae = Object.assign(window.Bae || {}, {
    MacWindow: MacWindow, MacAlert: MacAlert, MacSettingsWindow: MacSettingsWindow, MacSection: MacSection, MacBox: MacBox, MacRow: MacRow, MacFieldRow: MacFieldRow, MacButton: MacButton,
    MacTextField: MacTextField, MacRadio: MacRadio, MacPopUpButton: MacPopUpButton, MacSegmentedControl: MacSegmentedControl, MacDisclosure: MacDisclosure, MacSheet: MacSheet,
    MacSheetFooter: MacSheetFooter, MacProgress: MacProgress, MacSpinner: MacSpinner, MacErrorText: MacErrorText, MacNote: MacNote,
    MacSwitch: MacSwitch, MacAccentPicker: MacAccentPicker, MacMenu: MacMenu, MacMenuItem: MacMenuItem, AndroidAccentPicker: AndroidAccentPicker,
    AndroidScreen: AndroidScreen, AndroidSection: AndroidSection, AndroidCard: AndroidCard,
    AndroidRow: AndroidRow, AndroidButton: AndroidButton, AndroidIconButton: AndroidIconButton, AndroidSwitch: AndroidSwitch,
    AndroidTextField: AndroidTextField, AndroidRadio: AndroidRadio, AndroidDialog: AndroidDialog,
    AndroidDialogActions: AndroidDialogActions, AndroidNote: AndroidNote, AndroidProgress: AndroidProgress, AndroidSpinner: AndroidSpinner,
    StatusChip: StatusChip, Notice: Notice, MacScanCode: MacScanCode, AndroidScanCode: AndroidScanCode
  });
})();
